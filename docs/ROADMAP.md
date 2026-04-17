# Roadmap

## Current state

- TUI and CLI both functional: `tui`, `explain-pod`, `scan-metrics`,
  `export-triples`, `export-turtle`, `report`, `generate-completion`.
- Background cache + watch-driven invalidation in place for Pods,
  ReplicaSets, and Events; Containers and Ingresses go through the cache
  via list-fetchers.
- Prometheus metrics collection with in-memory history store feeds
  sparklines in the TUI.
- TLS certificate inspection for ingress hosts.
- SQLite-backed RDF scan/export path (N-Triples and Turtle).
- Active dependabot hygiene on `main`; version bumps land via merged PRs.

## Known next steps / open threads

- **UI / data-loop separation** — the UI loop and data loop should
  communicate only via channels. Partial today: the watch manager and
  background fetcher are decoupled, but several UI paths still `.await`
  cache reads inline. Also need a proper timeout/interrupt for keystroke
  polling rather than a busy poll.
- **Background-cache plan** (`docs/design/background-cache-plan.md`)
  tracks the broader rollout. Phases 1–2 are substantially landed
  (cache, background fetcher, subscriptions, watch-based invalidation,
  migrated views). Remaining items:
  - Phase 3: navigation-pattern-aware prefetching, compression/lazy
    deserialization, stricter memory limits.
  - Phase 4: freshness indicators in UI, offline mode with staleness
    banners, cross-view search.
- **Widget data registry** (`docs/design/widget-data-registry.md`) is
  aspirational — describes a widget-requirements/registry model that is
  only partially realized. Either drive the code toward it or prune it;
  should not be treated as current design.
- **File-size hygiene:** `cache_manager.rs`, `tui/data.rs`,
  `tui/ui_loop.rs`, `tui/theme.rs`, `tui/yaml_editor.rs`, and several
  `k8s/*.rs` files exceed the ~200-line target and are candidates for
  splitting.
- **Log path is hardcoded** to `/tmp/navipod.log`; likewise the default
  DB at `/tmp/navipod.db`. Neither is configurable through CLI flags
  beyond `--db-location`.
- **Release process** is manual per the user's git rules — version bumps
  and crates.io publishes are owner-driven, not automated from this
  codebase.
