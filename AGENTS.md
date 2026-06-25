# AGENTS.md

## Mission

Build **Nemo Lakehouse**, a Rust research implementation of a next-generation open table format focused on architecture-level advantages over Iceberg and Delta.

The core hypothesis:

- Iceberg and Delta still depend on metadata structures that require scanning many manifests/log entries.
- Nemo should use a **Metadata Graph** so planning can follow indexed dimensions and approach `O(log n)` or `O(1)` for common predicates.
- Nemo should use a **Virtual File Layer** to reduce compaction rewrites by grouping small physical files into logical files.

## Agent Roles

### dev-1-core

Owns:
- `src/schema.rs`
- `src/metadata.rs`
- `src/graph.rs`
- `src/table.rs`
- `src/error.rs`

Focus:
- Metadata Graph
- Virtual File Layer
- Snapshot commits
- Atomic metadata writes
- Planning APIs

### dev-2-integration

Owns:
- `src/catalog.rs`
- `src/bin/nemo.rs`
- `examples/`

Focus:
- Local catalog
- CLI
- Pipeline examples
- Future engine integration surface

### qa

Owns:
- `tests/`

Focus:
- Graph planning tests
- Virtual file tests
- Commit correctness
- CLI/catalog behavior

### security

Owns:
- `docs/security.md`
- `docs/threat-model.md`
- `scripts/security_check.sh`

Focus:
- Path traversal
- Metadata corruption
- Unsafe shell/process patterns
- Secret scanning

### devops

Owns:
- `Dockerfile`
- `compose.yaml`
- `Makefile`
- `.github/workflows/ci.yml`
- `.gitignore`

Focus:
- Rust build/test workflow
- Docker validation
- CI

