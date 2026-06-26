# Metadata Graph Demo

```bash
cargo run --bin nemo -- table create ./warehouse/events \
  --schema examples/event_schema.json \
  --graph-dim country \
  --graph-dim date \
  --graph-dim customer

cargo run --bin nemo -- table append ./warehouse/events \
  --file data/events-vn-2026-06-c123-0001.parquet \
  --file data/events-vn-2026-06-c123-0002.parquet \
  --records 100 \
  --partition country=VN \
  --partition date=2026-06 \
  --partition customer=123

cargo run --bin nemo -- table plan ./warehouse/events \
  --predicate country=VN \
  --predicate date=2026-06 \
  --predicate customer=123
```

Catalog workflow:

```bash
cargo run --bin nemo -- catalog create ./warehouse analytics.events \
  --schema examples/event_schema.json \
  --graph-dim country \
  --graph-dim date \
  --graph-dim customer

cargo run --bin nemo -- catalog list ./warehouse --details

cargo run --bin nemo -- catalog inspect ./warehouse analytics.events
```

Range predicates can prune graph dimensions with inclusive `start..end` bounds:

```bash
cargo run --bin nemo -- table plan ./warehouse/events \
  --predicate country=VN \
  --range date=2026-06-01..2026-06-30
```

Synthetic benchmark:

```bash
cargo run --bin nemo -- bench graph \
  --countries 8 \
  --dates 31 \
  --customers 100 \
  --files-per-leaf 2 \
  --country C001 \
  --date 2026-06-01 \
  --customer cust-000001
```
