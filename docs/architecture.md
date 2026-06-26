# Architecture

Nemo Lakehouse is organized around five research components.

## 1. Metadata Graph

Graph dimensions are declared at table creation time:

```text
country -> date -> customer -> bucket
```

Each append inserts data files into this graph based on partition values. Query planning follows predicate values directly through the graph.

The current `QueryPlan` exposes comparison metrics:

- `visited_nodes`: graph nodes touched during planning.
- `manifest_scan_physical_files`: physical file entries a manifest/log scan would inspect in the table.
- `selected_physical_files`: physical files returned through virtual files.
- `skipped_physical_files`: file entries pruned before scan.

For a synthetic table with 8 countries, 31 dates, 100 customers, and 2 files per leaf, the graph planner visits 4 nodes and selects 2 files while a manifest-style scan would inspect 49,600 file entries.

## 2. Virtual File Layer

Every append produces a virtual file that can reference one or more physical files. This lets engines plan against larger logical units before expensive compaction rewrites happen.

`compact-plan` is intentionally read-only. It returns candidate merge groups, source virtual-file IDs, physical files, record counts, and suggested output names without changing snapshots or metadata.

## 3. Immutable Snapshots

Snapshots remain immutable for auditability. Metadata can evolve to point at graph and virtual file updates.

## 4. Delete Bitmap Placeholder

The metadata model reserves per-column stat fields for `delete_bitmap_ref`. A future global delete index can map primary keys to bitmap refs by snapshot.

## 5. Cost Model / AI Optimizer Placeholder

This MVP records graph dimensions and planning behavior. Later versions can use query history to evolve graph dimensions and virtual-file grouping.

Current query history is stored in `_nemo/query_history.json` as structured entries containing timestamp, queried dimensions, equality predicates, and range predicates. `optimize --recommend` uses dimension frequency to recommend graph order.

## Metadata Integrity

Every metadata and snapshot JSON write also writes a `.sha256` sidecar. `table validate` verifies:

- `metadata.json.sha256`
- every snapshot checksum in current lineage
- snapshot lineage cycles
- missing virtual-file references in active snapshots
- graph references to missing active virtual files
