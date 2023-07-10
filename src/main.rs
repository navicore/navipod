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
use sqlx::sqlite::SqlitePool;
use std::error::Error;
use std::fs::File;
use std::path::Path;
use tokio::io::AsyncWriteExt;
use tracing::*;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the namespace to walk
    #[arg(short, long)]
    namespace: String,
    #[arg(short, long, default_value = "/tmp/k8p.db")]
    db_location: String,
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
    let mut value = name.replace('_', " ");

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

    if metrics_text.contains('{') {
        let re = Regex::new(r"(?P<metric_name>[^{]+)\{(?P<labels>.*)\} (?P<value>.*)")?;
        let caps = re.captures(metrics_text).ok_or(format!(
            "Failed to parse the input string: {}",
            metrics_text
        ))?;

        // Get the value for "k8p_metric_name" key
        result.push((
            "k8p_metric_name".to_string(),
            caps["metric_name"].trim().to_string(),
        ));

        // Get the value for "k8p_value" key
        result.push(("k8p_value".to_string(), caps["value"].trim().to_string()));

        // Process the labels inside {}
        let labels_text = &caps["labels"];
        let label_re = Regex::new(r#"(?P<key>[^=,]+)="(?P<value>[^"]*)""#)?;
        for caps in label_re.captures_iter(labels_text) {
            let key = caps["key"].trim();
            let value = caps["value"].trim();
            result.push((key.to_string(), value.to_string()));
        }
    } else {
        // The metrics_text does not contain {}, split on whitespace
        let split: Vec<&str> = metrics_text.split_whitespace().collect();
        if split.len() != 2 {
            warn!("Failed to parse the input string: {}", metrics_text);
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
                Ok((_name, value)) => {
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

async fn persist_triples(
    triples: Vec<Vec<(String, String, String)>>,
    pool: &SqlitePool,
) -> Result<(), Box<dyn Error>> {
    debug!("persisting {} metrics", triples.len());

    for vec in triples {
        for (subject, predicate, object) in vec {
            sqlx::query(
                r#"
                INSERT INTO triples (subject, predicate, object)
                VALUES (?, ?, ?)
                "#,
            )
            .bind(subject)
            .bind(predicate)
            .bind(object)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

fn format_triples(tuples: Vec<Vec<(String, String)>>) -> Vec<Vec<(String, String, String)>> {
    tuples
        .into_iter()
        .map(|inner_vec| {
            let my_uuid = Uuid::new_v4().to_string();
            inner_vec
                .into_iter()
                .map(|(first, second)| (my_uuid.clone(), first, second))
                .collect::<Vec<(String, String, String)>>()
        })
        .collect()
}

fn format_tuples(
    mut metrics: Vec<Vec<(String, String)>>,
    podname: &str,
    appname: &str,
    namespace: &str,
) -> Vec<Vec<(String, String)>> {
    let datetime: DateTime<Utc> = Utc::now();
    let date_string: String = datetime.to_rfc3339();
    info!(
        "formating {} metrics for app {} and pod {} in ns {}",
        metrics.len(),
        podname,
        appname,
        namespace
    );

    for observation in &mut *metrics {
        observation.push(("k8s_datetime".to_string(), date_string.to_string()));
        observation.push(("k8s_podname".to_string(), podname.to_string()));
        observation.push(("k8s_appname".to_string(), appname.to_string()));
        observation.push(("k8s_namespace".to_string(), namespace.to_string()));
    }
    metrics
}

async fn process_metrics(
    pool: &SqlitePool,
    pods: &Api<Pod>,
    metadata_name: &str,
    path: &str,
    port: &str,
    appname: &str,
    namespace: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("getting health from {}{}:{}", metadata_name, path, port);
    let local_port: u16 = port.parse()?; // Convert the port to u16

    let mut port_forwarder = pods.portforward(metadata_name, &[local_port]).await?;
    let mut port_stream = match port_forwarder.take_stream(local_port) {
        Some(stream) => stream,
        None => {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Unable to take stream",
            )))
        }
    };
    // Write a HTTP GET request to the metrics path
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nAccept: */*\r\n\r\n"
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
            Err(err) => error!("Error reading response: {:?}", err),
        }
    }

    let metrics = parse_all_metrics(&metrics_text);
    let tuples = format_tuples(metrics, metadata_name, appname, namespace);
    let triples = format_triples(tuples);
    persist_triples(triples, pool).await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let namespace = args.namespace;

    let db_location = args.db_location;
    let db_url = format!("sqlite:{db_location}");
    let db_path = Path::new(&db_location);
    if db_path.exists() {
        info!("adding to db {}", db_url);
    } else {
        info!("creating db {}", db_url);
        File::create(&db_location)?;
    }

    let pool = SqlitePool::connect(&db_url).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS triples (
            id INTEGER PRIMARY KEY,
            subject TEXT NOT NULL,
            predicate TEXT NOT NULL,
            object TEXT NOT NULL
        );
        "#,
    )
    .execute(&pool)
    .await?;

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
        let metadata_name = metadata.name.unwrap_or_default();
        let labels = metadata.labels.unwrap_or_default();
        let appname = labels
            .get("app")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
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
            let p = process_metrics(
                &pool,
                &pods,
                metadata_name.as_str(),
                path.as_str(),
                port.as_str(),
                appname.as_str(),
                namespace.as_str(),
            )
            .await;

            match p {
                Ok(_) => (),
                Err(e) => error!("Error processing metrics for {}: {:?}", metadata_name, e),
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_help_type_valid() {
        let result =
            parse_help_type("# HELP http_requests_total The total number of HTTP requests.")
                .unwrap();
        assert_eq!(
            (
                "http_requests_total".to_string(),
                "The total number of HTTP requests.".to_string()
            ),
            result
        );

        let result = parse_help_type("# TYPE http_requests_total counter").unwrap();
        assert_eq!(
            ("http_requests_total".to_string(), "counter".to_string()),
            result
        );
    }

    #[test]
    #[should_panic(expected = "Invalid line format")]
    fn test_parse_help_type_invalid_format() {
        parse_help_type("# HELP").unwrap();
    }

    #[test]
    #[should_panic(expected = "First word must be '# HELP' or '# TYPE'")]
    fn test_parse_help_type_invalid_first_word() {
        parse_help_type("# INVALID http_requests_total counter").unwrap();
    }

    #[test]
    fn test_parse_metric_with_labels() {
        let result = parse_metric(
            "http_requests_total{method=\"post\",code=\"200\"} 1027",
            "The total number of HTTP requests.",
            "counter",
        )
        .unwrap();
        let expected = vec![
            (
                "k8p_metric_name".to_string(),
                "http_requests_total".to_string(),
            ),
            ("k8p_value".to_string(), "1027".to_string()),
            ("method".to_string(), "post".to_string()),
            ("code".to_string(), "200".to_string()),
            (
                "k8p_description".to_string(),
                "The total number of HTTP requests.".to_string(),
            ),
            ("k8p_type".to_string(), "counter".to_string()),
        ];
        assert_eq!(expected, result);
    }

    #[test]
    fn test_parse_metric_without_labels() {
        let result = parse_metric(
            "http_requests_total 1027",
            "The total number of HTTP requests.",
            "counter",
        )
        .unwrap();
        let expected = vec![
            (
                "k8p_metric_name".to_string(),
                "http_requests_total".to_string(),
            ),
            ("k8p_value".to_string(), "1027".to_string()),
            (
                "k8p_description".to_string(),
                "The total number of HTTP requests.".to_string(),
            ),
            ("k8p_type".to_string(), "counter".to_string()),
        ];
        assert_eq!(expected, result);
    }
}
