use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::Event;
use crossterm::event::{poll, read};
use futures::stream::Stream;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use tracing::error;

use crate::k8s::pods::list_rspods;
use crate::k8s::rs::list_replicas;
use crate::tui::data;

const POLL_MS: u64 = 5000;

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

pub fn async_pod_events(
    selector: BTreeMap<String, String>,
    should_stop: Arc<AtomicBool>,
    current_data: Vec<data::RsPod>,
) -> impl Stream<Item = Message> {
    let (tx, rx) = mpsc::channel(100);

    tokio::spawn(async move {
        while !should_stop.load(Ordering::Relaxed) {
            //get Vec and send
            match list_rspods(selector.clone()).await {
                Ok(d) => {
                    if !d.is_empty() && d != current_data {
                        let sevent = Message::Pod(d);
                        if tx.send(sevent).await.is_err() {
                            break;
                        }
                    }
                    sleep(Duration::from_millis(POLL_MS)).await;
                }
                Err(_e) => {
                    break;
                }
            }
        }
    });

    ReceiverStream::new(rx)
}

pub fn async_rs_events(
    should_stop: Arc<AtomicBool>,
    current_data: Vec<data::Rs>,
) -> impl Stream<Item = Message> {
    let (tx, rx) = mpsc::channel(1);

    tokio::spawn(async move {
        while !should_stop.load(Ordering::Relaxed) {
            match list_replicas().await {
                Ok(d) => {
                    // only update if different from current events
                    if !d.is_empty() && d != current_data {
                        let sevent = Message::Rs(d);
                        if tx.send(sevent).await.is_err() {
                            break;
                        }
                    }
                    sleep(Duration::from_millis(POLL_MS)).await;
                }
                Err(_e) => {
                    break;
                }
            };
        }
    });

    ReceiverStream::new(rx)
}
