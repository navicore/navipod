use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{error::Error, io};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::stream::StreamExt;
use ratatui::prelude::*;
use tracing::{debug, error};

use crate::k8s::containers::list as list_containers;
use crate::k8s::rs::get_replicaset;
use crate::k8s::rs_ingress::list_ingresses;
use crate::net::analyze_tls_certificate;
use crate::tui::cert_app;
use crate::tui::container_app;
use crate::tui::data;
use crate::tui::data::pod_constraint_len_calculator;
use crate::tui::data::rs_constraint_len_calculator;
use crate::tui::ingress_app;
use crate::tui::pod_app;
use crate::tui::rs_app;
use crate::tui::stream::{async_key_events, async_pod_events, async_rs_events, Message};
use crate::tui::table_ui::TuiTableState;
use crate::tui::utils::time::asn1time_to_future_days_string;

// todo: change Apps enum to Switch command in AppCommand unum
// enum AppCommand {
//     Update,
//     Switch,
//     Quit,
// }

trait AppBehavior {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error>;

    async fn draw_ui<B: Backend>(&self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error>;
}

impl AppBehavior for rs_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Rs { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Char, Down, Enter, Up};
                    match key.code {
                        Char('q') => {
                            app_holder = None;
                            debug!("quiting...");
                        }
                        Char('j') | Down => {
                            self.next();
                            //todo: stop all this cloning
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Char('k') | Up => {
                            self.previous();
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Char('c' | 'C') => {
                            self.next_color();
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Char('i' | 'I') => {
                            if let Some(selection) = self.get_selected_item() {
                                if let Some(selector) = selection.selectors.clone() {
                                    let data_vec =
                                        create_ingress_data_vec(selector.clone()).await?;
                                    let new_app_holder = Apps::Ingress {
                                        app: ingress_app::app::App::new(data_vec),
                                    };
                                    app_holder = Some(new_app_holder);
                                    debug!("changing app from rs to ingress...");
                                };
                            };
                        }
                        Char('f' | 'F') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_forward();
                        }

                        Char('b' | 'B') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_backward();
                        }
                        Enter => {
                            if let Some(selection) = self.get_selected_item() {
                                if let Some(selectors) = selection.selectors.clone() {
                                    let data_vec = vec![];
                                    let new_app_holder = Apps::Pod {
                                        app: pod_app::app::App::new(selectors, data_vec),
                                    };
                                    app_holder = Some(new_app_holder);
                                    debug!("changing app from rs to pod...");
                                };
                            };
                        }
                        _k => {}
                    }
                }
            }
            Message::Rs(data_vec) => {
                debug!("updating rs app data...");
                let new_app = rs_app::app::App {
                    longest_item_lens: rs_constraint_len_calculator(&data_vec),
                    items: data_vec.to_vec(),
                    ..self.clone()
                };
                let new_app_holder = Apps::Rs { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        Ok(app_holder)
    }
    async fn draw_ui<B: Backend>(&self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| rs_app::ui::ui(f, &mut self.clone()))?;
        Ok(())
    }
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

async fn run_rs_app<B: Backend + Send>(
    terminal: &mut Terminal<B>,
    app: &mut rs_app::app::App,
) -> Result<Option<Apps>, io::Error> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let key_events = async_key_events(should_stop.clone());
    let data_events = async_rs_events(should_stop.clone(), app.get_items().to_vec());
    let mut events = futures::stream::select(data_events, key_events);

    #[allow(unused_assignments)] // we might quit or ESC
    let mut app_holder = Some(Apps::Rs { app: app.clone() });

    loop {
        _ = app.draw_ui(terminal).await;
        if let Some(event) = events.next().await {
            app_holder = app.handle_event(&event).await?;
            break;
        };
    }
    should_stop.store(true, Ordering::Relaxed);
    Ok(app_holder)
}

impl AppBehavior for pod_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Pod { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Char, Down, Enter, Esc, Up};
                    match key.code {
                        Char('q') | Esc => {
                            app_holder = None;
                        }
                        Char('j') | Down => {
                            self.next();
                            //todo: stop all this cloning
                            app_holder = Some(Apps::Pod { app: self.clone() });
                        }
                        Char('k') | Up => {
                            self.previous();
                            app_holder = Some(Apps::Pod { app: self.clone() });
                        }
                        Char('c' | 'C') => {
                            self.next_color();
                            app_holder = Some(Apps::Pod { app: self.clone() });
                        }
                        Char('i' | 'I') => {
                            if let Some(selection) = self.get_selected_item() {
                                if let Some(selector) = selection.selectors.clone() {
                                    let data_vec =
                                        create_ingress_data_vec(selector.clone()).await?;
                                    let new_app_holder = Apps::Ingress {
                                        app: ingress_app::app::App::new(data_vec),
                                    };
                                    app_holder = Some(new_app_holder);
                                    debug!("changing app from rs to ingress...");
                                };
                            };
                        }
                        Char('f' | 'F') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_forward();
                        }
                        Char('b' | 'B') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_backward();
                        }
                        Enter => {
                            if let Some(selection) = self.get_selected_item() {
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
                                };
                            }
                        }
                        _k => {}
                    }
                }
            }
            Message::Pod(data_vec) => {
                debug!("updating pod app data...");
                let new_app = pod_app::app::App {
                    longest_item_lens: pod_constraint_len_calculator(&data_vec),
                    items: data_vec.to_vec(),
                    ..self.clone()
                };
                let new_app_holder = Apps::Pod { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        Ok(app_holder)
    }
    async fn draw_ui<B: Backend>(&self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| pod_app::ui::ui(f, &mut self.clone()))?;
        Ok(())
    }
}

async fn run_pod_app<B: Backend + Send>(
    terminal: &mut Terminal<B>,
    app: &mut pod_app::app::App,
) -> Result<Option<Apps>, io::Error> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let key_events = async_key_events(should_stop.clone());
    let data_events = async_pod_events(
        app.selector.clone(),
        should_stop.clone(),
        app.get_items().to_vec(),
    );
    let mut events = futures::stream::select(data_events, key_events);

    #[allow(unused_assignments)] // we might quit or ESC
    let mut app_holder = Some(Apps::Pod { app: app.clone() });

    loop {
        _ = app.draw_ui(terminal).await;
        if let Some(event) = events.next().await {
            app_holder = app.handle_event(&event).await?;
            //todo: when switching to AppCommand from Apps you can decide to only
            //break when command is "switch apps"
            break;
        };
    }

    should_stop.store(true, Ordering::Relaxed);
    Ok(app_holder)
}

impl AppBehavior for cert_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Cert { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Char, Down, Esc, Up};
                    match key.code {
                        Char('q') | Esc => {
                            app_holder = None;
                        }
                        Char('j') | Down => {
                            self.next();
                            //todo: stop all this cloning
                            app_holder = Some(Apps::Cert { app: self.clone() });
                        }
                        Char('k') | Up => {
                            self.previous();
                            app_holder = Some(Apps::Cert { app: self.clone() });
                        }
                        Char('c' | 'C') => {
                            self.next_color();
                            app_holder = Some(Apps::Cert { app: self.clone() });
                        }
                        Char('f' | 'F') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_forward();
                        }
                        Char('b' | 'B') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_backward();
                        }
                        _k => {}
                    }
                }
            }
            _ => {}
        }
        Ok(app_holder)
    }
    async fn draw_ui<B: Backend>(&self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| cert_app::ui::ui(f, &mut self.clone()))?;
        Ok(())
    }
}

async fn run_cert_app<B: Backend + Send>(
    terminal: &mut Terminal<B>,
    app: &mut cert_app::app::App,
) -> Result<Option<Apps>, io::Error> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let mut events = async_key_events(should_stop.clone());
    #[allow(unused_assignments)] // we might quit or ESC
    let mut app_holder = Some(Apps::Cert { app: app.clone() });

    loop {
        _ = app.draw_ui(terminal).await;
        if let Some(event) = events.next().await {
            app_holder = app.handle_event(&event).await?;
            break;
        };
    }

    should_stop.store(true, Ordering::Relaxed);
    Ok(app_holder)
}

impl AppBehavior for container_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Container { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Char, Down, Esc, Up};
                    match key.code {
                        Char('q') | Esc => {
                            app_holder = None;
                        }
                        Char('j') | Down => {
                            self.next();
                            //todo: stop all this cloning
                            app_holder = Some(Apps::Container { app: self.clone() });
                        }
                        Char('k') | Up => {
                            self.previous();
                            app_holder = Some(Apps::Container { app: self.clone() });
                        }
                        Char('c' | 'C') => {
                            self.next_color();
                            app_holder = Some(Apps::Container { app: self.clone() });
                        }
                        Char('f' | 'F') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_forward();
                        }
                        Char('b' | 'B') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_backward();
                        }
                        _k => {}
                    }
                }
            }
            _ => {}
        }
        Ok(app_holder)
    }
    async fn draw_ui<B: Backend>(&self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| container_app::ui::ui(f, &mut self.clone()))?;
        Ok(())
    }
}

async fn run_container_app<B: Backend + Send>(
    terminal: &mut Terminal<B>,
    app: &mut container_app::app::App,
) -> Result<Option<Apps>, io::Error> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let mut events = async_key_events(should_stop.clone());
    #[allow(unused_assignments)] // we might quit or ESC
    let mut app_holder = Some(Apps::Container { app: app.clone() });

    loop {
        _ = app.draw_ui(terminal).await;
        if let Some(event) = events.next().await {
            app_holder = app.handle_event(&event).await?;
            break;
        };
    }

    should_stop.store(true, Ordering::Relaxed);
    Ok(app_holder)
}

impl AppBehavior for ingress_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Ingress { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Char, Down, Enter, Esc, Up};
                    match key.code {
                        Char('q') | Esc => {
                            app_holder = None;
                        }
                        Char('j') | Down => {
                            self.next();
                            //todo: stop all this cloning
                            app_holder = Some(Apps::Ingress { app: self.clone() });
                        }
                        Char('k') | Up => {
                            self.previous();
                            app_holder = Some(Apps::Ingress { app: self.clone() });
                        }
                        Char('c' | 'C') => {
                            self.next_color();
                            app_holder = Some(Apps::Ingress { app: self.clone() });
                        }
                        Char('f' | 'F') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_forward();
                        }
                        Char('b' | 'B') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_backward();
                        }
                        Enter => {
                            if let Some(selection) = self.get_selected_item() {
                                let host = &selection.host;
                                let data_vec = create_cert_data_vec(&host.clone()).await?;
                                let new_app_holder = Apps::Cert {
                                    app: cert_app::app::App::new(data_vec),
                                };
                                app_holder = Some(new_app_holder);
                                debug!("changing app from pod to cert...");
                            };
                        }

                        _k => {}
                    }
                }
            }
            _ => {}
        }
        Ok(app_holder)
    }
    async fn draw_ui<B: Backend>(&self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| ingress_app::ui::ui(f, &mut self.clone()))?;
        Ok(())
    }
}

async fn run_ingress_app<B: Backend + Send>(
    terminal: &mut Terminal<B>,
    app: &mut ingress_app::app::App,
) -> Result<Option<Apps>, io::Error> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let mut events = async_key_events(should_stop.clone());
    #[allow(unused_assignments)] // we might quit or ESC
    let mut app_holder = Some(Apps::Ingress { app: app.clone() });

    loop {
        _ = app.draw_ui(terminal).await;
        if let Some(event) = events.next().await {
            app_holder = app.handle_event(&event).await?;
            break;
        };
    }

    should_stop.store(true, Ordering::Relaxed);
    Ok(app_holder)
}

/// runs a stack of apps where navigation is "<Enter>" into and "<Esc>" out of
async fn run_root_ui_loop<B: Backend + Send>(terminal: &mut Terminal<B>) -> io::Result<()> {
    let data_vec = vec![];
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
