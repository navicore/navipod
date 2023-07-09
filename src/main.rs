use chrono::{DateTime, Utc};
use clap::Parser;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::api::ObjectList;
use kube::{
    api::{Api, ListParams},
    Client,
};
use regex::Regex;
use std::error::Error;
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

fn parse_help_type(line: &str) -> Result<(String, String), Box<dyn Error>> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    // Check if there are at least 3 parts
    if parts.len() < 3 {
        return Err(From::from("Invalid line format"));
    }

    // Concatenate the first two parts to get the "first word"
    let first_word = format!("{} {}", parts[0], parts[1]);

    // Check if the first word is "# HELP" or "# TYPE"
    if first_word != "# HELP" && first_word != "# TYPE" {
        return Err(From::from("First word must be '# HELP' or '# TYPE'"));
    }

    let name = parts[2].to_string();

    // Use the name as the default value
    let mut value = name.replace("_", " ");

    // If there are more parts, use the rest of the line as the value
    if parts.len() > 3 {
        value = parts[3..].join(" ");
    }

    Ok((name, value))
}

fn parse_metric(
    metrics_text: &str,
    k8p_description: &str,
    k8p_type: &str,
) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let mut result = Vec::new();

    if metrics_text.contains("{") {
        // The metrics_text contains {}, so process it using the original approach
        let re = Regex::new(r"(?P<metric_name>[^{]+)\{(?P<labels>[^}]*)\} (?P<value>.*)")?;
        let caps = re
            .captures(metrics_text)
            .ok_or("Failed to parse the input string")?;

        // Get the value for "k8p_metric_name" key
        result.push((
            "k8p_metric_name".to_string(),
            caps["metric_name"].trim().to_string(),
        ));

        // Get the value for "k8p_value" key
        result.push(("k8p_value".to_string(), caps["value"].trim().to_string()));

        // Process the labels inside {}
        let labels_text = &caps["labels"];
        let labels = labels_text.split(',');

        for label in labels {
            if label.trim().is_empty() {
                continue;
            }

            let key_value: Vec<&str> = label.split('=').collect();
            if key_value.len() == 2 {
                // Remove quotes around the value if present
                let value = key_value[1].trim_matches('"');
                result.push((key_value[0].to_string(), value.to_string()));
            } else {
                return Err(From::from("Failed to parse a label=value pair"));
            }
        }
    } else {
        // The metrics_text does not contain {}, split on whitespace
        let split: Vec<&str> = metrics_text.split_whitespace().collect();
        if split.len() != 2 {
            warn!("Failed to parse the input string");
            return Ok(Vec::new());
        }

        result.push(("k8p_metric_name".to_string(), split[0].to_string()));
        result.push(("k8p_value".to_string(), split[1].to_string()));
    }

    // Append "k8p_description" and "k8p_type"
    result.push(("k8p_description".to_string(), k8p_description.to_string()));
    result.push(("k8p_type".to_string(), k8p_type.to_string()));

    Ok(result)
}

fn parse_all_metrics(metrics_text: &str) -> Vec<Vec<(String, String)>> {
    let mut result = Vec::new();
    let mut k8p_description = String::new();
    let mut k8p_type = String::new();

    for line in metrics_text.lines() {
        let line = line.trim();
        if line.starts_with("# HELP") || line.starts_with("# TYPE") {
            match parse_help_type(line) {
                Ok((name, value)) => {
                    if line.starts_with("# HELP") {
                        k8p_description = value;
                    } else {
                        k8p_type = value;
                    }
                }
                Err(err) => warn!("Failed to parse line '{}': {}", line, err),
            }
        } else if !line.is_empty() {
            match parse_metric(line, &k8p_description, &k8p_type) {
                Ok(metric) => result.push(metric),
                Err(err) => warn!("Failed to parse line '{}': {}", line, err),
            }
        }
    }

    result
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
    let mut metrics_text = String::new();

    // Read the response and write it to a string
    while let Some(response) = response_stream.next().await {
        match response {
            Ok(bytes) => {
                metrics_text.push_str(std::str::from_utf8(&bytes[..])?);
            }
            Err(err) => println!("Error reading response: {:?}", err),
        }
    }

    let _r = parse_all_metrics(&metrics_text);
    //info!("got results: {:?}", _r.size());

    let datetime: DateTime<Utc> = Utc::now();
    let filename = format!("logs/{}-{}.txt", metadata_name, datetime);
    let mut file = File::create(filename)?;
    file.write_all(metrics_text.as_bytes())?;

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
            .unwrap_or_default();

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
