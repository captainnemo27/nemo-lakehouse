#!/bin/sh
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"

if grep -RInE 'ghp_[A-Za-z0-9_]{20,}|AKIA[0-9A-Z]{16}|password *= *"[^"]+"' \
  --exclude-dir=.git \
  --exclude-dir=target \
  "$root"; then
  echo "potential secret found" >&2
  exit 1
fi

echo "security check passed"
