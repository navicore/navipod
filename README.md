A TUI and CLI for Inspecting Containers in Kubernetes
============

![an image showing navipod inspecting replicas and pod and ingress](docs/demo077.gif)

Alpha - Under constant development as I practice Rust programming and discover
needs not met by other tools.

The command uses the local kubecontext credentials to access Kubernetes clusters.

The primary use case of the tool is to get quick answers to replica and pod
and ingress state.

The tool also captures data to an embedded DB for exporting as RDF.

It will get Prometheus data from pods which are annotated for Prometheus
using the convention:

```
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/path: "/actuator/prometheus"
        prometheus.io/port: "8081"
```

The cli supports exporting the db to both N-Triple and Turtle RDF files.

Install
----------

```bash
#latest stable version via https://crates.io/crates/navipod
cargo install navipod

#or from this repo:
cargo install --path .
```

Configure for tab completion:

```bash
navipod generate-completion zsh > /usr/local/share/zsh/site-functions/_navipod
```

Usage
---------

from `navipod -h`

```
A cli tool for inspecting containers in Kubernetes

Usage: navipod [OPTIONS] <COMMAND>

Commands:
  tui                  start text-based UI
  explain-pod          report on pod external ingress
  scan-metrics         collect pod metrics and write to db
  export-triples       export db data to RDF nt files
  export-turtle        export db data to RDF turtle files
  report               show db stats
  generate-completion  generate completion script for bash and zsh
  help                 Print this message or the help of the given subcommand(s)

Options:
  -t, --ttl-rdf-filename <TTL_RDF_FILENAME>  export Turtle RDF file [default: navipod.ttl]
  -r, --rdf-filename <RDF_FILENAME>          export N-Triples RDF file [default: navipod.nt]
  -n, --namespace <NAMESPACE>                Name of the namespace to walk
  -d, --db-location <DB_LOCATION>            [default: /tmp/navipod.db]
  -h, --help                                 Print help
  -V, --version                              Print version
```

