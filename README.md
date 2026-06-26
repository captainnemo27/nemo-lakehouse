# Nemo Lakehouse

Nemo Lakehouse is a Rust research MVP for a graph-native open table format.

It is not an Iceberg clone. The goal is to test whether a **Metadata Graph** plus a **Virtual File Layer** can reduce query planning cost and compaction IO compared with manifest/log-oriented table formats.

## Core Ideas

### Metadata Graph

Instead of:

```text
Snapshot -> Manifest List -> Manifest -> Data File
```

Nemo stores:

```text
Table Graph
country
  VN
    date
      2026-06
        customer
          123
            bucket
              07 -> virtual files
```

Each graph node can store:

- row count
- file count
- min/max stats
- NDV placeholder
- null count placeholder
- delete bitmap refs placeholder

Planning can follow predicates directly instead of scanning all manifests.

### Virtual File Layer

Small files can be grouped into a logical file:

```text
virtual-file-1
  data/a.parquet
  data/b.parquet
  data/c.parquet
```

Engines can plan against virtual files while physical rewrites are deferred.

## Quick Start

With local Rust:

```bash
cargo test
cargo run --bin nemo -- table create ./warehouse/events \
  --schema examples/event_schema.json \
  --graph-dim country --graph-dim date --graph-dim customer

cargo run --bin nemo -- table append ./warehouse/events \
  --file data/events-0001.parquet \
  --records 100 \
  --partition country=VN \
  --partition date=2026-06 \
  --partition customer=123

cargo run --bin nemo -- table plan ./warehouse/events \
  --predicate country=VN \
  --predicate date=2026-06

cargo run --bin nemo -- table compact-plan ./warehouse/events \
  --partition country=VN \
  --partition date=2026-06 \
  --target-file data/events-vn-compact.parquet

cargo run --bin nemo -- table validate ./warehouse/events
cargo run --bin nemo -- table query-history ./warehouse/events
```

With Docker:

```bash
docker compose run --rm dev cargo test
```

## Metadata Graph Benchmark

Run a synthetic planning benchmark without writing a warehouse:

```bash
docker compose run --rm dev /usr/local/cargo/bin/cargo run --bin nemo -- bench graph \
  --countries 8 \
  --dates 31 \
  --customers 100 \
  --files-per-leaf 2 \
  --country C001 \
  --date 2026-06-01 \
  --customer cust-000001
```

Expected shape:

```json
{
  "manifest_scan_physical_files": 49600,
  "selected_physical_files": 2,
  "selected_virtual_files": 1,
  "skipped_physical_files": 49598,
  "visited_nodes": 4
}
```

This is the research target: for selective predicates, graph planning follows `root -> country -> date -> customer` instead of scanning every manifest/data-file entry.
