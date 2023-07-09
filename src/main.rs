use chrono::{DateTime, Utc};
use clap::Parser;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::api::ObjectList;
use kube::{
    api::{Api, ListParams},
    Client,
};
use std::fs::File;
use std::io::Write;
use tokio::io::AsyncWriteExt;
use tracing::*;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the namespace to walk
    #[arg(short, long)]
    namespace: String,
}

async fn process_metrics(
    pods: &Api<Pod>,
    metadata_name: &str,
    path: &str,
    port: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("getting health from {}{}:{}", metadata_name, path, port);
    let local_port: u16 = port.parse()?; // Convert the port to u16

    let mut port_forwarder = pods.portforward(metadata_name, &[local_port]).await?;
    let mut port_stream = port_forwarder.take_stream(local_port).unwrap();

    // Write a HTTP GET request to the metrics path
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nAccept: */*\r\n\r\n",
        path
    );
    port_stream.write_all(request.as_bytes()).await?;

    let mut response_stream = tokio_util::io::ReaderStream::new(port_stream);

    // Read the response and write it to a file
    if let Some(response) = response_stream.next().await {
        match response {
            Ok(bytes) => {
                let metrics_text = std::str::from_utf8(&bytes[..])?;
                let datetime: DateTime<Utc> = Utc::now();
                let filename = format!("logs/{}-{}.txt", metadata_name, datetime);
                let mut file = File::create(filename)?;
                file.write_all(metrics_text.as_bytes())?;
            }
            Err(err) => println!("Error reading response: {:?}", err),
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let namespace = args.namespace;
    let client = Client::try_default()
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let pods: Api<Pod> = Api::namespaced(client, namespace.as_str());
    let lp = ListParams::default();

    let pod_list: ObjectList<Pod> = pods
        .list(&lp)
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    for p in pod_list.items {
        let metadata = p.metadata.clone();
        let metadata_name = metadata.name.unwrap();
        let annotations = metadata.annotations.unwrap_or_default();
        let scrape = annotations
            .get("prometheus.io/scrape")
            .cloned()
            .unwrap_or_else(|| "false".to_string());
        let path = annotations
            .get("prometheus.io/path")
            .cloned()
            .unwrap_or_else(|| "/metrics".to_string());
        let port = annotations
            .get("prometheus.io/port")
            .cloned()
            .unwrap_or_else(|| "".to_string());

        if scrape == "true" {
            match process_metrics(&pods, metadata_name.as_str(), path.as_str(), port.as_str()).await
            {
                Ok(_) => (),
                Err(e) => eprintln!("Error processing metrics for {}: {:?}", metadata_name, e),
            }
        }
    }

    Ok(())
}
