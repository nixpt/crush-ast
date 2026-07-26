# CRUSH-70 — Concurrent AOT compiles of one program clobber each other

| Field | Value |
|-------|-------|
| **ID** | CRUSH-70 |
| **Priority** | P1 — AOT tier unusable on a cold cache under any parallelism |
| **Status** | Done (PR pending) |
| **Phase** | AOT / build hygiene |
| **Assignee** | nixp |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

`AotCompiler::compile_casm` built the shared library straight into the shared
cache dir (`-o $TMPDIR/crush-aot-cache/<name>.tmp.<pid>`). `rustc` scatters its
thin-LTO intermediate objects next to the `-o` path under names derived from the
*crate name*, not the output file name, so two compiles of the same program
racing on the same cold cache entry write and then delete each other's
`.rcgu.o` files. The loser dies at link time:

```
rust-lld: error: cannot open /tmp/crush-aot-cache/bozo_crush_51a70dfbc106dfdf...rcgu.o:
No such file or directory
```

The `.tmp.<pid>` suffix that was meant to prevent this does not help: threads in
one test binary share a pid, and the intermediates ignore the output name anyway.

Found by bozo's first CI run (bozo M6 landed a workflow). Three `bozo` dispatch
tests fail on any machine with a cold `/tmp/crush-aot-cache`; they pass locally
only because a warm cache short-circuits the compile.

## Success criteria

- [x] Each compile builds inside its own work dir, then publishes into the cache
- [x] Work dirs are unique per invocation, not per process (thread-safe)
- [x] Cross-filesystem custom cache dirs still work (rename falls back to copy)
- [x] Regression test: 4 threads compile one source against a cold cache dir
- [x] `bozo`'s dispatch suite passes with `/tmp/crush-aot-cache` removed

## Non-goals

- Inter-process locking of cache entries (rename stays the publish primitive;
  concurrent winners are bit-identical artifacts)
- Cache eviction / size bounds
- The `c_` (gcc/clang) path's separate `-flto` behavior, beyond the same
  build-then-publish treatment

## Resolution

Shipped on `agent/foreman/CRUSH-70-aot-parallel-compile`: added
`unique_work_dir()` (pid + atomic sequence) and `publish_artifact()`
(rename, falling back to copy across filesystems) in `crush-aot/src/compiler.rs`,
and routed both the rustc and the C paths through them.
