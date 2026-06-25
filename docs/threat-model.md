# Threat Model

## Assets

- `_nemo/metadata.json`
- `_nemo/snapshots/*.json`
- Metadata Graph nodes and stats
- Virtual File mappings
- Future delete bitmap refs

## Threats

- Path traversal escaping a warehouse/table directory
- Metadata corruption from interrupted writes
- Conflicting concurrent writers
- Stale graph nodes causing incorrect query plans
- Poisoned query history in future adaptive optimizers

## Mitigations In MVP

- Strict path validation.
- Atomic local metadata replacement.
- Immutable snapshot files.
- Single-writer assumption documented.

