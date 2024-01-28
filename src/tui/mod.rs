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
use tracing::error;

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

    let res = run_app(&mut terminal).await;

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

// todo: fix mess - issue is letting the enter key change the app_holder across fn calls
#[allow(clippy::too_many_lines)]
async fn run_app<B: Backend + Send>(terminal: &mut Terminal<B>) -> io::Result<()> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let mut key_events = async_key_events(should_stop.clone());
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
            Apps::Rs { app: rs_app } => {
                terminal.draw(|f| rs_app::ui::ui(f, &mut rs_app.clone()))?;

                if let Some(Event::Key(key)) = key_events.next().await {
                    if key.kind == KeyEventKind::Press {
                        use KeyCode::{Char, Down, Enter, Up};
                        match key.code {
                            Char('q') => {
                                should_stop.store(true, Ordering::Relaxed);
                                return Ok(());
                            }
                            Char('j') | Down => rs_app.next(),
                            Char('k') | Up => rs_app.previous(),
                            Char('c' | 'C') => rs_app.next_color(),
                            Enter => {
                                if let Some(selection) = rs_app.get_selected_item() {
                                    if let Some(selector) = selection.selectors.clone() {
                                        let data_vec = create_rspod_data_vec(selector).await?;
                                        let new_app_holder = Apps::Pod {
                                            app: pod_app::app::App::new(data_vec),
                                        };
                                        history.push(Arc::new(app_holder.clone())); // Save current state
                                        app_holder = new_app_holder;
                                    };
                                };
                            }
                            _ => {}
                        }
                    }
                }
            }
            Apps::Pod { app: pod_app } => {
                terminal.draw(|f| pod_app::ui::ui(f, &mut pod_app.clone()))?;

                if let Some(Event::Key(key)) = key_events.next().await {
                    if key.kind == KeyEventKind::Press {
                        use KeyCode::{Char, Down, Enter, Esc, Up};
                        match key.code {
                            Char('q') => {
                                should_stop.store(true, Ordering::Relaxed);
                                return Ok(());
                            }
                            Char('j') | Down => pod_app.next(),
                            Char('k') | Up => pod_app.previous(),
                            Char('c' | 'C') => pod_app.next_color(),
                            Enter => {
                                if let Some(selection) = pod_app.get_selected_item() {
                                    let data_vec = selection.container_names.clone();
                                    let new_app_holder = Apps::Container {
                                        app: container_app::app::App::new(data_vec),
                                    };
                                    history.push(Arc::new(app_holder.clone())); // Save current state
                                    app_holder = new_app_holder;
                                }
                            }
                            Esc => {
                                if let Some(previous_app) = history.pop() {
                                    app_holder = (*previous_app).clone();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Apps::Container { app: container_app } => {
                terminal.draw(|f| container_app::ui::ui(f, &mut container_app.clone()))?;

                if let Some(Event::Key(key)) = key_events.next().await {
                    if key.kind == KeyEventKind::Press {
                        use KeyCode::{Char, Down, Esc, Up};
                        match key.code {
                            Char('q') => {
                                should_stop.store(true, Ordering::Relaxed);
                                return Ok(());
                            }
                            Char('j') | Down => container_app.next(),
                            Char('k') | Up => container_app.previous(),
                            Char('c' | 'C') => container_app.next_color(),
                            Esc => {
                                if let Some(previous_app) = history.pop() {
                                    app_holder = (*previous_app).clone();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
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
