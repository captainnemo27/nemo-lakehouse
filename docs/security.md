# Security

## Controls

- Table names reject absolute paths, separators, and `..`.
- Data file paths must be relative and cannot contain `..`.
- Metadata and snapshot writes use temp-file plus rename.
- CLI arguments are parsed, not executed as shell.
- No secrets are required for the MVP.

## Known Gaps

- Single-writer only.
- No signed metadata.
- No object storage auth model.
- No distributed lock manager.

