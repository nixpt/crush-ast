# crush-cson — retired

Renamed to [`crush-caison`](https://crates.io/crates/crush-caison) (CAISON,
formerly CSON). This crate is a final release that re-exports it; use
`crush-caison` directly. The VM capability is now `caison.parse`
(`cson.parse` is a deprecated alias, removed in 0.5).

The source lives in this repo at `crates/crush-cson-shim/`; the real crate is
`crates/crush-caison/`.
