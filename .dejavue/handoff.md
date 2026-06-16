# Handoff

Read `.dejavue/state.md`, `.dejavue/decisions.md`, and `.dejavue/timeline.jsonl` before making changes.

## Next Steps

1. Publish core crates (crush-errors, crush-cast, casm) to crates.io so external dependents can use versioned deps instead of path deps.
2. Add `examples/cast/` fixtures to fix crush-cast test.
3. Fix walker-core doc-test crate dependency references.
4. Align walker crate versions (0.1.0) with workspace policy — decide whether to bump to 0.2.0 or keep separate version track.
