#!/usr/bin/env bash
# Build crush-web for wasm32, generate the JS glue, stage a site with the
# harness and examples/crush/blackjack_interactive.crush, and play it in headless Chromium.
#
#   WASM_BINDGEN=/path/to/wasm-bindgen scripts/browser-test.sh [site-dir]
#
# Needs: the wasm32-unknown-unknown target, a wasm-bindgen CLI whose version
# matches the `wasm-bindgen` locked in this crate's Cargo.lock, and python3
# with playwright + its Chromium.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
wb="${WASM_BINDGEN:-wasm-bindgen}"
want="$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/"/, "", $3); print $3 }' "${here}/Cargo.lock")"
have="$("${wb}" --version | awk '{print $2}')"
if [[ "${have}" != "${want}" ]]; then
  echo "wasm-bindgen CLI is ${have}, Cargo.lock has ${want}: install the matching CLI" >&2
  exit 1
fi
cargo build --release --target wasm32-unknown-unknown --manifest-path "${here}/Cargo.toml"
target="$(cargo metadata --format-version 1 --no-deps --manifest-path "${here}/Cargo.toml" | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
site="${1:-${target}/crush-web-site}"
rm -rf "${site}" && mkdir -p "${site}/fixtures"
"${wb}" --target web --out-dir "${site}/pkg" "${target}/wasm32-unknown-unknown/release/crush_web.wasm"
cp "${here}/www/index.html" "${site}/"
cp "${here}/../../examples/crush/blackjack_interactive.crush" "${site}/fixtures/"
python3 "${here}/scripts/browser_test.py" "${site}"
