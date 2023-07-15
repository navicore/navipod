use clap::Parser;
use k8p::db;
use k8p::pods;

#[derive(Parser, Debug, Clone)]
enum Command {
    ScanMetrics,
    ExportRDF,
    Report,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the file to export RDF to
    #[arg(short, long, default_value = "k8p.rdf")]
    rdf_filename: Option<String>,
    /// Name of the namespace to walk
    #[arg(short, long)]
    namespace: Option<String>,
    #[arg(short, long, default_value = "/tmp/k8p.db")]
    db_location: String,

    #[clap(subcommand)]
    command: Command,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let db_location = args.db_location;

    match args.command {
        Command::ScanMetrics => {
            if let Some(namespace) = args.namespace {
                let pool = db::init(db_location).await?;
                db::create_table(&pool).await?;

                let (pod_list, pods) = pods::fetch(namespace.clone()).await?;
                pods::gather_metrics(&pool, pod_list, &pods, namespace).await;
            } else {
                println!("'namespace' is required for scanning");
            }
        }
        Command::Report => {
            // Here you will implement the reporting logic
        }
        Command::ExportRDF => {
            if let Some(rdf_filename) = args.rdf_filename {
                let pool = db::init(db_location).await?;
                db::export_to_rdf(&pool, &rdf_filename).await?;
            } else {
                println!("'rdf_filename' is required for export");
            }
        }
    }

    Ok(())
}
