use chrono::{DateTime, Utc};
use tracing::info;

pub fn format(
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
        observation.push(("k8p_datetime".to_string(), date_string.to_string()));
        observation.push(("k8p_podname".to_string(), podname.to_string()));
        observation.push(("k8p_appname".to_string(), appname.to_string()));
        observation.push(("k8p_namespace".to_string(), namespace.to_string()));
    }
    metrics
}
