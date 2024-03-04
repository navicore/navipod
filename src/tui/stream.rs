use crate::tui::data;
use crossterm::event::Event;
use crossterm::event::{poll, read};
use futures::stream::Stream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::error;

pub enum Message {
    Key(Event),
    Pod(Vec<data::RsPod>),
    Rs(Vec<data::Rs>),
    #[allow(dead_code)]
    Ingress(Vec<data::Ingress>),
    #[allow(dead_code)]
    Container(Vec<data::Container>),
    #[allow(dead_code)]
    Cert(Vec<data::Cert>),
    #[allow(dead_code)]
    Log(Vec<data::LogRec>),
}

pub fn async_key_events(should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message> {
    let (tx, rx) = mpsc::channel(100);

    tokio::spawn(async move {
        while !should_stop.load(Ordering::Relaxed) {
            match poll(Duration::from_millis(100)) {
                Ok(true) => {
                    if let Ok(event) = read() {
                        let sevent = Message::Key(event);
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
