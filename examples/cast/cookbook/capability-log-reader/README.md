# capability-log-reader

Demonstrates `ImportStatement::Capability` plus the `CapabilityCall`
expression — the production pattern for everything effectful in CAST
(the squad-tool capsules under `projects/crush-capsules/` print, read
files, and shell out exclusively through `CapabilityCall`).

The import compiles to a `cap.acquire` call and registers
`fs.read:/var/log/exo` in the compiled manifest; the `capsule.toml`
`[capabilities] required` list is the runtime-enforced counterpart.

Note: `CapabilityCall` is the one expression whose `meta` field is
required (no serde default) — always include `"meta": {}`.

```bash
ant crush cast validate main.cast.json
ant crush compile --from-cast main.cast.json -o capability-log-reader.casmb
```
