# Private Rust AOT support — CRUSH-xps-13

The support library is generated from the same std-only method kernel used by
VM adapters and Rust AOT. It is a private static library built by rustc and
linked into C artifacts; workspace library crate types and the public plugin ABI
stay unchanged. The bridge supports scalar/string method inputs, not
sequence Debug descriptors (the xps-9 C refusal remains).

## Reviewed boundary

- Input records carry a validated tag, integer/float payload, and UTF-8 pointer
  plus byte length. The generated C caller owns valid live buffers for the call;
  arbitrary foreign pointers are outside this private ABI's contract. Null with
  nonzero length, oversized lengths, invalid UTF-8 and unknown tags are errors.
- Results carry a status, scalar payload or owned byte buffer plus length.
  Every owned buffer, including error text, is released by the matching Rust
  free function. Rust catches panics before returning across the C boundary.
- Calls do not call back into Crush or collect. C copies successful text into a
  managed string before releasing Rust storage, and pushes the result before
  the next instruction/collection. No pointer into another string is returned.
- The existing C Value layout stays unchanged. Programs using this bridge
  reject embedded NUL constants and unproven external/character input producers
  before artifact generation. Canonical string calls additionally require
  walker-proven string inputs or immediate string literals; generic C coercions
  are refused. Arbitrarily large representable strings use
  dynamic allocation rather than fixed buffers or locale-based conversion.
- SDK string predicates retain integer flags where that is their contract;
  core contains retains Bool. Rust prefix/suffix adapters return Bool explicitly.
  Numeric adapters retain integer widths, distinguish powi from powf, and use
  the conformance lane's optimized rustc wrapping profile. Bounded u64/usize
  results need source proof before they can enter the guest i64 representation.
- Artifact and support-library cache keys include generated source/kernel,
  compiler identity, host/CC target and optimization options. Linux native
  linkage is the supported bridge target; other targets fail explicitly. The
  static support archive symbols are hidden from the generated module exports.

This review keeps pure language kernels separate from OS/host authority,
introduces no peer dependency or dependency back-edge, and leaves stdlib
registration and host capability checks in the existing adapters.

## Existing host result transport

`Module::call_main` uses the existing exported C-string entrypoint. Rust AOT
now quotes String results consistently with C AOT, preserving numeric/Boolean
text and whitespace; the loader recognizes non-finite Float results. AOT method
programs with potentially NUL-containing root text results are compile-refused
until a length-aware host result ABI exists. The private support ABI already
preserves NUL bytes, but neither it nor this change expands the public result
ABI or the C Value layout. Runtime export failures also remain loud.

After rebasing onto #268, compiler banners/targets are cached per process,
cache-hit mtimes are refreshed for persistent CI pruning, and environment
cache/optimization overrides plus deterministic generation are retained.
