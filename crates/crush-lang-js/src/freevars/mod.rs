//! Free-variable analysis for `@javascript { ... }` polyglot blocks (CRUSH-68).
//!
//! Mirrors `crush_lang_python::analyzer::free_variables`: names the block
//! *reads* but never binds are inputs; top-level plain assignments are
//! candidate outputs (last one wins).
//!
//! Two walks over the same shallow, scope-unaware rules: `boa` (used whenever
//! the `js-boa` feature is on, which crush-lang-sdk's `polyglot-javascript`
//! turns on, so the SDK carries no swc) and `swc`. `twin_tests` keeps them in
//! step until CRUSH-250's scope pass replaces both.

#[cfg(feature = "js-boa")]
mod boa;
#[cfg(feature = "swc-backend")]
#[cfg_attr(feature = "js-boa", allow(dead_code))]
mod swc;

/// Result of free-variable analysis on a polyglot JS block.
pub struct FreeVars {
    /// Names read but never bound in this block (first-occurrence order).
    pub reads: Vec<String>,
    /// Names assigned at the block's own top level (occurrence order).
    /// Marshaling protocol picks the last entry as the single output.
    pub top_level_bound: Vec<String>,
}

/// Analyze a JS source block with a real parser — not a regex.
pub fn free_variables(source: &str) -> Result<FreeVars, String> {
    #[cfg(feature = "js-boa")]
    return boa::free_variables(source);
    #[cfg(all(feature = "swc-backend", not(feature = "js-boa")))]
    return swc::free_variables(source);
    #[cfg(not(any(feature = "js-boa", feature = "swc-backend")))]
    {
        let _ = source;
        Err("crush-lang-js was built without a JS parser: enable `js-boa`".to_string())
    }
}

#[cfg(all(test, any(feature = "js-boa", feature = "swc-backend")))]
mod tests {
    use super::*;

    #[test]
    fn free_read_and_top_level_assign() {
        let fv = free_variables("result = base * 2;\n").unwrap();
        assert_eq!(fv.reads, vec!["base".to_string()]);
        assert_eq!(fv.top_level_bound, vec!["result".to_string()]);
    }

    #[test]
    fn let_binding_is_output_not_free_read() {
        let fv = free_variables("let result = base + 1;\n").unwrap();
        assert_eq!(fv.reads, vec!["base".to_string()]);
        assert_eq!(fv.top_level_bound, vec!["result".to_string()]);
    }

    #[test]
    fn local_binding_not_free() {
        let fv = free_variables("let x = 1;\nlet result = x + 1;\n").unwrap();
        assert!(fv.reads.is_empty());
        assert_eq!(
            fv.top_level_bound,
            vec!["x".to_string(), "result".to_string()]
        );
    }
}

/// Both walks must agree, so switching the SDK to boa changed nothing.
#[cfg(all(test, feature = "js-boa", feature = "swc-backend"))]
mod twin_tests {
    const BLOCKS: &[&str] = &[
        "result = base * 2;\n",
        "let result = base + 1;\n",
        "let x = 1;\nlet result = x + 1;\n",
        "const a = [p, ...q, r[i]];\nvar o = {k: v, w, [c]: d};\nout = `${t} and ${u}`;\n",
        "function f(n) { return n + g; }\nlet y = f(h) ? m.n : o[p];\n",
        "if (a > b) { c = d; } else { e = -f; }\nwhile (i < n) { i = i + step; }\n",
        "for (let i = 0; i < len; i = i + 1) { total = total + xs[i]; }\n",
        "for (const k of ks) { seen = k; }\nfor (var j in obj) { last = j; }\n",
        "try { risky(z); } catch (err) { msg = err; } finally { done = true; }\n",
        "obj.prop = val;\narr[idx] = item;\ncount += inc;\n",
        "import fs from 'fs';\nimport * as path from 'path';\nimport { a as b, c } from 'm';\nr = fs(b, c, path, ext);\n",
        "class K {}\nlet inst = K;\ndo { q = w; } while (cond);\n",
        "const f2 = (a) => a + free1;\nlet s = (x1, y1);\nz1 = !flag && other || typeof thing;\n",
        "x2++;\nnew Ctor(arg);\nopt?.call(v2);\nswitch (sel) { case 1: hit = sel; }\n",
    ];

    #[test]
    fn boa_and_swc_agree() {
        for src in BLOCKS {
            let b = super::boa::free_variables(src).unwrap();
            let s = super::swc::free_variables(src).unwrap();
            assert_eq!(b.reads, s.reads, "reads differ for {src:?}");
            assert_eq!(b.top_level_bound, s.top_level_bound, "outputs differ for {src:?}");
        }
    }
}
