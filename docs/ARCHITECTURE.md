# Architecture

## Context & Scope

NaviPod is a Rust binary (`navipod`) providing a TUI and CLI for inspecting
Kubernetes clusters. It runs as a user-space client — no server component, no
agent running in-cluster.

**External systems it talks to:**
- **Kubernetes API server** — via `kube` + `k8s-openapi`, authenticated with
  the user's local kubeconfig (`~/.kube/config`). Used for listing
  ReplicaSets, Pods, Containers, Events, Ingresses, Namespaces, and for
  executing probes (`exec` subresource).
- **Pod endpoints** — direct HTTP(S) to pods annotated with
  `prometheus.io/scrape=true` to pull Prometheus metrics, and direct
  probe calls (HTTP/TCP).
- **TLS endpoints** — raw TCP + `tokio-rustls` handshake to inspect x509
  certificates for ingress hosts.
- **Local SQLite file** — `/tmp/navipod.db` by default, used by the scan/RDF
  export subcommands (`scan-metrics`, `export-turtle`, `export-triples`,
  `report`).

Everything else (the cache, the UI, the metrics history store) is in-process.

## Solution Strategy

- **Rust 2024 edition**, async via **tokio** multi-thread runtime.
- **`kube` + `k8s-openapi`** for typed K8s access; **watch streams** drive
  cache invalidation, list calls drive initial loads and cache misses.
- **`ratatui` + `crossterm`** for the TUI; alternate-screen mode, mouse
  capture on. Keystrokes flow through an async stream (`tui::stream`).
- **`rustls` (aws-lc-rs)** for TLS — installed once as the default crypto
  provider at startup. `reqwest` is configured with `rustls-tls`.
- **`sqlx` with SQLite** for the RDF/metrics-scan persistence path. Schema
  is a single `triples(subject, predicate, object)` table — schema-less by
  design to fit the RDF export use case.
- **`tracing` → file** (`/tmp/navipod.log`): the TUI can't share stdout with
  log output, so logs are redirected to a file unconditionally.
- **Global singletons via `OnceLock`** for the cache, background fetcher,
  watch manager, metrics-history store, and shutdown channels. Namespace
  state is in a `RwLock` so it can be swapped when the user changes
  namespace.

## Building Blocks

### Top-level crate layout (`src/`)

- **`main.rs`** — clap-based CLI entry. Subcommands: `tui`, `explain-pod`,
  `scan-metrics`, `export-triples`, `export-turtle`, `report`,
  `generate-completion`. Default is `tui`.
- **`lib.rs`** — exposes `cache_manager`, `error`, `k8s`, `net`, `tui`.
- **`cache_manager.rs`** — process-wide singletons and lifecycle
  (`initialize_cache`, `shutdown_cache`, namespace switching, network-activity
  counters for UI indicators).
- **`error.rs`** — `Error` enum via `derive_more::From` wrapping
  `serde_json`, `kube`, `InferConfigError`, `http`, `io`, plus a `Custom(String)`
  variant. `Result<T>` alias.

### `k8s/` — Kubernetes integration

- **`client.rs` / `client_manager.rs`** — kube `Client` construction; lenient
  vs strict user-agent handling. `USER_AGENT` defined in `k8s/mod.rs` as
  `navipod/<pkg-version>`, overridable via `NAVIPOD_USER_AGENT`.
- **Resource modules** — `pods`, `rs` (ReplicaSets), `ds` (DaemonSets),
  `ss` (StatefulSets), `jobs`, `cronjobs`, `containers`, `events`,
  `namespaces`, `resources`, `rs_ingress`, `pod_ingress`, `probes`.
- **`metrics_client.rs` / `metrics_history.rs`** — Prometheus scraping and
  an in-memory time-series store for sparkline rendering in the TUI.
- **`cache/`** — the active data layer (see below).
- **`scan/`** — the offline/RDF path. `db`, `pods`, `metrics`, `triples`,
  `tuples`. Writes subject/predicate/object rows into SQLite and exports to
  N-Triples or Turtle.

### `k8s/cache/` — background data cache

The core invariant: **widgets never call the K8s API directly.** They issue
`DataRequest`s; the cache returns fresh data if available, otherwise the
`BackgroundFetcher` fills it. `WatchManager` runs K8s watch streams for
Pods, ReplicaSets, DaemonSets, StatefulSets, Jobs, CronJobs, and Events
(seven watchers total, tracked by `ACTIVE_WATCHER_COUNT`) and pushes
`InvalidationEvent`s so the cache stays current without polling.

- **`data_cache.rs`** — `K8sDataCache`: `HashMap<String, CachedEntry>` behind
  a `tokio::sync::RwLock`, with a memory-byte budget and a
  `SubscriptionManager`. Cache keys come from `DataRequest::cache_key()`.
- **`fetcher.rs`** — `DataRequest` enum (`ReplicaSets`, `DaemonSets`,
  `StatefulSets`, `Jobs`, `CronJobs`, `Pods`, `Containers`, `Events`,
  `Ingresses`, `Custom`), `PodSelector` (including `Unowned` and
  `ByJob(String)`), `ResourceRef`, `FetchParams`, `FetchPriority`,
  `FetchResult`, `DataFetcher` trait.
- **`background_fetcher.rs`** — priority-queue-driven worker pool; started
  via `start()` which returns `(Arc<Self>, mpsc::Sender<()>)` for shutdown.
- **`watch_manager.rs`** — long-running watch loops per resource type with
  bounded backoff (`INITIAL_BACKOFF_SECONDS`, `MAX_BACKOFF_SECONDS`,
  `MAX_WATCH_RESTARTS`). Emits `InvalidationEvent::Pattern | Key | Update`.
- **`cached_data.rs`** — `CachedData<T>` with `last_updated`, `ttl`,
  `FetchStatus { Fresh, Stale, Fetching, Error }`.
- **`subscription.rs`** — pub/sub so widgets get `DataUpdate`s when their
  slice of cache changes.
- **`config.rs`** — TTLs, memory budget, concurrency limits,
  channel capacities. All magic numbers live here.

### `tui/` — terminal UI

- **`ui_loop.rs`** — the `run()` entry point, terminal setup/teardown, the
  `AppBehavior` trait (`handle_event`, `draw_ui`, `stream`), the `Apps` enum
  used as the navigation target, and a `FORCE_QUIT` atomic for `Q`.
- **Per-view apps** — `rs_app`, `pod_app`, `container_app`, `log_app`,
  `ingress_app`, `cert_app`, `event_app`, `namespace_app`. Each has
  `app.rs` (state + `AppBehavior` impl), `mod.rs`, and a `modern_ui.rs`
  (or `ui.rs` for events) that owns rendering.
- **`data.rs`** — shared view-model types (`Rs`, `RsPod`, `Container`,
  `Ingress`, `ResourceEvent`, the `Filterable` trait). These are the types
  that flow through `DataRequest` / `FetchResult`.
- **`common/`** — shared scaffolding: `AppController`, `DomainService` and
  `NavigationHandler` traits, `base_table_state`, `key_handler`,
  `stream_factory`.
- **`theme.rs`** — `NaviTheme` struct; `c` cycles themes at runtime.
- **`sparkline.rs`, `table_ui.rs`, `yaml_editor.rs`, `clipboard.rs`,
  `stream.rs`, `style.rs`, `utils/`** — cross-view helpers.

### `net/` — TLS inspection

Single module (`net/mod.rs`): `analyze_tls_certificate(host)` opens a TCP
connection, performs a TLS handshake via `tokio-rustls`, parses the peer's
leaf cert with `x509-parser`, and returns `CertificateInfo { host, is_valid,
expires, issued_by }` for the cert view.

### Core domain entities (the language the code uses)

- **`Rs`** — a ReplicaSet projection with pods rolled up.
- **`RsPod`** — a pod as seen from the replicaset view (owns containers).
- **`Container`** — a container projection including probes, env, mounts.
- **`Ingress`** — ingress rule with TLS host list.
- **`ResourceEvent`** — a K8s `Event` flattened for table display.

The **navigation invariant** is a Workloads landing (ReplicaSet,
DaemonSet, StatefulSet, Job, CronJob, plus a synthesized `Unowned` row
for static/nodeless pods) → Pod → Container → Logs, with
`ReplicaSet → Ingress → Cert` as a side branch. CronJob selection
resolves to the latest active child Job via `owner_references` before
routing. The cache key shape and `DataRequest` variants mirror this tree.

## Crosscutting Concepts

- **Error handling.** Library code uses `crate::error::Result<T>`;
  `main.rs` and some async boundaries use `Box<dyn Error>`. Fetchers return
  typed errors; cache marks entries `FetchStatus::Error(String)` rather than
  propagating panics.
- **Concurrency model.** Single tokio multi-thread runtime. Shared state is
  `Arc<RwLock<_>>` (`tokio::sync::RwLock` for cache data touched across
  `.await`, `std::sync::RwLock` for sync-access state like the namespace).
  Singletons use `OnceLock`. Shutdown is cooperative via `mpsc::Sender<()>`
  capability tokens.
- **Data access discipline.** Widgets must go through the cache — direct
  `kube::Api` calls from UI code are a layering violation. The cache and
  background fetcher exist so view switches are non-blocking.
- **Logging.** `tracing` everywhere; subscriber writes to
  `/tmp/navipod.log` because the TUI owns stdout. `navipod=debug`,
  `navipod::k8s::cache=info` by default; override via `RUST_LOG`/env
  filter.
- **TLS crypto.** `rustls::crypto::CryptoProvider::install_default(ring)` at
  startup — required before any TLS client is built.
- **User agent.** Every outbound K8s/HTTP client uses
  `k8s::USER_AGENT` (`navipod/<version>`), overridable via
  `NAVIPOD_USER_AGENT`. Tests/dev use strict mode that fails on invalid
  headers.
- **File-length convention.** The project aims to keep individual source
  files under ~200 lines where practical; several cache and UI files
  exceed this and are candidates for splitting.
