use crate::k8s::containers::list as list_containers;
use crate::k8s::rs::get_replicaset;
use crate::k8s::rs_ingress::list_ingresses;
use crate::net::analyze_tls_certificate;
use crate::tui::cert_app;
use crate::tui::container_app;
use crate::tui::data;
use crate::tui::event_app;
use crate::tui::ingress_app;
use crate::tui::log_app;
use crate::tui::pod_app;
use crate::tui::rs_app;
use crate::tui::stream::{Message, async_key_events};
use crate::tui::utils::time::asn1time_to_future_days_string;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::stream::Stream;
use futures::stream::StreamExt;
use ratatui::prelude::*;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{error::Error, io};
use tracing::error;

pub(crate) trait AppBehavior {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error>;

    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error>;

    fn stream(&self, should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message>;
}

/// # Errors
///
/// Will return `Err` if function cannot access a terminal or render a ui
pub async fn run() -> Result<(), Box<dyn Error>> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_root_ui_loop(&mut terminal).await;

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        error!("{err:?}");
    }

    Ok(())
}

#[derive(Clone, Debug)]
pub enum Apps {
    Rs { app: rs_app::app::App },
    Pod { app: pod_app::app::App },
    Container { app: container_app::app::App },
    Ingress { app: ingress_app::app::App },
    Cert { app: cert_app::app::App },
    Log { app: log_app::app::App },
    Event { app: event_app::app::App },
}

/// # Errors
///
/// Will return `Err` if function cannot access the k8s api
pub async fn create_container_data_vec(
    selectors: BTreeMap<String, String>,
    pod_name: String,
) -> Result<Vec<data::Container>, io::Error> {
    match list_containers(selectors, pod_name).await {
        Ok(cntrs) => Ok(cntrs),
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
    }
}

/// # Errors
///
/// Will return `Err` if function cannot access the k8s api
pub async fn create_ingress_data_vec(
    selector: BTreeMap<String, String>,
) -> Result<Vec<data::Ingress>, io::Error> {
    match get_replicaset(selector).await {
        Ok(rso) => match rso {
            Some(rs) => match list_ingresses(&rs, "").await {
                Ok(ingress) => Ok(ingress),
                Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
            },
            _ => Ok(vec![]),
        },
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
    }
}

/// # Errors
///
/// Will return `Err` if function cannot access the remote host and cert
pub async fn create_cert_data_vec(host: &str) -> Result<Vec<data::Cert>, io::Error> {
    match analyze_tls_certificate(host).await {
        Ok(cinfo) => {
            let d = data::Cert {
                host: host.to_string(),
                is_valid: cinfo.is_valid.to_string(),
                expires: asn1time_to_future_days_string(&cinfo.expires),
                issued_by: cinfo.issued_by,
            };
            Ok(vec![d])
        }
        Err(e) => {
            let emsg = format!("host: {host} error: {e}");
            Err(io::Error::new(io::ErrorKind::Other, emsg))
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn run_app<B>(
    terminal: &mut Terminal<B>,
    apps_app: &mut Apps,
) -> Result<(Option<Apps>, Option<Apps>), io::Error>
where
    B: Backend + Send,
{
    let should_stop = Arc::new(AtomicBool::new(false));
    let key_events = async_key_events(should_stop.clone());

    #[allow(unused_assignments)] // we might quit or ESC
    let mut old_app_holder = Some(apps_app.clone());
    #[allow(unused_assignments)] // we might quit or ESC
    let mut new_app_holder = None;
    match apps_app {
        Apps::Rs { app } => {
            let data_init_clone = app.clone();
            let data_events = data_init_clone.stream(should_stop.clone());
            let mut events = futures::stream::select(data_events, key_events);
            let mut current_app = app.clone();
            loop {
                _ = current_app.draw_ui(terminal);
                if let Some(event) = events.next().await {
                    let app_holder = current_app.handle_event(&event).await?;
                    if let Some(Apps::Rs { app }) = &app_holder {
                        current_app = app.clone();
                        old_app_holder = app_holder;
                    } else {
                        new_app_holder = app_holder;
                        break;
                    };
                };
            }
        }
        Apps::Pod { app } => {
            let data_init_clone = app.clone();
            let data_events = data_init_clone.stream(should_stop.clone());
            let mut events = futures::stream::select(data_events, key_events);
            let mut current_app = app.clone();
            loop {
                _ = current_app.draw_ui(terminal);
                if let Some(event) = events.next().await {
                    let app_holder = current_app.handle_event(&event).await?;
                    if let Some(Apps::Pod { app }) = &app_holder {
                        current_app = app.clone();
                        old_app_holder = app_holder;
                    } else {
                        new_app_holder = app_holder;
                        break;
                    };
                };
            }
        }
        Apps::Container { app } => {
            let data_init_clone = app.clone();
            let data_events = data_init_clone.stream(should_stop.clone());
            let mut events = futures::stream::select(data_events, key_events);
            let mut current_app = app.clone();
            loop {
                _ = current_app.draw_ui(terminal);
                if let Some(event) = events.next().await {
                    let app_holder = current_app.handle_event(&event).await?;
                    if let Some(Apps::Container { app }) = &app_holder {
                        current_app = app.clone();
                        old_app_holder = app_holder;
                    } else {
                        new_app_holder = app_holder;
                        break;
                    };
                };
            }
        }
        Apps::Cert { app } => {
            let data_init_clone = app.clone();
            let data_events = data_init_clone.stream(should_stop.clone());
            let mut events = futures::stream::select(data_events, key_events);
            let mut current_app = app.clone();
            loop {
                _ = current_app.draw_ui(terminal);
                if let Some(event) = events.next().await {
                    let app_holder = current_app.handle_event(&event).await?;
                    if let Some(Apps::Cert { app }) = &app_holder {
                        current_app = app.clone();
                        old_app_holder = app_holder;
                    } else {
                        new_app_holder = app_holder;
                        break;
                    };
                };
            }
        }
        Apps::Ingress { app } => {
            let data_init_clone = app.clone();
            let data_events = data_init_clone.stream(should_stop.clone());
            let mut events = futures::stream::select(data_events, key_events);
            let mut current_app = app.clone();
            loop {
                _ = current_app.draw_ui(terminal);
                if let Some(event) = events.next().await {
                    let app_holder = current_app.handle_event(&event).await?;
                    if let Some(Apps::Ingress { app }) = &app_holder {
                        current_app = app.clone();
                        old_app_holder = app_holder;
                    } else {
                        new_app_holder = app_holder;
                        break;
                    };
                };
            }
        }

        Apps::Log { app } => {
            let data_init_clone = app.clone();
            let data_events = data_init_clone.stream(should_stop.clone());
            let mut events = futures::stream::select(data_events, key_events);
            let mut current_app = app.clone();
            loop {
                _ = current_app.draw_ui(terminal);
                if let Some(event) = events.next().await {
                    let app_holder = current_app.handle_event(&event).await?;
                    if let Some(Apps::Log { app }) = &app_holder {
                        current_app = app.clone();
                        old_app_holder = app_holder;
                    } else {
                        new_app_holder = app_holder;
                        break;
                    };
                };
            }
        }

        Apps::Event { app } => {
            let data_init_clone = app.clone();
            let data_events = data_init_clone.stream(should_stop.clone());
            let mut events = futures::stream::select(data_events, key_events);
            let mut current_app = app.clone();
            loop {
                _ = current_app.draw_ui(terminal);
                if let Some(event) = events.next().await {
                    let app_holder = current_app.handle_event(&event).await?;
                    if let Some(Apps::Event { app }) = &app_holder {
                        current_app = app.clone();
                        old_app_holder = app_holder;
                    } else {
                        new_app_holder = app_holder;
                        break;
                    };
                };
            }
        }
    }

    should_stop.store(true, Ordering::Relaxed);
    Ok((old_app_holder, new_app_holder))
}

/// runs a stack of apps where navigation is "<Enter>" into and "<Esc>" out of
async fn run_root_ui_loop<B: Backend + Send>(terminal: &mut Terminal<B>) -> io::Result<()> {
    let data_vec = vec![];
    let mut app_holder = Apps::Rs {
        app: rs_app::app::App::new(data_vec),
    };

    let mut history: Vec<Arc<Apps>> = Vec::new();
    loop {
        match run_app(terminal, &mut app_holder).await? {
            (Some(old_app_holder), Some(new_app_holder)) => {
                history.push(Arc::new(old_app_holder)); // this is an app switch
                app_holder = new_app_holder;
            }
            (_, _) => {
                if let Some(previous_app) = history.pop() {
                    app_holder = (*previous_app).clone();
                } else {
                    break; //quit
                }
            }
        }
    }
    Ok(())
}
