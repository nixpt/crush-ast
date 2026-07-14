//! # crush-aotc — CASM → C source AOT compiler
//!
//! Translates a `casm::Program` into a self-contained C translation unit that
//! can be compiled with any `cc`-compatible compiler into a shared library or
//! standalone binary.
//!
//! ## Architecture
//!
//! ```text
//! .crush source
//!     │  crush-frontend
//!     ▼
//! casm::Program  ←  also accepted directly (polyglot walkers produce CAST→CASM)
//!     │  crush-aotc (this crate)
//!     ▼
//! C source text
//!     │  cc / clang / gcc
//!     ▼
//! .so / .a / binary
//! ```
//!
//! ## Design choices
//!
//! * **Pathway 1 (fast scalar math)**: typed `double`/`int64_t` scalars when the
//!   compiler can statically prove the type. Falls back to boxed `CrushValue`
//!   when the type is dynamic.
//! * **Pathway 3 (SIMD / AVX2)**: optional; when `AotcOpts::simd` is set, array
//!   dot-product and element-wise loops emit `__m256d` intrinsics.
//! * The generated C file `#include`s a small inline runtime header
//!   (`crush_rt.h`) that defines `CrushValue`, the capability dispatch table,
//!   and a handful of helper macros.  No external library dependency at link
//!   time beyond libc.
//!
//! ## CPU kernel entry point
//!
//! A function annotated `@kernel` in Crush is emitted as:
//!
//! ```c
//! void __crush_kernel_<name>(
//!     const double * __restrict__ in,
//!     double       * __restrict__ out,
//!     size_t n
//! );
//! ```
//!
//! This is the "CPU kernel" analogue to the PTX kernel in `crush-ptx`.

pub mod codegen;
pub mod infer;
pub mod kernel;
pub mod rt_header;

pub use codegen::{AotcCompiler, AotcOpts};
pub use kernel::CpuKernelEmitter;
