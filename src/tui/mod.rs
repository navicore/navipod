use crossterm::event::{poll, read};
use futures::stream::Stream;
use futures::stream::StreamExt; // Needed for the `.next()` method
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream; // Assuming you're using crossterm for events
mod container_app;
use std::sync::Arc;
pub mod data;
mod pod_app;
mod rs_app;
mod style;
mod table_ui;
use crate::k8s::pods::list_rspods;
use crate::k8s::rs::list_replicas;
use crate::tui::table_ui::TuiTableState;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::collections::BTreeMap;
use std::{error::Error, io};
use tracing::{debug, error};

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
    let mut key_events = async_key_events(should_stop.clone());
    #[allow(unused_assignments)] // we might quit or ESC
    let mut app_holder = Some(Apps::Rs { app: app.clone() });

    loop {
        terminal.draw(|f| rs_app::ui::ui(f, &mut app.clone()))?;
        if let Some(Event::Key(key)) = key_events.next().await {
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
                    Enter => {
                        if let Some(selection) = app.get_selected_item() {
                            if let Some(selector) = selection.selectors.clone() {
                                let data_vec = create_rspod_data_vec(selector).await?;
                                let new_app_holder = Apps::Pod {
                                    app: pod_app::app::App::new(data_vec),
                                };
                                app_holder = Some(new_app_holder);
                                debug!("changing app from rs to pod...");
                                break;
                            };
                        };
                    }
                    _ => {} //noop
                }
            }
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
    let mut key_events = async_key_events(should_stop.clone());
    #[allow(unused_assignments)] // we might quit or ESC
    let mut app_holder = Some(Apps::Pod { app: app.clone() });

    loop {
        terminal.draw(|f| pod_app::ui::ui(f, &mut app.clone()))?;
        if let Some(Event::Key(key)) = key_events.next().await {
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
                            let data_vec = selection.container_names.clone();
                            let new_app_holder = Apps::Container {
                                app: container_app::app::App::new(data_vec),
                            };
                            app_holder = Some(new_app_holder);
                            break;
                        }
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
        if let Some(Event::Key(key)) = key_events.next().await {
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
                    history.push(Arc::new(app_holder.clone())); // Save current state
                    app_holder = new_app_holder;
                } else {
                    break; //quit
                }
            }

            Apps::Pod { app } => {
                if let Some(new_app_holder) = run_pod_app(terminal, app).await? {
                    history.push(Arc::new(app_holder.clone())); // Save current state
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
        }
    }
    Ok(())
}

fn async_key_events(should_stop: Arc<AtomicBool>) -> impl Stream<Item = Event> {
    let (tx, rx) = mpsc::channel(100); // `100` is the capacity of the channel

    tokio::spawn(async move {
        while !should_stop.load(Ordering::Relaxed) {
            match poll(Duration::from_millis(100)) {
                Ok(true) => {
                    if let Ok(event) = read() {
                        if tx.send(event).await.is_err() {
                            error!("Error sending event");
                            break;
                        }
                    }
                }
                Ok(false) => {
                    // No event, continue the loop to check should_stop again
                }
                Err(e) => {
                    error!("Error polling for events: {e}");
                    break;
                }
            }
            // The loop will also check the should_stop flag here
        }
    });

    ReceiverStream::new(rx)
}
