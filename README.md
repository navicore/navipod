A CLI for Inspecting Containers in Kubernetes
============

UNDER CONSTRUCTION
-----------

Currently, the command will use a local kubecontext to access a cluster
and get all the Prometheus data available from pods annotated for Prometheus
using the convention:

```
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/path: "/actuator/prometheus"
        prometheus.io/port: "8081"
```

Planed - formatted output and liveliness and readyness probe info
in aggregated report form.

Install
----------

```bash
#latest stable version via https://crates.io/crates/k8p
cargo install k8p

#or from this repo:
cargo install --path .
```

