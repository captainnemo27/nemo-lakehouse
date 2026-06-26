# Security

## Controls

- Table names reject absolute paths, separators, and `..`.
- Data file paths must be relative and cannot contain `..`.
- Metadata and snapshot writes use temp-file plus rename.
- Metadata and snapshot writes include SHA-256 sidecar checksums.
- CLI arguments are parsed, not executed as shell.
- No secrets are required for the MVP.
- Snapshot files are immutable once written.
- Planning reads graph and virtual-file metadata through typed JSON structures.

## Graph-Native Lakehouse Risks

- Graph poisoning: incorrect partition values, node stats, or virtual-file IDs can bias pruning and return incomplete or excessive file sets.
- Stale virtual file references: graph nodes can point to logical virtual files whose physical file list is missing, deleted, or superseded.
- Range predicate abuse: wide or adversarial range predicates can force traversal of large graph regions and degrade planning into a near-full metadata scan.
- Metadata tampering: direct edits to `_nemo/metadata.json` or snapshot files can rewrite current snapshot pointers, graph dimensions, stats, or file mappings.

## Required Engineering Practices

- Treat graph edges, node stats, and virtual-file mappings as untrusted until validated against the current snapshot lineage.
- Reject absolute or parent-relative paths for every persisted file reference.
- Keep query predicates parsed as data. Do not pass predicate text, table names, paths, or metadata values to a shell.
- Add bounded traversal and predicate normalization before supporting range predicates in production.
- Run `table validate` before trusting copied or externally produced table metadata.
- Run `scripts/security_check.sh` before commits that change Rust source or security docs.

## Known Gaps

- Single-writer only.
- No signed metadata.
- No object storage auth model.
- No distributed lock manager.
- Checksums are local sidecars only; no signed metadata yet.
- No stale-reference sweeper for virtual files.
- No authorization model around graph dimensions, stats, or adaptive optimizer inputs.
