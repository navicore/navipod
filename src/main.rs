use clap::Parser;
use k8p::metrics;
use k8s_openapi::api::core::v1::Pod;
use kube::api::ObjectList;
use kube::{
    api::{Api, ListParams},
    Client,
};
use sqlx::sqlite::SqlitePool;
use std::fs::File;
use std::path::Path;
use tracing::{error, info};

async fn init_db(db_location: String) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    let db_url = format!("sqlite:{db_location}");
    let db_path = Path::new(&db_location);
    if db_path.exists() {
        info!("adding to db {}", db_url);
    } else {
        info!("creating db {}", db_url);
        File::create(&db_location)?;
    }

    let pool = SqlitePool::connect(&db_url).await?;
    Ok(pool)
}

async fn create_table(pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS triples (
            id INTEGER PRIMARY KEY,
            subject TEXT NOT NULL,
            predicate TEXT NOT NULL,
            object TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_subject ON triples (subject);
        CREATE INDEX IF NOT EXISTS idx_predicate ON triples (predicate);
        CREATE INDEX IF NOT EXISTS idx_object ON triples (object);
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn fetch_pods(
    client: Client,
    namespace: String,
) -> Result<(ObjectList<Pod>, Api<Pod>), Box<dyn std::error::Error>> {
    let lp = ListParams::default();
    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace.as_str());

    let pod_list: ObjectList<Pod> = pods
        .list(&lp)
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    Ok((pod_list, pods))
}

async fn process_pod_metrics(
    pool: &SqlitePool,
    pod_list: ObjectList<Pod>,
    pods: &Api<Pod>,
    namespace: String,
) -> Result<(), Box<dyn std::error::Error>> {
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
            let p = metrics::process(
                pool,
                pods,
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

#[derive(Parser, Debug, Clone)]
enum Command {
    ScanMetrics,
    Report,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the namespace to walk
    #[arg(short, long)]
    namespace: Option<String>,
    #[arg(short, long, default_value = "/tmp/k8p.db")]
    db_location: String,

    #[clap(subcommand)]
    command: Command,
}

#[allow(clippy::expect_used)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let namespace = args.namespace;
    let db_location = args.db_location;

    let pool = init_db(db_location).await?;
    create_table(&pool).await?;

    let client = Client::try_default()
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    match args.command {
        Command::ScanMetrics => {
            let namespace = namespace.expect("A namespace is required for scan_metrics command");
            let (pod_list, pods) = fetch_pods(client, namespace.clone()).await?;
            process_pod_metrics(&pool, pod_list, &pods, namespace).await?;
        }
        Command::Report => {
            // Here you will implement the reporting logic
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{Connection, SqliteConnection};
    use std::fs;
    use tokio::runtime::Runtime;

    #[test]
    fn test_init_db() {
        let db_location = "/tmp/test_init_k8p.db";

        // Ensure there's no db file before the test
        let _ = fs::remove_file(db_location);

        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let _pool = init_db(db_location.to_string()).await.unwrap();

            // Check if the database has been created successfully
            let mut conn = SqliteConnection::connect(&format!("sqlite:{}", db_location))
                .await
                .unwrap();
            assert!(conn.ping().await.is_ok());
        });

        // Clean up after the test
        let _ = fs::remove_file(db_location);
    }

    #[test]
    fn test_create_table() {
        let db_location = "/tmp/test_k8p.db";

        // Ensure there's no db file before the test
        let _ = fs::remove_file(db_location);

        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = init_db(db_location.to_string()).await.unwrap();

            match create_table(&pool).await {
                Ok(_) => (),
                Err(e) => panic!("create_table failed with {:?}", e),
            }

            // Check if the table has been created
            let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM triples")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(row.0, 0);
        });

        // Clean up after the test
        let _ = fs::remove_file(db_location);
    }
}
