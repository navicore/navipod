use clap::Parser;
use k8p::db;
use k8p::pods;

#[derive(Parser, Debug, Clone)]
enum Command {
    ScanMetrics,
    ExportTriples,
    ExportTurtle,
    Report,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// export Turtle RDF file
    #[arg(short, long, default_value = "k8p.ttl")]
    ttl_rdf_filename: Option<String>,
    /// export N-Triples RDF file
    #[arg(short, long, default_value = "k8p.nt")]
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
    let pool = db::init(db_location).await?;

    match args.command {
        Command::ScanMetrics => {
            if let Some(namespace) = args.namespace {
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
        Command::ExportTurtle => {
            if let Some(ttl_rdf_filename) = args.ttl_rdf_filename {
                db::export_to_ttl_rdf(&pool, &ttl_rdf_filename).await?;
            } else {
                println!("'rdf_filename' is required for export");
            }
        }
        Command::ExportTriples => {
            if let Some(rdf_filename) = args.rdf_filename {
                db::export_to_nt_rdf(&pool, &rdf_filename).await?;
            } else {
                println!("'rdf_filename' is required for export");
            }
        }
    }

    Ok(())
}
