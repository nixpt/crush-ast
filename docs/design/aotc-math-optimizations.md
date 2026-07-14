# AOTC Complex Math Optimization Pathways

To enable the `crush-aotc` compiler to achieve near-C performance for complex math (e.g., Python-level computations, matrix operations, or high-precision floats), we need to transition away from dynamic `Value` boxing. 

Here are three distinct architectural pathways we can take to upgrade the Crush toolchain. We have chosen to pursue **Pathway 1** initially.

---

## Pathway 1: The Gradual Typing & Hinting Approach (Active)
**Concept:** Introduce optional Type Annotations to the AST, allowing external languages (like Python) to explicitly opt-in to raw C-speed variables.

1. **AST Upgrade**: We add a `Type` enum to `crush-cast` (e.g., `Type::F64`, `Type::Int64`, `Type::BigInt`, `Type::Dynamic`).
2. **Compiler Changes (`crush-aotc`)**:
   - When generating C code, if a variable is annotated as `Type::F64`, we emit a raw `double` in C.
   - Math operations between `double` types are emitted as raw `a + b` C operations, executing at native hardware speed.
   - If an un-annotated variable is encountered, we fall back to the slow, boxed `Value` struct.
3. **Python Integration**: This mimics **Numba**. Python developers could decorate functions like `@crush.jit(types=[f64, f64])`, giving the Crush AST the hints it needs to drop the boxing overhead.

---

## Pathway 2: The Tracing & Type-Inference Approach
**Concept:** Do not change the AST at all. Instead, make the `crush-aotc` compiler extremely smart so it can deduce mathematical types automatically.

1. **Data-Flow Analysis**: The AOTC compiler performs a multi-pass analysis over the AST before generating C code.
2. **Type Promotion**: If the compiler sees `let x = 5.0; let y = x + 2.0;`, it definitively knows `x` and `y` are `f64`. It automatically lowers them to C `double` primitives without needing explicit hints.
3. **De-optimization Guards**: If a branch allows a variable to switch from a `Float` to an `Array`, the compiler inserts a "guard" that boxes the primitive back into a `Value` enum.

---

## Pathway 3: Hardware Intrinsics & Vector Extensions
**Concept:** Rather than just speeding up scalar floats, we introduce specialized AST nodes designed exclusively for array/matrix and complex math acceleration.

1. **AST Upgrade**: Add explicit `VectorMath` expressions to `crush-cast` (e.g., `VecAdd`, `VecDotProduct`, `MatrixMul`).
2. **Interpreter Fallback**: The standard interpreted `FastVM` executes these nodes using standard Rust loops.
3. **AOTC Acceleration**: When `crush-aotc` encounters these nodes, it skips standard loops entirely and generates C code that utilizes **Hardware SIMD intrinsics** (like AVX2 `_mm256_add_pd`) or links directly against native libraries like **BLAS/LAPACK**.
