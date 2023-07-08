use clap::Parser;
use k8s_openapi::api::core::v1::Pod;
use kube::api::ObjectList;
use kube::{
    api::{Api, ListParams},
    Client,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[arg(short, long)]
    namespace: String,
}

#[tokio::main]
async fn main() -> Result<(), kube::Error> {
    let args = Args::parse();
    let namespace = args.namespace;
    let client = Client::try_default().await?;

    let pods: Api<Pod> = Api::namespaced(client, namespace.as_str());
    let lp = ListParams::default();

    let pod_list: ObjectList<Pod> = pods.list(&lp).await?;

    for p in pod_list.items {
        let metadata_name = p.metadata.name.unwrap();
        for container in p.spec.unwrap().containers {
            if let Some(readiness_probe) = container.readiness_probe {
                if let Some(http_get) = readiness_probe.http_get {
                    let port = match http_get.port {
                        k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(i) => {
                            i.to_string()
                        }
                        k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::String(s) => s,
                    };
                    println!(
                        "Readiness Probe - Pod: {}, Path: {}, Port: {}",
                        metadata_name,
                        http_get.path.unwrap(),
                        port
                    );
                }
            }

            if let Some(liveness_probe) = container.liveness_probe {
                if let Some(http_get) = liveness_probe.http_get {
                    let port = match http_get.port {
                        k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(i) => {
                            i.to_string()
                        }
                        k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::String(s) => s,
                    };
                    println!(
                        "Liveness Probe - Pod: {}, Path: {}, Port: {}",
                        metadata_name,
                        http_get.path.unwrap(),
                        port
                    );
                }
            }
        }
    }

    Ok(())
}
