#!/bin/sh
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"

scan() {
  label="$1"
  pattern="$2"
  shift 2

  if grep -InE "$pattern" "$@"; then
    echo "$label" >&2
    exit 1
  fi
}

rust_files=$(find "$root/src" -type f -name '*.rs' | sort)
doc_files=$(find "$root/docs" -type f -name '*.md' | sort)

# Hardcoded token checks are scoped to source and docs so this script can carry
# detection regexes without matching itself.
scan "potential GitHub token found" \
  'gh[psru]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,}' \
  $rust_files $doc_files

scan "potential AWS access key id found" \
  '(^|[^A-Z0-9])(AKIA|ASIA)[0-9A-Z]{16}([^A-Z0-9]|$)' \
  $rust_files $doc_files

scan "potential AWS secret material found" \
  '(aws_secret_access_key|AWS_SECRET_ACCESS_KEY|aws_session_token|AWS_SESSION_TOKEN)[[:space:]]*[:=][[:space:]]*["'\''"]?[A-Za-z0-9/+=]{20,}' \
  $rust_files $doc_files

# Flag direct process execution APIs and shell interpreter command strings in
# Rust. This intentionally avoids bare "Command" so clap subcommand enums do
# not trip the check.
scan "unsafe Rust process execution pattern found" \
  'std::process::Command|process::Command|Command::new[[:space:]]*\(|\.arg[[:space:]]*\([[:space:]]*["'\''"](-c|/bin/sh|/bin/bash|sh|bash)["'\''"]' \
  $rust_files

echo "security check passed"
