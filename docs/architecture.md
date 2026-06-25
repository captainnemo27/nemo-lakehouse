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

## 3. Immutable Snapshots

Snapshots remain immutable for auditability. Metadata can evolve to point at graph and virtual file updates.

## 4. Delete Bitmap Placeholder

The metadata model reserves per-column stat fields for `delete_bitmap_ref`. A future global delete index can map primary keys to bitmap refs by snapshot.

## 5. Cost Model / AI Optimizer Placeholder

This MVP records graph dimensions and planning behavior. Later versions can use query history to evolve graph dimensions and virtual-file grouping.
