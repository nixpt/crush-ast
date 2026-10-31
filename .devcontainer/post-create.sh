#!/usr/bin/env bash
# One-time setup after the dev container is created (Codespaces / Dev Containers).
# Mirrors what CI does before building crush-ast. Safe to re-run.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "==> system packages: C toolchain for the C AOT backend, Python 3.12 with a shared libpython for crush-python (pyo3)"
sudo apt-get update -qq
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends \
  build-essential clang pkg-config libssl-dev python3 python3-dev python3-venv

echo "==> sibling buckets checkout (the workspace path-depends on ../buckets, as in CI)"
if [ ! -d ../buckets/.git ]; then
  git clone --depth 1 https://github.com/nixpt/buckets.git ../buckets
else
  echo "    ../buckets already present"
fi

echo "==> Rust toolchain from rust-toolchain.toml, plus the wasm32 target crush-web needs"
rustup toolchain install   # no argument: installs what rust-toolchain.toml pins
rustup target add wasm32-unknown-unknown

echo "==> prefetch crates"
cargo fetch --locked

cat <<'EOF'

crush-ast dev container is ready.

  Build the CLI:   cargo build -p crush-lang-sdk --bin crush-run
  Run a program:   target/debug/crush-run run examples/crush/<file>.crush
  Polyglot blocks: add --polyglot (python3 and node are installed)
  Tests:           see the jobs in .github/workflows/ci.yml for the exact package lists

EOF
