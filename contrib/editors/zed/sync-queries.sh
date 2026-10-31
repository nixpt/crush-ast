#!/bin/sh
# Zed extensions are packaged on their own, so the queries are copied (not
# symlinked) from the grammar crate. Re-run after editing the queries.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
src="$here/../../../crates/tree-sitter-crush/queries"
for q in highlights indents injections; do cp "$src/$q.scm" "$here/languages/crush/$q.scm"; done
