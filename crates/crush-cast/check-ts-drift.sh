#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINDINGS_FILE="${SCRIPT_DIR}/bindings/cast.d.ts"

if [[ ! -f "${BINDINGS_FILE}" ]]; then
    echo "ERROR: ${BINDINGS_FILE} not found. Run the export first."
    exit 1
fi

# Save current committed binding
COMMITTED=$(mktemp)
cp "${BINDINGS_FILE}" "${COMMITTED}"

# Re-export from Rust types
cargo run -p crush-cast --bin export-ts --features ts-export --quiet

# Compare
if ! diff -q "${COMMITTED}" "${BINDINGS_FILE}" > /dev/null 2>&1; then
    echo "ERROR: TypeScript bindings are out of sync with Rust types."
    echo "Run: cargo run -p crush-cast --bin export-ts --features ts-export"
    rm "${COMMITTED}"
    exit 1
fi

echo "OK: TypeScript bindings are in sync with Rust types."
rm "${COMMITTED}"
