# TODO

One-line items; real work goes in `.jagent/planning/tickets/` (get an ID with
`cargo run -p xtask --bin ticket-id -- next`). Rewritten 2026-10-31: the old list (AI opcodes, JIT
phases 2–5, debugger inspection, MOD sign, EXEC_LANG) had shipped — see CRUSH-1/32, the M2 row in
ROADMAP.md, and CRUSH-317. The status board is `planning/BACKLOG-INDEX.md`; 1.0 gaps are in
`docs/planning/RELEASE-1.0-CRITERIA.md`.

## Release

- [ ] Cut 0.9.0 (`release.yml`), check its pre-release artifacts, publish (`publish.yml tag=v0.9.0`)
- [ ] Backfill v0.8.0 artifacts (`artifacts.yml -f tag=v0.8.0`)
- [ ] First publish of the 15 never-published crates (hold the walkers until they settle)
- [ ] crush-vm off the `../buckets` path dep (nixpt/buckets#7)

## Correctness

- [ ] #258 `@capabilities` ignored by crushc · #261 ungranted AI op → null (CRUSH-234)
- [ ] CRUSH-110 `import` · CRUSH-219 catchable runtime errors on every engine · CRUSH-211 conformance in CI
- [ ] CRUSH-343 lexer: unknown operator characters become identifiers
- [ ] Runtime errors carry no source position (pc → source map; CRUSH-190)
- [ ] CRUSH-34 spawn/await/yield: commits 2–3

## Walkers

- [ ] C: #329, #330, #331 · shell: #332 (arithmetic), #333 (document refusals)
