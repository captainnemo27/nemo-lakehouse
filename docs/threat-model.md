# Threat Model

## Assets

- `_nemo/metadata.json`
- `_nemo/snapshots/*.json`
- Metadata Graph nodes and stats
- Virtual File mappings
- Future delete bitmap refs
- Graph dimensions and partition-value edges
- Predicate inputs used by planners and future adaptive optimizers

## Threats

- Path traversal escaping a warehouse/table directory
- Metadata corruption from interrupted writes
- Conflicting concurrent writers
- Stale graph nodes causing incorrect query plans
- Poisoned query history in future adaptive optimizers
- Graph poisoning through malicious or malformed partition values that create misleading edges or aggregate stats.
- Stale virtual file references where graph nodes keep IDs for virtual files that no longer exist or no longer match their physical file list.
- Range predicate abuse where broad, overlapping, or malformed ranges cause excessive traversal and planning latency.
- Metadata tampering that changes current snapshot pointers, graph dimensions, node stats, or virtual-file mappings outside the commit path.

## Abuse Scenarios

- A writer appends a file with forged partition values so selective predicates skip legitimate files or include attacker-chosen files.
- A stale graph node references a virtual file removed from `virtual_files`, producing silent under-selection if planners ignore the missing mapping.
- A user submits a very broad range predicate after range support is added, forcing traversal of most graph branches and increasing CPU cost.
- An operator or compromised process edits `_nemo/metadata.json` directly, moving `current_snapshot_id` or mutating graph stats without an audit trail.

## Mitigations In MVP

- Strict path validation.
- Atomic local metadata replacement.
- Immutable snapshot files.
- Single-writer assumption documented.
- Typed predicate parsing in the CLI.
- No shell execution path for user-supplied table names, paths, or predicates.

## Required Future Mitigations

- Validate graph membership against snapshot lineage before planning.
- Fail closed when graph nodes reference missing virtual-file IDs.
- Add traversal limits, normalized predicate forms, and cost guards for range predicates.
- Add metadata signatures, checksums, or content-addressed commits before multi-writer or object-storage deployments.
- Record lineage and audit events for graph dimension changes and optimizer-driven graph rewrites.
