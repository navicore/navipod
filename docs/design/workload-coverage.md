# Non-ReplicaSet workload coverage

## Intent

Today the TUI tree is rooted at ReplicaSet (`rs_app` is the landing page,
pods are only reachable via an RS's label selector). In real clusters —
especially control planes like `kind`'s `kube-system` — most pods are not
owned by a ReplicaSet: DaemonSets (kube-proxy, kindnet), StatefulSets,
Jobs/CronJobs, and static pods (kube-apiserver, kube-scheduler,
kube-controller-manager, etcd) are invisible to navipod. We want those
pods to be inspectable with the same container/probe/log flow we already
have — without breaking the RS-centric flow that works for app workloads.

## Constraints

- **Must not break the RS-first flow.** The Deployment→RS→Pod path is
  what users expect for app workloads and it's what the background
  cache/watch is tuned for.
- **Must not require a second cluster round-trip per pod.** Owner-kind
  classification should fall out of the pod list we already fetch
  (`pod.metadata.owner_references`) — not a separate per-pod API call.
- **Cache key shape stays compatible.** Existing cache entries and
  `DataRequest` variants can be extended but shouldn't be renamed in a
  way that invalidates watches mid-session.
- **Out of scope:** namespace-wide "show everything" tree views,
  cross-workload grouping (e.g. "all things labelled app=foo"),
  CronJob-schedule visualization, HPA/PDB.

## Approach

Prefer an **additive** change over a refactor to a generalized
"Workloads" root. Two pieces:

1. **Extend the RS landing into a Workloads landing.** Same view, same
   keys — but the table shows rows for ReplicaSets, DaemonSets,
   StatefulSets, Jobs, and CronJobs, plus a single synthetic **Unowned
   leaf**. A `Kind` column distinguishes them. Selector semantics for
   `Enter → Pods` branch on kind: RS/DS/SS use `spec.selector`;
   Job routes through `owner_references`; CronJob fans out to its
   child Jobs (open: whether CronJob row jumps straight to pods of the
   latest Job or lists Jobs first — default to the former, revisit if
   users need historical-run inspection).
2. **Unowned is a leaf, not a workload row.** Selecting the Unowned
   entry goes directly to `pod_app` filtered to pods with no
   controller-kind owner (`owner_references` empty or `kind=Node`).
   No intermediate "workload" abstraction — static pods are
   singletons; the extra hop would be noise.
3. **New `DataRequest` variants and watches.** `DaemonSets`,
   `StatefulSets`, `Jobs`, `CronJobs` each get fetcher +
   watch-manager coverage with their own TTLs in `cache/config.rs`.
   Events watching stays cluster-wide.
4. **Bias Job/CronJob views to currently-active work.** What matters
   operationally is Jobs running *right now* (they consume cluster
   resources); recently-completed Jobs are not critical to surface.
   Default filter: `status.active > 0` for Jobs in the landing
   and in CronJob→pods routing. Completed Jobs can be revealed via
   the existing filter key (`/`) but are hidden by default. This
   also sidesteps the CronJob watch-noise problem: most of the churn
   is transition-to-complete, which we don't render.

Where possible, share projection code: `Rs` becomes one case of a
`Workload` enum (or a struct with a `kind` tag), and `pod_app` keeps its
current shape because pods-under-a-workload is still a flat list.

## Domain Events

- **Produced:**
  - `DataRequest::{DaemonSets, StatefulSets, Jobs, CronJobs}` — new
    fetchers, new cache-key prefixes (`ds:`, `ss:`, `job:`, `cj:`).
  - New `WatchedResource` variants with their own backoff/restart
    loops; watches for Jobs/CronJobs especially need sane backoff
    because CronJobs churn.
  - UI event: selecting a workload row routes
    `handle_switch_to_pods` with a kind-tagged selector;
    CronJob selection resolves to "pods of latest Job" before routing.
  - UI event: selecting the Unowned leaf goes directly to `pod_app`
    with a filter predicate, not a selector.
- **Consumed:**
  - `Pod` fetches already populate `owner_references`; the
    workload landing uses that to build the Unowned leaf's count
    without an extra call.
  - Namespace switching (`cache_manager::switch_namespace`) must
    restart the new watches too, not just the existing three.

## Delivery Slices

Ordered by increasing novelty and user value. Each slice is independently
shippable and leaves the tree in a working state.

- **Slice 1 — DaemonSets.** ✅ *Merged.* Typed `Api<DaemonSet>`, reuses
  `Rs` row shape with `description = "DaemonSet"`. New cache variant,
  watch, prefetch chain, UI title now "Workloads". Proves the extension
  pattern.
- **Slice 2 — StatefulSets.** ✅ *Merged.* Near-copy of Slice 1. Typed
  `Api<StatefulSet>`, status fields `ready_replicas` / `replicas`,
  `spec.selector.match_labels` for pod lookup. New `ss:` cache prefix,
  new `WatchedResource::StatefulSets`. No UI model changes beyond adding
  another kind tag.
- **Slice 3 — Unowned leaf.** ✅ *Merged.* Highest user-visible payoff —
  unlocks static-pod visibility (kube-apiserver, etcd, scheduler,
  controller-manager). Not a typed K8s resource: synthesized from the
  existing pod fetch by filtering `owner_references` empty or
  `kind = Node`. New `PodSelector::Unowned` variant drives both the
  rs_app landing row and the pod_app drill-down; a shared `project_pod`
  helper in `src/k8s/pods.rs` keeps projection logic DRY between
  `list_rspods` and `list_unowned_pods`. No new watch — reuses the Pods
  watch.
- **Slice 4 — Jobs.** First slice where the selector model breaks:
  Job→Pod goes through `owner_references`, not `spec.selector`. Default
  filter `status.active > 0`; completed Jobs only visible via `/`.
  New `job:` cache prefix and watch.
- **Slice 5 — CronJobs.** Depends on Slice 4. Row represents the
  CronJob; `Enter` resolves to the latest active Job's pods
  (`owner_references` chain: CronJob → Job → Pod). Sane watch backoff —
  CronJob status churns on every tick. New `cj:` cache prefix and watch.

Slice-level checkpoint: after each slice, the corresponding pods in
`kind`'s `kube-system` become navigable without regression to the RS
flow. After Slice 3 the full kube-system is inspectable end-to-end.

## Checkpoints (whole feature)

1. `kind create cluster` → launch navipod → `kube-system` shows
   `coredns` (RS), `kube-proxy`/`kindnet` (DS), and a single
   `Unowned` leaf row whose count matches the four static pods
   (kube-apiserver, kube-scheduler, kube-controller-manager, etcd).
2. `Enter` on each workload row lands on `pod_app` with the right
   pods and no stray/extra ones. `Enter` on the Unowned leaf lands
   directly on `pod_app` showing those four static pods.
3. A namespace with a `CronJob` (e.g. apply a test `CronJob` that
   runs every minute with a long-running command) shows a CronJob
   row whose count/selection routes to the currently-active Job's
   pods. A CronJob whose last Job has completed shows zero pods by
   default; completed Jobs appear only when the user toggles the
   filter via `/`.
4. Switching namespace (`n` → pick) still clears cache and restarts
   *all* watches, including the new DS/SS/Job/CronJob ones — no
   zombie watch streams left from the prior namespace.
5. Filtering and the existing RS flow are unchanged for clusters where
   everything is RS-rooted (e.g. an app-tier namespace).
6. `just ci` green — new fetchers covered by cache unit tests; no
   regression in `cache_integration_test` / `k8s_cache_integration`.
