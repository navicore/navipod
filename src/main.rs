use clap::{CommandFactory, Parser}; // CommandFactory is needed for into_app()
use clap_complete::{generate, Shell};
use kube::{config::KubeConfigOptions, Config};
use navipod::k8s::pod_ingress;
use navipod::k8s::scan::db;
use navipod::k8s::scan::pods;
use navipod::tui;

#[derive(Parser, Debug, Clone)]
enum Command {
    Tui,
    ExplainPod { podname: String },
    ScanMetrics,
    ExportTriples,
    ExportTurtle,
    Report,
    GenerateCompletion { shell: Shell },
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// export Turtle RDF file
    #[arg(short, long, default_value = "navipod.ttl")]
    ttl_rdf_filename: Option<String>,
    /// export N-Triples RDF file
    #[arg(short, long, default_value = "navipod.nt")]
    rdf_filename: Option<String>,
    /// Name of the namespace to walk
    #[arg(short, long)]
    namespace: Option<String>,
    #[arg(short, long, default_value = "/tmp/navipod.db")]
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
    let namespace = if let Some(n) = args.namespace {
        n
    } else {
        let config = Config::from_kubeconfig(&KubeConfigOptions::default()).await?;
        config.default_namespace
    };

    match args.command {
        Command::Tui => {
            tui::run()?;
        }
        Command::GenerateCompletion { shell } => {
            let app = Args::command();
            generate(
                shell,
                &mut app.clone(),
                app.get_name(),
                &mut std::io::stdout(),
            );
        }
        Command::ExplainPod { podname } => {
            pod_ingress::explain(&namespace, &podname).await?;
        }
        Command::ScanMetrics => {
            db::create_table(&pool).await?;
            let (pod_list, pods) = pods::fetch(namespace.clone()).await?;
            pods::gather_metrics(&pool, pod_list, &pods, namespace).await;
        }
        Command::Report => {
            // Here you will implement the reporting logic
            let report = db::report(&pool).await?;
            println!("{report}");
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
