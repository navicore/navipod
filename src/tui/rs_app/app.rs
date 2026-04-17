use crate::cache_manager;
use crate::impl_tui_table_state;
use crate::k8s::cache::{DataRequest, FetchResult, PodSelector};
use crate::k8s::ds::list_daemonsets;
use crate::k8s::jobs::list_jobs;
use crate::k8s::pods::list_unowned_pods;
use crate::k8s::rs::list_replicas;
use crate::k8s::ss::list_statefulsets;
use crate::tui::common::base_table_state::BaseTableState;
use crate::tui::common::key_handler::{KeyHandlerResult, handle_common_keys};
use crate::tui::data::{Rs, RsPod};
use crate::tui::pod_app;
// use crate::tui::rs_app::ui; // Unused while testing modern UI
use crate::tui::rs_app::domain::ReplicaSetDomainService;
use crate::tui::stream::Message;
use crate::tui::style::ITEM_HEIGHT;
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{AppBehavior, Apps, create_ingress_data_vec, create_namespace_data_vec};
use crate::tui::yaml_editor::YamlEditor;
use crate::tui::{event_app, ingress_app, namespace_app};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use futures::Stream;
use ratatui::prelude::*;
use ratatui::widgets::ScrollbarState;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};

const POLL_MS: u64 = 5000;

/// Display string written to `Rs.description` for the synthesized unowned
/// row. Referenced at synthesis and at route-on-Enter; kept in a constant
/// so the two sites can't drift.
const UNOWNED_KIND: &str = "Unowned";

/// Display string written to `Rs.description` for Job rows. Kept as a
/// constant so synthesis, routing, and the YAML view arm stay in sync.
const JOB_KIND: &str = "Job";

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) base: BaseTableState<Rs>,
    /// Track network activity for UI indicator
    pub(crate) has_network_activity: bool,
    /// Track blocking activity (cache misses) for red spinner
    pub(crate) has_blocking_activity: bool,
}

impl_tui_table_state!(App, Rs);

impl AppBehavior for App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        // Handle YAML editor events first if editor is active
        if self.base.yaml_editor.is_active {
            return self.handle_yaml_editor_event(event);
        }

        if self.get_show_filter_edit() {
            Ok(self.handle_filter_edit_event(event))
        } else {
            self.handle_table_event(event).await
        }
    }

    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal
            .draw(|f| super::modern_ui::ui(f, self))
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn stream(&self, should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message> {
        let (tx, rx) = mpsc::channel(1);
        let initial_items = self.get_items().to_vec();

        tokio::spawn(async move {
            let cache = cache_manager::get_cache_or_default();
            let namespace = cache_manager::get_current_namespace_or_default();
            let rs_request = DataRequest::ReplicaSets {
                namespace: Some(namespace.clone()),
                labels: std::collections::BTreeMap::new(),
            };
            let ds_request = DataRequest::DaemonSets {
                namespace: Some(namespace.clone()),
                labels: std::collections::BTreeMap::new(),
            };
            let ss_request = DataRequest::StatefulSets {
                namespace: Some(namespace.clone()),
                labels: std::collections::BTreeMap::new(),
            };
            let job_request = DataRequest::Jobs {
                namespace: Some(namespace.clone()),
                labels: std::collections::BTreeMap::new(),
            };
            let unowned_request = DataRequest::Pods {
                namespace,
                selector: PodSelector::Unowned,
            };

            // Subscribe to cache updates for all workload kinds so we
            // re-emit a merged list when any changes.
            let (rs_sub_id, mut rs_rx) = cache
                .subscription_manager
                .subscribe("rs:*".to_string())
                .await;
            let (ds_sub_id, mut ds_rx) = cache
                .subscription_manager
                .subscribe("ds:*".to_string())
                .await;
            let (ss_sub_id, mut ss_rx) = cache
                .subscription_manager
                .subscribe("ss:*".to_string())
                .await;
            let (job_sub_id, mut job_rx) = cache
                .subscription_manager
                .subscribe("job:*".to_string())
                .await;
            let (pods_sub_id, mut pods_rx) = cache
                .subscription_manager
                .subscribe("pods:*".to_string())
                .await;

            let mut last_sent = initial_items;

            // Send an initial merged view from cache (or a direct fetch if
            // all caches are empty — e.g. right after a namespace switch).
            {
                let rs_items = match cache.get(&rs_request).await {
                    Some(FetchResult::ReplicaSets(v)) => v,
                    _ => Vec::new(),
                };
                let ds_items = match cache.get(&ds_request).await {
                    Some(FetchResult::DaemonSets(v)) => v,
                    _ => Vec::new(),
                };
                let ss_items = match cache.get(&ss_request).await {
                    Some(FetchResult::StatefulSets(v)) => v,
                    _ => Vec::new(),
                };
                let job_items = match cache.get(&job_request).await {
                    Some(FetchResult::Jobs(v)) => v,
                    _ => Vec::new(),
                };
                let unowned_items = match cache.get(&unowned_request).await {
                    Some(FetchResult::Pods(v)) => v,
                    _ => Vec::new(),
                };

                let mut merged = if rs_items.is_empty()
                    && ds_items.is_empty()
                    && ss_items.is_empty()
                    && job_items.is_empty()
                    && unowned_items.is_empty()
                {
                    debug!(
                        "Workloads stream: cold cache, fetching RS+DS+SS+Jobs+Unowned in parallel"
                    );
                    cache_manager::start_blocking_operation();
                    let (rs_res, ds_res, ss_res, job_res, unowned_res) = tokio::join!(
                        list_replicas(),
                        list_daemonsets(),
                        list_statefulsets(),
                        list_jobs(),
                        list_unowned_pods()
                    );
                    cache_manager::end_blocking_operation();
                    let rs_fetch = rs_res.unwrap_or_default();
                    let ds_fetch = ds_res.unwrap_or_default();
                    let ss_fetch = ss_res.unwrap_or_default();
                    let job_fetch = job_res.unwrap_or_default();
                    let unowned_fetch = unowned_res.unwrap_or_default();

                    if !rs_fetch.is_empty() {
                        let _ = cache
                            .put(&rs_request, FetchResult::ReplicaSets(rs_fetch.clone()))
                            .await;
                        ReplicaSetDomainService::trigger_pod_prefetch(&rs_fetch, "IMMEDIATE").await;
                    }
                    if !ds_fetch.is_empty() {
                        let _ = cache
                            .put(&ds_request, FetchResult::DaemonSets(ds_fetch.clone()))
                            .await;
                        ReplicaSetDomainService::trigger_pod_prefetch(&ds_fetch, "IMMEDIATE").await;
                    }
                    if !ss_fetch.is_empty() {
                        let _ = cache
                            .put(&ss_request, FetchResult::StatefulSets(ss_fetch.clone()))
                            .await;
                        ReplicaSetDomainService::trigger_pod_prefetch(&ss_fetch, "IMMEDIATE").await;
                    }
                    if !job_fetch.is_empty() {
                        let _ = cache
                            .put(&job_request, FetchResult::Jobs(job_fetch.clone()))
                            .await;
                    }
                    if !unowned_fetch.is_empty() {
                        let _ = cache
                            .put(&unowned_request, FetchResult::Pods(unowned_fetch.clone()))
                            .await;
                    }

                    merge_workloads(rs_fetch, ds_fetch, ss_fetch, job_fetch, &unowned_fetch)
                } else {
                    merge_workloads(rs_items, ds_items, ss_items, job_items, &unowned_items)
                };

                if !merged.is_empty() && merged != last_sent {
                    if tx.send(Message::Rs(merged.clone())).await.is_err() {
                        cache.subscription_manager.unsubscribe(&rs_sub_id).await;
                        cache.subscription_manager.unsubscribe(&ds_sub_id).await;
                        cache.subscription_manager.unsubscribe(&ss_sub_id).await;
                        cache.subscription_manager.unsubscribe(&job_sub_id).await;
                        cache.subscription_manager.unsubscribe(&pods_sub_id).await;
                        return;
                    }
                    std::mem::swap(&mut last_sent, &mut merged);
                }
            }

            while !should_stop.load(Ordering::Relaxed) {
                let mut refresh = false;

                tokio::select! {
                    update = rs_rx.recv() => {
                        if let Some(crate::k8s::cache::DataUpdate::ReplicaSets(v)) = update {
                            ReplicaSetDomainService::trigger_pod_prefetch(&v, "UPDATE").await;
                            refresh = true;
                        }
                    }
                    update = ds_rx.recv() => {
                        if let Some(crate::k8s::cache::DataUpdate::DaemonSets(v)) = update {
                            ReplicaSetDomainService::trigger_pod_prefetch(&v, "UPDATE").await;
                            refresh = true;
                        }
                    }
                    update = ss_rx.recv() => {
                        if let Some(crate::k8s::cache::DataUpdate::StatefulSets(v)) = update {
                            ReplicaSetDomainService::trigger_pod_prefetch(&v, "UPDATE").await;
                            refresh = true;
                        }
                    }
                    update = job_rx.recv() => {
                        // Job updates don't drive pod prefetch the same way —
                        // Jobs don't have a label selector for us to use.
                        if let Some(crate::k8s::cache::DataUpdate::Jobs(_)) = update {
                            refresh = true;
                        }
                    }
                    update = pods_rx.recv() => {
                        // Any pod cache update may change the Unowned count. We
                        // don't care which selector fired — just re-merge.
                        if update.is_some() {
                            refresh = true;
                        }
                    }
                    () = sleep(Duration::from_millis(POLL_MS)) => {
                        refresh = true;
                    }
                }

                if !refresh {
                    continue;
                }

                let rs_items = if let Some(FetchResult::ReplicaSets(v)) =
                    cache.get(&rs_request).await
                {
                    v
                } else {
                    warn!("🔴 CACHE MISS: ReplicaSets, calling K8s API (blocking)");
                    cache_manager::start_blocking_operation();
                    let api = list_replicas().await;
                    cache_manager::end_blocking_operation();
                    if let Ok(v) = api {
                        if !v.is_empty() {
                            let _ = cache
                                .put(&rs_request, FetchResult::ReplicaSets(v.clone()))
                                .await;
                            ReplicaSetDomainService::trigger_pod_prefetch(&v, "IMMEDIATE").await;
                        }
                        v
                    } else if let Some(FetchResult::ReplicaSets(v)) =
                        cache.get_or_mark_stale(&rs_request).await
                    {
                        v
                    } else {
                        Vec::new()
                    }
                };

                let ds_items = if let Some(FetchResult::DaemonSets(v)) =
                    cache.get(&ds_request).await
                {
                    v
                } else {
                    warn!("🔴 CACHE MISS: DaemonSets, calling K8s API (blocking)");
                    cache_manager::start_blocking_operation();
                    let api = list_daemonsets().await;
                    cache_manager::end_blocking_operation();
                    if let Ok(v) = api {
                        if !v.is_empty() {
                            let _ = cache
                                .put(&ds_request, FetchResult::DaemonSets(v.clone()))
                                .await;
                            ReplicaSetDomainService::trigger_pod_prefetch(&v, "IMMEDIATE").await;
                        }
                        v
                    } else if let Some(FetchResult::DaemonSets(v)) =
                        cache.get_or_mark_stale(&ds_request).await
                    {
                        v
                    } else {
                        Vec::new()
                    }
                };

                let ss_items = if let Some(FetchResult::StatefulSets(v)) =
                    cache.get(&ss_request).await
                {
                    v
                } else {
                    warn!("🔴 CACHE MISS: StatefulSets, calling K8s API (blocking)");
                    cache_manager::start_blocking_operation();
                    let api = list_statefulsets().await;
                    cache_manager::end_blocking_operation();
                    if let Ok(v) = api {
                        if !v.is_empty() {
                            let _ = cache
                                .put(&ss_request, FetchResult::StatefulSets(v.clone()))
                                .await;
                            ReplicaSetDomainService::trigger_pod_prefetch(&v, "IMMEDIATE").await;
                        }
                        v
                    } else if let Some(FetchResult::StatefulSets(v)) =
                        cache.get_or_mark_stale(&ss_request).await
                    {
                        v
                    } else {
                        Vec::new()
                    }
                };

                let job_items = if let Some(FetchResult::Jobs(v)) = cache.get(&job_request).await {
                    v
                } else {
                    // Cache miss for Jobs: many namespaces have none, so
                    // this rarely runs. Blocking matches the other arms.
                    debug!("Workloads stream: Jobs cache miss, direct fetch");
                    cache_manager::start_blocking_operation();
                    let api = list_jobs().await;
                    cache_manager::end_blocking_operation();
                    if let Ok(v) = api {
                        if !v.is_empty() {
                            let _ = cache.put(&job_request, FetchResult::Jobs(v.clone())).await;
                        }
                        v
                    } else if let Some(FetchResult::Jobs(v)) =
                        cache.get_or_mark_stale(&job_request).await
                    {
                        v
                    } else {
                        Vec::new()
                    }
                };

                let unowned_items =
                    if let Some(FetchResult::Pods(v)) = cache.get(&unowned_request).await {
                        v
                    } else {
                        // Cache miss for Unowned: most clusters have zero, so
                        // this path rarely runs. Still blocking like RS/DS/SS
                        // to keep the landing consistent.
                        debug!("Workloads stream: Unowned cache miss, direct fetch");
                        cache_manager::start_blocking_operation();
                        let api = list_unowned_pods().await;
                        cache_manager::end_blocking_operation();
                        if let Ok(v) = api {
                            if !v.is_empty() {
                                let _ = cache
                                    .put(&unowned_request, FetchResult::Pods(v.clone()))
                                    .await;
                            }
                            v
                        } else if let Some(FetchResult::Pods(v)) =
                            cache.get_or_mark_stale(&unowned_request).await
                        {
                            v
                        } else {
                            Vec::new()
                        }
                    };

                let mut merged =
                    merge_workloads(rs_items, ds_items, ss_items, job_items, &unowned_items);
                if merged != last_sent && tx.send(Message::Rs(merged.clone())).await.is_err() {
                    break;
                }
                std::mem::swap(&mut last_sent, &mut merged);
            }

            cache.subscription_manager.unsubscribe(&rs_sub_id).await;
            cache.subscription_manager.unsubscribe(&ds_sub_id).await;
            cache.subscription_manager.unsubscribe(&ss_sub_id).await;
            cache.subscription_manager.unsubscribe(&job_sub_id).await;
            cache.subscription_manager.unsubscribe(&pods_sub_id).await;
        });

        ReceiverStream::new(rx)
    }
}

/// Combine `ReplicaSet`, `DaemonSet`, `StatefulSet`, Job, and unowned-pod
/// rows into a single workload list sorted by kind then name, so the
/// landing has a stable order regardless of which cache updated most
/// recently.
///
/// Jobs are already filtered to `status.active > 0` upstream in
/// `list_jobs`. Unowned pods are collapsed into a single synthetic row
/// with `description = "Unowned"` and a pod count; the row is omitted if
/// there are no unowned pods.
fn merge_workloads(
    rs: Vec<Rs>,
    ds: Vec<Rs>,
    ss: Vec<Rs>,
    jobs: Vec<Rs>,
    unowned: &[RsPod],
) -> Vec<Rs> {
    let mut merged = Vec::with_capacity(rs.len() + ds.len() + ss.len() + jobs.len() + 1);
    merged.extend(rs);
    merged.extend(ds);
    merged.extend(ss);
    merged.extend(jobs);
    if let Some(row) = synthesize_unowned_row(unowned) {
        merged.push(row);
    }
    merged.sort_by(|a, b| a.description.cmp(&b.description).then(a.name.cmp(&b.name)));
    merged
}

/// Collapse the unowned-pod list into a single landing row, or `None` when
/// the list is empty (so the row doesn't appear on clusters where every pod
/// has an owner controller).
///
/// `age` is intentionally blank: the row aggregates a heterogeneous set of
/// pods (e.g. kube-apiserver, etcd, scheduler on kubeadm), so there is no
/// single meaningful creation time. Per-pod ages are visible one `Enter`
/// deeper in `pod_app`.
fn synthesize_unowned_row(unowned: &[RsPod]) -> Option<Rs> {
    if unowned.is_empty() {
        return None;
    }
    let count = unowned.len();
    Some(Rs {
        name: "(unowned)".to_string(),
        owner: String::new(),
        description: UNOWNED_KIND.to_string(),
        age: String::new(),
        pods: format!("{count}"),
        selectors: None,
        events: Vec::new(),
    })
}

impl App {
    pub fn new(data_vec: Vec<Rs>) -> Self {
        Self {
            base: BaseTableState::new(data_vec),
            has_network_activity: false,
            has_blocking_activity: false,
        }
    }

    /// Update activity status for UI indicator
    pub fn update_activity_status(&mut self) {
        self.has_network_activity = cache_manager::has_network_activity();
        self.has_blocking_activity = cache_manager::has_blocking_activity();
    }

    /// Get current network activity status
    pub const fn get_network_activity(&self) -> bool {
        self.has_network_activity
    }

    /// Get current blocking activity status (cache misses - should be red!)
    pub const fn get_blocking_activity(&self) -> bool {
        self.has_blocking_activity
    }

    fn handle_filter_edit_event(&mut self, event: &Message) -> Option<Apps> {
        let mut app_holder = Some(Apps::Rs { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Backspace, Char, Enter, Esc, Left, Right};

                    match key.code {
                        Char(to_insert) => {
                            self.enter_char(to_insert);
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Backspace => {
                            self.delete_char();
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Left => {
                            self.move_cursor_left();
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Right => {
                            self.move_cursor_right();
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Esc | Enter => {
                            self.set_show_filter_edit(false);
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        _ => {}
                    }
                }
            }
            Message::Rs(data_vec) => {
                debug!("updating rs app data...");
                let mut new_app = self.clone();
                new_app.base.items.clone_from(data_vec);
                new_app.base.scroll_state =
                    ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
                let new_app_holder = Apps::Rs { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        app_holder
    }

    async fn handle_table_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    // First try common keys (navigation, quit, color, vim motions)
                    return match handle_common_keys(self, key, |app| Apps::Rs { app }) {
                        KeyHandlerResult::Quit => Ok(None),
                        KeyHandlerResult::HandledWithUpdate(app_holder)
                        | KeyHandlerResult::Handled(app_holder) => Ok(app_holder),
                        KeyHandlerResult::NotHandled => {
                            // Handle RS-specific keys
                            self.handle_rs_specific_keys(key).await
                        }
                    };
                }
                Ok(Some(Apps::Rs { app: self.clone() }))
            }
            Message::Rs(data_vec) => Ok(Some(self.handle_data_update(data_vec))),
            _ => Ok(Some(Apps::Rs { app: self.clone() })),
        }
    }

    /// Handle RS-specific key events that aren't covered by common key handler
    async fn handle_rs_specific_keys(
        &mut self,
        key: &crossterm::event::KeyEvent,
    ) -> Result<Option<Apps>, io::Error> {
        use KeyCode::{Char, Enter};

        match key.code {
            Char('e') => {
                debug!("changing app from rs to event...");
                Ok(Some(Self::handle_switch_to_events()))
            }
            Char('i' | 'I') => self.handle_switch_to_ingress().await,
            Char('n') => self.handle_switch_to_namespace().await,
            Enter => Ok(Some(self.handle_switch_to_pods())),
            Char('/') => Ok(Some(self.handle_filter_mode())),
            Char('y' | 'Y') => Ok(Some(self.handle_yaml_view())),
            _ => Ok(Some(Apps::Rs { app: self.clone() })),
        }
    }

    /// Handle data update message
    fn handle_data_update(&self, data_vec: &[Rs]) -> Apps {
        debug!("updating rs app data...");
        let mut new_app = self.clone();
        new_app.base.items = data_vec.to_vec();
        new_app.base.scroll_state =
            ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);

        // Update network activity status for UI indicator
        new_app.update_activity_status();

        Apps::Rs { app: new_app }
    }

    /// Switch to Events app
    fn handle_switch_to_events() -> Apps {
        Apps::Event {
            app: event_app::app::App::new(),
        }
    }

    /// Switch to Ingress app
    async fn handle_switch_to_ingress(&mut self) -> Result<Option<Apps>, io::Error> {
        if let Some(selection) = self.get_selected_item()
            && let Some(selector) = selection.selectors.clone()
        {
            let data_vec = create_ingress_data_vec(selector.clone()).await?;
            debug!("changing app from rs to ingress...");
            return Ok(Some(Apps::Ingress {
                app: ingress_app::app::App::new(data_vec),
            }));
        }
        Ok(Some(Apps::Rs { app: self.clone() }))
    }

    /// Switch to Namespace picker app
    async fn handle_switch_to_namespace(&self) -> Result<Option<Apps>, io::Error> {
        debug!("changing app from rs to namespace...");
        let data_vec = create_namespace_data_vec().await?;
        debug!("namespace picker received {} namespaces", data_vec.len());
        Ok(Some(Apps::Namespace {
            app: namespace_app::app::App::new(data_vec),
        }))
    }

    /// Switch to Pods app. Routing depends on the selected row's kind:
    /// `Unowned` → `PodSelector::Unowned` (predicate, no selector);
    /// `Job` → `PodSelector::ByJob(name)` (owner-reference walk, labels
    /// don't identify Job pods); everything else (RS/DS/SS) → labels.
    fn handle_switch_to_pods(&mut self) -> Apps {
        if let Some(selection) = self.get_selected_item() {
            if selection.description == UNOWNED_KIND {
                debug!("changing app from rs to pod (unowned)...");
                return Apps::Pod {
                    app: pod_app::app::App::new(PodSelector::Unowned, Vec::new()),
                };
            }
            if selection.description == JOB_KIND {
                debug!("changing app from rs to pod (job={})...", selection.name);
                return Apps::Pod {
                    app: pod_app::app::App::new(
                        PodSelector::ByJob(selection.name.clone()),
                        Vec::new(),
                    ),
                };
            }
            if let Some(selectors) = selection.selectors.clone() {
                debug!("changing app from rs to pod...");
                return Apps::Pod {
                    app: pod_app::app::App::new(PodSelector::ByLabels(selectors), Vec::new()),
                };
            }
        }
        Apps::Rs { app: self.clone() }
    }

    /// Enter filter editing mode
    fn handle_filter_mode(&mut self) -> Apps {
        self.set_show_filter_edit(true);
        Apps::Rs { app: self.clone() }
    }

    /// View YAML for selected workload row (`ReplicaSet`, `DaemonSet`, or `StatefulSet`).
    fn handle_yaml_view(&mut self) -> Apps {
        if let Some(selection) = self.get_selected_item() {
            // `description` carries the workload kind for merged rows.
            let resource_type = match selection.description.as_str() {
                "DaemonSet" => "daemonset",
                "StatefulSet" => "statefulset",
                "Job" => "job",
                _ => "replicaset",
            }
            .to_string();
            self.base.yaml_editor = YamlEditor::new(
                resource_type,
                selection.name.clone(),
                Some(cache_manager::get_current_namespace_or_default()),
            );
            if let Err(e) = self.base.yaml_editor.fetch_yaml() {
                debug!("Error fetching YAML: {}", e);
            }
        }
        Apps::Rs { app: self.clone() }
    }

    pub const fn set_cursor_pos(&mut self, cursor_pos: usize) {
        self.base.edit_filter_cursor_position = cursor_pos;
    }
    pub const fn get_cursor_pos(&self) -> usize {
        self.base.edit_filter_cursor_position
    }

    pub const fn set_show_filter_edit(&mut self, show_filter_edit: bool) {
        self.base.show_filter_edit = show_filter_edit;
    }
    pub const fn get_show_filter_edit(&self) -> bool {
        self.base.show_filter_edit
    }

    pub fn get_event_details(&mut self) -> Vec<(String, String, Option<String>)> {
        self.get_selected_item().map_or_else(Vec::new, |pod| {
            pod.events
                .iter()
                .map(|event| {
                    (
                        event.type_.clone(),
                        event.message.clone(),
                        Some(event.age.clone()),
                    )
                })
                .collect()
        })
    }

    pub fn get_left_details(&mut self) -> Vec<(String, String, Option<String>)> {
        self.get_selected_item().map_or_else(Vec::new, |pod| {
            pod.selectors.clone().map_or_else(Vec::new, |labels| {
                let mut r = Vec::new();
                for (name, value) in &labels {
                    r.push((name.clone(), value.clone(), None));
                }
                r
            })
        })
    }

    /// Handle YAML editor events
    fn handle_yaml_editor_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        if let Message::Key(Event::Key(key)) = event
            && key.kind == KeyEventKind::Press
        {
            use KeyCode::{Char, Down, Esc, Up};

            match key.code {
                Char('q') | Esc => {
                    // Close YAML editor
                    self.base.yaml_editor.close();
                }
                Char('r' | 'R') => {
                    // Refresh YAML content
                    self.base.yaml_editor.fetch_yaml()?;
                }
                // Removed mode switching - now read-only viewer only
                Up | Char('k') => {
                    // Scroll up (vim-like navigation)
                    self.base.yaml_editor.scroll_up(3);
                }
                Down | Char('j') => {
                    // Scroll down (vim-like navigation)
                    self.base.yaml_editor.scroll_down(3, None); // Use dynamic height calculation
                }
                Char('G') => {
                    // Jump to bottom (vim motion)
                    self.base.yaml_editor.jump_to_bottom(None); // Use dynamic height calculation
                }
                Char('g') => {
                    // Jump to top (vim motion)
                    self.base.yaml_editor.jump_to_top();
                }
                _ => {}
            }
        }

        Ok(Some(Apps::Rs { app: self.clone() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rs(name: &str, kind: &str) -> Rs {
        Rs {
            name: name.to_string(),
            owner: String::new(),
            description: kind.to_string(),
            age: String::new(),
            pods: String::new(),
            selectors: None,
            events: Vec::new(),
        }
    }

    #[test]
    fn merge_workloads_sorts_by_kind_then_name() {
        let rs_list = vec![rs("web", "ReplicaSet"), rs("api", "ReplicaSet")];
        let ds_list = vec![rs("kube-proxy", "DaemonSet")];
        let ss_list = vec![rs("kafka", "StatefulSet"), rs("etcd", "StatefulSet")];

        let merged = merge_workloads(rs_list, ds_list, ss_list, vec![], &[]);

        let order: Vec<(&str, &str)> = merged
            .iter()
            .map(|r| (r.description.as_str(), r.name.as_str()))
            .collect();

        // DaemonSet < Job < ReplicaSet < StatefulSet (lex order on description),
        // then by name within each group. No Unowned row when list is empty.
        assert_eq!(
            order,
            vec![
                ("DaemonSet", "kube-proxy"),
                ("ReplicaSet", "api"),
                ("ReplicaSet", "web"),
                ("StatefulSet", "etcd"),
                ("StatefulSet", "kafka"),
            ]
        );
    }

    #[test]
    fn merge_workloads_sorts_jobs_between_daemonset_and_replicaset() {
        let merged = merge_workloads(
            vec![rs("web", "ReplicaSet")],
            vec![rs("kube-proxy", "DaemonSet")],
            vec![rs("etcd", "StatefulSet")],
            vec![rs("backup-nightly", "Job"), rs("apply-config", "Job")],
            &[],
        );

        let order: Vec<(&str, &str)> = merged
            .iter()
            .map(|r| (r.description.as_str(), r.name.as_str()))
            .collect();

        assert_eq!(
            order,
            vec![
                ("DaemonSet", "kube-proxy"),
                ("Job", "apply-config"),
                ("Job", "backup-nightly"),
                ("ReplicaSet", "web"),
                ("StatefulSet", "etcd"),
            ]
        );
    }

    fn pod(name: &str, kind: &str) -> RsPod {
        RsPod {
            name: name.to_string(),
            status: "Running".to_string(),
            description: kind.to_string(),
            age: String::new(),
            containers: "1/1".to_string(),
            selectors: None,
            events: Vec::new(),
            cpu_request: None,
            cpu_limit: None,
            cpu_usage: None,
            memory_request: None,
            memory_limit: None,
            memory_usage: None,
            node_name: None,
            node_cpu_percent: None,
            node_memory_percent: None,
        }
    }

    #[test]
    fn merge_workloads_synthesizes_single_unowned_row_sorted_last() {
        let rs_list = vec![rs("web", "ReplicaSet")];
        let ds_list = vec![rs("kube-proxy", "DaemonSet")];
        let ss_list: Vec<Rs> = vec![];
        let unowned = vec![
            pod("kube-apiserver-node1", "StaticPod"),
            pod("kube-scheduler-node1", "StaticPod"),
        ];

        let merged = merge_workloads(rs_list, ds_list, ss_list, vec![], &unowned);

        let last = merged.last().expect("unowned row should be present");
        assert_eq!(last.description, "Unowned");
        assert_eq!(last.name, "(unowned)");
        assert_eq!(last.pods, "2");
        assert_eq!(
            merged.iter().filter(|r| r.description == "Unowned").count(),
            1,
            "exactly one synthesized Unowned row"
        );
    }

    #[test]
    fn merge_workloads_omits_unowned_row_when_empty() {
        let merged = merge_workloads(vec![rs("web", "ReplicaSet")], vec![], vec![], vec![], &[]);
        assert!(merged.iter().all(|r| r.description != "Unowned"));
    }
}
