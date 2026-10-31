//! Source-level string slices `s[a:b]` (and steps) must reach `seq.slice` —
//! char-based, arrays-or-strings — once the parser desugar stopped targeting
//! the array-only `arr_slice` (which rejected strings on every engine with
//! "expected array, got str").
//!
//! Slicing is by Unicode scalar (`s.chars()`), like `s[i]` — negative bounds
//! count from the end via the shared `slice_indices` (CPython semantics).

use crush_vm::{Quotas, run};

#[test]
fn string_slice_bounds_count_by_character() {
    let src = "let s = \"héllo wörld\"\n\
               print(s[0:5])\n\
               print(s[-5:])\n\
               print(s[7:])\n\
               print(s[:6])\n\
               print(s[9:2])\n";
    let casm = crush_frontend::compile_crush_source(src).expect("compile");
    let program = crush_lang_sdk::compile::casm_to_vm(&casm).expect("lower");
    let out = run(&program, &Quotas::default()).expect("run").output;
    assert_eq!(
        out,
        "héllo\nwörld\nörld\nhéllo \n\n",
        "slices must cut at char boundaries (é/ö are 2 bytes in UTF-8)"
    );
}

#[test]
fn multibyte_scalar_cannot_be_split_by_slicing() {
    // 4-byte emoji (🎉) and 3-byte kanji (日) must survive every
    // slice machinery, including single-char slices and reversal.
    let src = "let u = \"日本語🎉!\"\n\
               print(u[3:4])\n\
               print(u[::-1])\n\
               print(u[::2])\n\
               print(u[1::2])\n";
    let casm = crush_frontend::compile_crush_source(src).expect("compile");
    let program = crush_lang_sdk::compile::casm_to_vm(&casm).expect("lower");
    let out = run(&program, &Quotas::default()).expect("run").output;
    assert_eq!(
        out,
        "🎉\n!🎉語本日\n日語!\n本🎉\n"
    );
}

#[test]
fn slice_step_forms_reach_the_step_slot() {
    // `::` lexes as one DoubleColon token; parsing used to route it to the
    // wrong slot (or reject it outright). Every form here must behave as
    // Python's slice.indices does.
    let src = "let xs = [10, 20, 30, 40, 50, 60]\n\
               print(xs[::2])\n\
               print(xs[::-1])\n\
               print(xs[1::2])\n\
               print(xs[:4:2])\n\
               print(xs[1:4])\n\
               print(xs[:])\n";
    let casm = crush_frontend::compile_crush_source(src).expect("compile");
    let program = crush_lang_sdk::compile::casm_to_vm(&casm).expect("lower");
    let out = run(&program, &Quotas::default()).expect("run").output;
    assert_eq!(
        out,
        "[10, 30, 50]\n[60, 50, 40, 30, 20, 10]\n[20, 40, 60]\n[10, 30]\n[20, 30, 40]\n[10, 20, 30, 40, 50, 60]\n"
    );
}
