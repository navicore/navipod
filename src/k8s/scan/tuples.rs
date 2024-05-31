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
        "formatting {} metrics for app {} and pod {} in ns {}",
        metrics.len(),
        podname,
        appname,
        namespace
    );

    for observation in &mut *metrics {
        observation.push(("navipod_datetime".to_string(), date_string.to_string()));
        observation.push(("navipod_podname".to_string(), podname.to_string()));
        observation.push(("navipod_appname".to_string(), appname.to_string()));
        observation.push(("navipod_namespace".to_string(), namespace.to_string()));
    }
    metrics
}
