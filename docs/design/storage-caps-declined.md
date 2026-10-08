# `storage.*` capabilities — declined (CRUSH-154, decision C-6)

**Status:** declined, 2026-10-07. Reopen only if a real consumer appears that `db.*` and `fs.*`
cannot serve.

## What was proposed

exosphere's stdlib (and the older `nixpt/crush` ancestor) had a handle-based store:
`storage.open` returns a handle, `storage.read` / `storage.write` / `storage.size` act on it, and
`storage.close` releases it, all over exosphere's HAL behind a "store" grant. The CRUSH-150
migration inventory (§2, §5.1) listed it as the only persistence family crush-ast lacks.

## Why it is declined

- **Persistence is already covered.** `db.query` / `db.execute` (`db` feature, `--db PATH`) give
  a program a SQLite database scoped to one path the host chose; `fs.*` (`--fs`, confined to
  `--fs-root`) covers files.
- **Handles are a new kind of VM state.** A handle API needs handle scoping per VM, cleanup on VM
  drop, and use-after-close checks in every backend — CVM1, FastVM, JIT and both AOT targets — for
  a capability with no current Crush consumer.
- **No consumer.** No program in `examples/crush/`, the language guide, or any dependent repo
  calls `storage.*`.

## Mapping for code that used `storage.*`

| `storage.*` use | Use instead | Grant |
|---|---|---|
| open a named store, read/write whole values | `db.execute("CREATE TABLE IF NOT EXISTS kv(k TEXT PRIMARY KEY, v TEXT)")`, then `db.execute("INSERT OR REPLACE INTO kv VALUES (?, ?)", key, value)` / `db.query("SELECT v FROM kv WHERE k = ?", key)` | `--db PATH` |
| append records, read them back in order | a table with an `INTEGER PRIMARY KEY` and `db.query(... ORDER BY id)` | `--db PATH` |
| `storage.size` | `db.query("SELECT length(v) FROM kv WHERE k = ?", key)`, or `str.len(fs.read(path))` for a file | `--db` / `--fs` |
| a file-shaped blob | `fs.write(path, data)` / `fs.read(path)` | `--fs` (`--fs-root DIR`) |
| `storage.close` | nothing — `db.*` and `fs.*` hold no per-program handles | — |

## If it is ever reopened

Put the caps behind their own grant (`--store DIR`), scope handles to the VM, close them when the
VM is dropped, and test use-after-close — the success criteria CRUSH-154 already wrote down.
