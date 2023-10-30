//!A module to encapsulate how the k8s data is marshaled into triples.
//!
use crate::triples;
use crate::tuples;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::api::Api;
use regex::Regex;
use sqlx::sqlite::SqlitePool;
use std::error::Error;
use tokio::io::AsyncWriteExt;
use tracing::{error, info, warn};

/// the metrics records often are accompanied by HELP and TYPE records to
/// give us "description" and whether a metric is a gauge or an accumulator.
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

    let value = if parts.len() > 3 {
        parts[3..].join(" ")
    } else {
        name.replace('_', " ")
    };

    Ok((name, value))
}

/// break an individual record down into its parts.
fn parse_metric(
    metrics_text: &str,
    k8p_description: &str,
    k8p_type: &str,
) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let mut result = Vec::new();

    if metrics_text.contains('{') {
        let re = Regex::new(r"(?P<metric_name>[^{]+)\{(?P<labels>.*)\} (?P<value>.*)")?;
        let caps = re
            .captures(metrics_text)
            .ok_or(format!("Failed to parse the input string: {metrics_text}"))?;

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

/// the data retrieved from the pod is a single crlf-delimited blob of text.
fn parse_all(metrics_text: &str) -> Vec<Vec<(String, String)>> {
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

/// # Errors
///
/// Will return `Err` if access to k8s is not enabled via `kubeconfig`.
pub async fn get_text(
    pods: &Api<Pod>,
    metadata_name: &str,
    path: &str,
    port: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let local_port: u16 = port.parse()?; // Convert the port to u16

    let mut port_forwarder = pods.portforward(metadata_name, &[local_port]).await?;
    let Some(mut port_stream) = port_forwarder.take_stream(local_port) else {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Unable to take stream",
        )));
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

    Ok(metrics_text)
}

/// # Errors
///
/// Will return `Err` if access to k8s is not enabled via `kubeconfig`.
pub async fn process(
    pool: &SqlitePool,
    pods: &Api<Pod>,
    metadata_name: &str,
    path: &str,
    port: &str,
    appname: &str,
    namespace: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("getting health from {}{}:{}", metadata_name, path, port);
    let metrics_text = get_text(pods, metadata_name, path, port).await?;

    let metrics = parse_all(&metrics_text);
    let tuples = tuples::format(metrics, metadata_name, appname, namespace);
    let triples = triples::format(tuples);
    triples::persist(triples, pool).await
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
