use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::{error::Error, io};
use time::OffsetDateTime;
use x509_parser::time::ASN1Time; // Import time::Duration as TimeDuration to avoid name clash

use crossterm::event::{poll, read};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::stream::Stream;
use futures::stream::StreamExt;
use ratatui::prelude::*;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error};

use crate::k8s::containers::list_containers;
use crate::k8s::pods::list_rspods;
use crate::k8s::rs::get_replicaset;
use crate::k8s::rs::list_replicas;
use crate::k8s::rs_ingress::list_ingresses;
use crate::net::analyze_tls_certificate;
use crate::tui::table_ui::TuiTableState;

// Assuming you're using crossterm for events
mod cert_app;
mod container_app;
pub mod data;
mod ingress_app;
mod pod_app;
mod rs_app;
mod style;
mod table_ui;

const POLL_MS: u64 = 5000;

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
enum Apps {
    Rs { app: rs_app::app::App },
    Pod { app: pod_app::app::App },
    Container { app: container_app::app::App },
    Ingress { app: ingress_app::app::App },
    Cert { app: cert_app::app::App },
}

async fn create_container_data_vec(
    selectors: BTreeMap<String, String>,
    pod_name: String,
) -> Result<Vec<data::Container>, io::Error> {
    match list_containers(selectors, pod_name).await {
        Ok(cntrs) => Ok(cntrs),
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
    }
}

async fn create_ingress_data_vec(
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

fn asn1time_to_future_days_string(asn1_time: &ASN1Time) -> String {
    let now = OffsetDateTime::now_utc();

    // Directly get OffsetDateTime from ASN1Time
    let target_time = asn1_time.to_datetime();

    // Calculate the difference in days
    let duration = target_time - now;
    let days_difference = duration.whole_days();

    // Return the difference in days as a String with a "d" suffix
    format!("{days_difference}d")
}

async fn create_cert_data_vec(host: &str) -> Result<Vec<data::Cert>, io::Error> {
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

async fn create_rspod_data_vec(
    selector: BTreeMap<String, String>,
) -> Result<Vec<data::RsPod>, io::Error> {
    match list_rspods(selector).await {
        Ok(d) => Ok(d),
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
    }
}

async fn run_rs_app<B: Backend + Send>(
    terminal: &mut Terminal<B>,
    app: &mut rs_app::app::App,
) -> Result<Option<Apps>, io::Error> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let key_events = async_key_events(should_stop.clone());
    let data_events = async_rs_events(should_stop.clone());
    let mut events = futures::stream::select(data_events, key_events);

    #[allow(unused_assignments)] // we might quit or ESC
    let mut app_holder = Some(Apps::Rs { app: app.clone() });

    loop {
        terminal.draw(|f| rs_app::ui::ui(f, &mut app.clone()))?;
        match events.next().await {
            Some(StreamEvent::Key(Event::Key(key))) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Char, Down, Enter, Up};
                    match key.code {
                        Char('q') => {
                            app_holder = None;
                            debug!("quiting...");
                            break;
                        }
                        Char('j') | Down => {
                            app.next();
                        }
                        Char('k') | Up => {
                            app.previous();
                        }
                        Char('c' | 'C') => {
                            app.next_color();
                        }
                        Char('i' | 'I') => {
                            if let Some(selection) = app.get_selected_item() {
                                if let Some(selector) = selection.selectors.clone() {
                                    let data_vec =
                                        create_ingress_data_vec(selector.clone()).await?;
                                    let new_app_holder = Apps::Ingress {
                                        app: ingress_app::app::App::new(data_vec),
                                    };
                                    app_holder = Some(new_app_holder);
                                    debug!("changing app from rs to ingress...");
                                    break;
                                };
                            };
                        }
                        Enter => {
                            if let Some(selection) = app.get_selected_item() {
                                if let Some(selectors) = selection.selectors.clone() {
                                    let data_vec = create_rspod_data_vec(selectors.clone()).await?;
                                    let new_app_holder = Apps::Pod {
                                        app: pod_app::app::App::new(selectors, data_vec),
                                    };
                                    app_holder = Some(new_app_holder);
                                    debug!("changing app from rs to pod...");
                                    break;
                                };
                            };
                        }
                        _k => {}
                    }
                }
            }
            Some(StreamEvent::Rs(data_vec)) => {
                debug!("updating rs app data...");
                let new_app = rs_app::app::App {
                    items: data_vec,
                    ..app.clone()
                };
                let new_app_holder = Apps::Rs { app: new_app };
                app_holder = Some(new_app_holder);
                break;
            }
            _ => {}
        }
    }
    should_stop.store(true, Ordering::Relaxed);
    Ok(app_holder)
}

async fn run_pod_app<B: Backend + Send>(
    terminal: &mut Terminal<B>,
    app: &mut pod_app::app::App,
) -> Result<Option<Apps>, io::Error> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let key_events = async_key_events(should_stop.clone());
    let data_events = async_pod_events(app.selector.clone(), should_stop.clone());
    let mut events = futures::stream::select(data_events, key_events);

    #[allow(unused_assignments)] // we might quit or ESC
    let mut app_holder = Some(Apps::Pod { app: app.clone() });

    loop {
        terminal.draw(|f| pod_app::ui::ui(f, &mut app.clone()))?;
        match events.next().await {
            Some(StreamEvent::Key(Event::Key(key))) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Char, Down, Enter, Esc, Up};
                    match key.code {
                        Char('q') | Esc => {
                            app_holder = None;
                            break;
                        }
                        Char('j') | Down => {
                            app.next();
                        }
                        Char('k') | Up => {
                            app.previous();
                        }
                        Char('c' | 'C') => {
                            app.next_color();
                        }
                        Enter => {
                            if let Some(selection) = app.get_selected_item() {
                                if let Some(selectors) = selection.selectors.clone() {
                                    let data_vec = create_container_data_vec(
                                        selectors,
                                        selection.name.clone(),
                                    )
                                    .await?;
                                    let new_app_holder = Apps::Container {
                                        app: container_app::app::App::new(data_vec),
                                    };
                                    app_holder = Some(new_app_holder);
                                    break;
                                };
                            }
                        }
                        _k => {}
                    }
                }
            }
            Some(StreamEvent::Pod(data_vec)) => {
                debug!("updating pod app data...");
                let new_app = pod_app::app::App {
                    items: data_vec,
                    ..app.clone()
                };
                let new_app_holder = Apps::Pod { app: new_app };
                app_holder = Some(new_app_holder);
                break;
            }
            _ => {}
        }
    }
    should_stop.store(true, Ordering::Relaxed);
    Ok(app_holder)
}

async fn run_cert_app<B: Backend + Send>(
    terminal: &mut Terminal<B>,
    app: &mut cert_app::app::App,
) -> Result<Option<Apps>, io::Error> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let mut key_events = async_key_events(should_stop.clone());
    #[allow(unused_assignments)] // we might quit or ESC
    let mut app_holder = Some(Apps::Cert { app: app.clone() });

    loop {
        terminal.draw(|f| cert_app::ui::ui(f, &mut app.clone()))?;
        if let Some(StreamEvent::Key(Event::Key(key))) = key_events.next().await {
            if key.kind == KeyEventKind::Press {
                use KeyCode::{Char, Down, Esc, Up};
                match key.code {
                    Char('q') | Esc => {
                        app_holder = None;
                        break;
                    }
                    Char('j') | Down => {
                        app.next();
                    }
                    Char('k') | Up => {
                        app.previous();
                    }
                    Char('c' | 'C') => {
                        app.next_color();
                    }
                    _ => {}
                }
            }
        }
    }
    should_stop.store(true, Ordering::Relaxed);
    Ok(app_holder)
}

async fn run_container_app<B: Backend + Send>(
    terminal: &mut Terminal<B>,
    app: &mut container_app::app::App,
) -> Result<Option<Apps>, io::Error> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let mut key_events = async_key_events(should_stop.clone());
    #[allow(unused_assignments)] // we might quit or ESC
    let mut app_holder = Some(Apps::Container { app: app.clone() });

    loop {
        terminal.draw(|f| container_app::ui::ui(f, &mut app.clone()))?;
        if let Some(StreamEvent::Key(Event::Key(key))) = key_events.next().await {
            if key.kind == KeyEventKind::Press {
                use KeyCode::{Char, Down, Esc, Up};
                match key.code {
                    Char('q') | Esc => {
                        app_holder = None;
                        break;
                    }
                    Char('j') | Down => {
                        app.next();
                    }
                    Char('k') | Up => {
                        app.previous();
                    }
                    Char('c' | 'C') => {
                        app.next_color();
                    }
                    _ => {}
                }
            }
        }
    }
    should_stop.store(true, Ordering::Relaxed);
    Ok(app_holder)
}

async fn run_ingress_app<B: Backend + Send>(
    terminal: &mut Terminal<B>,
    app: &mut ingress_app::app::App,
) -> Result<Option<Apps>, io::Error> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let mut key_events = async_key_events(should_stop.clone());
    #[allow(unused_assignments)] // we might quit or ESC
    let mut app_holder = Some(Apps::Ingress { app: app.clone() });

    loop {
        terminal.draw(|f| ingress_app::ui::ui(f, &mut app.clone()))?;
        if let Some(StreamEvent::Key(Event::Key(key))) = key_events.next().await {
            if key.kind == KeyEventKind::Press {
                use KeyCode::{Char, Down, Enter, Esc, Up};
                match key.code {
                    Char('q') | Esc => {
                        app_holder = None;
                        break;
                    }
                    Char('j') | Down => {
                        app.next();
                    }
                    Char('k') | Up => {
                        app.previous();
                    }
                    Char('c' | 'C') => {
                        app.next_color();
                    }
                    Enter => {
                        if let Some(selection) = app.get_selected_item() {
                            let host = &selection.host;
                            let data_vec = create_cert_data_vec(&host.clone()).await?;
                            let new_app_holder = Apps::Cert {
                                app: cert_app::app::App::new(data_vec),
                            };
                            app_holder = Some(new_app_holder);
                            debug!("changing app from pod to cert...");
                            break;
                        };
                    }

                    _ => {}
                }
            }
        }
    }
    should_stop.store(true, Ordering::Relaxed);
    Ok(app_holder)
}

/// runs a stack of apps where navigation is "<Enter>" into and "<Esc>" out of
async fn run_root_ui_loop<B: Backend + Send>(terminal: &mut Terminal<B>) -> io::Result<()> {
    let data_vec = match list_replicas().await {
        Ok(d) => d,
        Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
    };
    let mut app_holder = Apps::Rs {
        app: rs_app::app::App::new(data_vec),
    };

    let mut history: Vec<Arc<Apps>> = Vec::new();
    loop {
        match &mut app_holder {
            Apps::Rs { app } => {
                if let Some(new_app_holder) = run_rs_app(terminal, app).await? {
                    match new_app_holder.clone() {
                        Apps::Rs { app: _ } => {}
                        _ => {
                            history.push(Arc::new(app_holder.clone())); // Save current state
                        }
                    }
                    app_holder = new_app_holder;
                } else {
                    break; //quit
                }
            }

            Apps::Pod { app } => {
                if let Some(new_app_holder) = run_pod_app(terminal, app).await? {
                    match new_app_holder.clone() {
                        Apps::Pod { app: _ } => {}
                        _ => {
                            history.push(Arc::new(app_holder.clone())); // Save current state
                        }
                    }
                    app_holder = new_app_holder;
                } else if let Some(previous_app) = history.pop() {
                    app_holder = (*previous_app).clone();
                } else {
                    break;
                }
            }

            Apps::Container { app } => {
                if let Some(new_app_holder) = run_container_app(terminal, app).await? {
                    history.push(Arc::new(app_holder.clone())); // Save current state
                    app_holder = new_app_holder;
                } else if let Some(previous_app) = history.pop() {
                    app_holder = (*previous_app).clone();
                } else {
                    break;
                }
            }

            Apps::Cert { app } => {
                if let Some(new_app_holder) = run_cert_app(terminal, app).await? {
                    history.push(Arc::new(app_holder.clone())); // Save current state
                    app_holder = new_app_holder;
                } else if let Some(previous_app) = history.pop() {
                    app_holder = (*previous_app).clone();
                } else {
                    break;
                }
            }

            Apps::Ingress { app } => {
                if let Some(new_app_holder) = run_ingress_app(terminal, app).await? {
                    history.push(Arc::new(app_holder.clone())); // Save current state
                    app_holder = new_app_holder;
                } else if let Some(previous_app) = history.pop() {
                    app_holder = (*previous_app).clone();
                } else {
                    break;
                }
            }
        }
    }
    Ok(())
}

enum StreamEvent {
    Key(Event),
    Pod(Vec<data::RsPod>),
    Rs(Vec<data::Rs>),
}

fn async_key_events(should_stop: Arc<AtomicBool>) -> impl Stream<Item = StreamEvent> {
    let (tx, rx) = mpsc::channel(100);

    tokio::spawn(async move {
        while !should_stop.load(Ordering::Relaxed) {
            match poll(Duration::from_millis(100)) {
                Ok(true) => {
                    if let Ok(event) = read() {
                        let sevent = StreamEvent::Key(event);
                        if tx.send(sevent).await.is_err() {
                            break;
                        }
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    error!("Error polling for events: {e}");
                    break;
                }
            }
        }
    });

    ReceiverStream::new(rx)
}

fn async_pod_events(
    selector: BTreeMap<String, String>,
    should_stop: Arc<AtomicBool>,
) -> impl Stream<Item = StreamEvent> {
    let (tx, rx) = mpsc::channel(100);

    tokio::spawn(async move {
        while !should_stop.load(Ordering::Relaxed) {
            sleep(Duration::from_millis(POLL_MS)).await;
            //get Vec and send
            match list_rspods(selector.clone()).await {
                Ok(d) => {
                    let sevent = StreamEvent::Pod(d);
                    if tx.send(sevent).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    error!("Error listing pods: {e}");
                    break;
                }
            }
        }
    });

    ReceiverStream::new(rx)
}

fn async_rs_events(should_stop: Arc<AtomicBool>) -> impl Stream<Item = StreamEvent> {
    let (tx, rx) = mpsc::channel(100);

    tokio::spawn(async move {
        while !should_stop.load(Ordering::Relaxed) {
            sleep(Duration::from_millis(POLL_MS)).await;
            match list_replicas().await {
                Ok(d) => {
                    let sevent = StreamEvent::Rs(d);
                    if tx.send(sevent).await.is_err() {}
                }
                Err(_e) => {
                    break;
                }
            };
        }
    });

    ReceiverStream::new(rx)
}
