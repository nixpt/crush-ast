//! The builtin and method table (CRUSH-250; was CRUSH-zorro-11).
//!
//! Every JS global function, namespace function (`Math.floor`) and receiver
//! method (`a.push(x)`) the lowering knows is a row here. A call to anything
//! that is neither a user binding nor a row fails at walk time, naming what
//! isn't supported, instead of reaching the compiler or the VM as an unknown
//! function or capability. Area tickets add rows (and the `js.*` capabilities
//! behind them), not match arms: a `Planned` row turns into a lowering when
//! its ticket lands.

/// How many arguments a row accepts. A call outside the range is refused
/// rather than passed through (JS ignores extra arguments; a capability might
/// not).
#[derive(Clone, Copy, Debug)]
pub(super) enum Arity {
    Exactly(usize),
    Any,
}

impl Arity {
    pub(super) fn accepts(self, n: usize) -> bool {
        match self {
            Arity::Exactly(k) => n == k,
            Arity::Any => true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Lowering {
    /// A capability call with the JS arguments in order.
    Cap(&'static str),
    /// `console.log(…)`: the line node prints, `io.print(js.format(…))`.
    ConsoleLine,
    /// A method the compiler dispatches on the receiver variable
    /// (`recv.m(args)` → `cap_call m [recv, args…]`). The receiver must be a
    /// plain name. `value: false` refuses the call in value position (its
    /// result differs from JS's). No row uses it yet: area tickets add string
    /// and array method rows with it once their capabilities land.
    #[allow(dead_code)]
    Receiver { cap: &'static str, value: bool },
    /// `recv.push(…)` (receiver a plain name). The `push` capability appends
    /// one value and returns the array, so:
    /// - in statement position, `xs.push(a, b, …)` evaluates every argument
    ///   first (into hidden locals, as JS does) and then pushes them one by one
    ///   (`Lower::push_statements`);
    /// - as a value, `xs.push(a)` is `len(xs.push(a))`, JS's new length, and
    ///   `xs.push()` is `len(xs)`. Several values as a value are refused.
    Push,
    /// Not supported yet: the ticket that adds it.
    Planned(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Row {
    pub name: &'static str,
    pub arity: Arity,
    pub lower: Lowering,
}

const fn row(name: &'static str, arity: Arity, lower: Lowering) -> Row {
    Row { name, arity, lower }
}

const fn planned(name: &'static str, ticket: &'static str) -> Row {
    Row {
        name,
        arity: Arity::Any,
        lower: Lowering::Planned(ticket),
    }
}

use Arity::{Any, Exactly};
use Lowering::{Cap, ConsoleLine};

/// Global and namespace functions, by their JS name.
pub(super) static GLOBALS: &[Row] = &[
    // ── console ──
    row("console.log", Any, ConsoleLine),
    row("console.info", Any, ConsoleLine),
    // Node writes these to stderr; the crush VM has no stderr print.
    planned("console.error", "CRUSH-zorro-23 (stderr output)"),
    planned("console.warn", "CRUSH-zorro-23 (stderr output)"),
    // ── Math (CRUSH-65: the lowercase `math.*` capabilities) ──
    row("Math.abs", Exactly(1), Cap("math.abs")),
    row("Math.floor", Exactly(1), Cap("math.floor")),
    row("Math.ceil", Exactly(1), Cap("math.ceil")),
    row("Math.round", Exactly(1), Cap("math.round")),
    row("Math.sqrt", Exactly(1), Cap("math.sqrt")),
    row("Math.sin", Exactly(1), Cap("math.sin")),
    row("Math.cos", Exactly(1), Cap("math.cos")),
    row("Math.tan", Exactly(1), Cap("math.tan")),
    row("Math.pow", Exactly(2), Cap("math.pow")),
    row("Math.min", Exactly(2), Cap("math.min")),
    row("Math.max", Exactly(2), Cap("math.max")),
    row("Math.random", Exactly(0), Cap("math.random")),
    planned("Math.trunc", "CRUSH-zorro-17 (Math)"),
    planned("Math.sign", "CRUSH-zorro-17 (Math)"),
    planned("Math.log", "CRUSH-zorro-17 (Math)"),
    planned("Math.exp", "CRUSH-zorro-17 (Math)"),
    planned("Math.hypot", "CRUSH-zorro-17 (Math)"),
    // ── conversions, JSON, objects, arrays ──
    planned("parseInt", "CRUSH-zorro-17 (number conversions)"),
    planned("parseFloat", "CRUSH-zorro-17 (number conversions)"),
    planned("Number", "CRUSH-zorro-17 (number conversions)"),
    planned("String", "CRUSH-zorro-17 (string conversions)"),
    planned("Boolean", "CRUSH-zorro-17 (conversions)"),
    planned("isNaN", "CRUSH-zorro-12 (NaN)"),
    planned("isFinite", "CRUSH-zorro-12 (Infinity/NaN)"),
    planned("Number.isInteger", "CRUSH-zorro-17 (Number)"),
    planned("JSON.parse", "CRUSH-zorro-17 (JSON)"),
    planned("JSON.stringify", "CRUSH-zorro-17 (JSON)"),
    planned("Object.keys", "CRUSH-zorro-16 (Object.*)"),
    planned("Object.values", "CRUSH-zorro-16 (Object.*)"),
    planned("Object.entries", "CRUSH-zorro-16 (Object.*)"),
    planned("Object.assign", "CRUSH-zorro-16 (Object.*)"),
    planned("Array.isArray", "CRUSH-zorro-15 (arrays)"),
    planned("Array.from", "CRUSH-zorro-15 (arrays)"),
    planned("Array", "CRUSH-zorro-15 (arrays)"),
    planned("Object", "CRUSH-zorro-16 (objects)"),
    planned("Symbol", "out of scope: symbols"),
    // ── host ──
    row("fetch", Exactly(1), Cap("net.http_get")),
    row("xip.invoke", Any, Cap("xip.invoke")),
];

/// Receiver methods, by name. One row covers every receiver type that has
/// the method (JS dispatches at run time; so does the capability).
pub(super) static METHODS: &[Row] = &[
    row("push", Any, Lowering::Push),
    // ── strings (CRUSH-zorro-14) ──
    planned("toUpperCase", "CRUSH-zorro-14 (string methods)"),
    planned("toLowerCase", "CRUSH-zorro-14 (string methods)"),
    planned("trim", "CRUSH-zorro-14 (string methods)"),
    planned("trimStart", "CRUSH-zorro-14 (string methods)"),
    planned("trimEnd", "CRUSH-zorro-14 (string methods)"),
    planned("split", "CRUSH-zorro-14 (string methods)"),
    planned("startsWith", "CRUSH-zorro-14 (string methods)"),
    planned("endsWith", "CRUSH-zorro-14 (string methods)"),
    planned("replace", "CRUSH-zorro-14 (string methods)"),
    planned("replaceAll", "CRUSH-zorro-14 (string methods)"),
    planned("repeat", "CRUSH-zorro-14 (string methods)"),
    planned("padStart", "CRUSH-zorro-14 (string methods)"),
    planned("padEnd", "CRUSH-zorro-14 (string methods)"),
    planned("charAt", "CRUSH-zorro-14 (string methods)"),
    planned("charCodeAt", "CRUSH-zorro-14 (string methods)"),
    planned("substring", "CRUSH-zorro-14 (string methods)"),
    // ── arrays and strings (CRUSH-zorro-14/15) ──
    planned("indexOf", "CRUSH-zorro-14/15 (string and array methods)"),
    planned(
        "lastIndexOf",
        "CRUSH-zorro-14/15 (string and array methods)",
    ),
    planned("includes", "CRUSH-zorro-14/15 (string and array methods)"),
    planned("slice", "CRUSH-zorro-14/15 (string and array methods)"),
    planned("concat", "CRUSH-zorro-14/15 (string and array methods)"),
    planned("at", "CRUSH-zorro-14/15 (string and array methods)"),
    // ── arrays (CRUSH-zorro-15; callbacks CRUSH-zorro-20) ──
    planned("pop", "CRUSH-zorro-15 (array methods)"),
    planned("shift", "CRUSH-zorro-15 (array methods)"),
    planned("unshift", "CRUSH-zorro-15 (array methods)"),
    planned("splice", "CRUSH-zorro-15 (array methods)"),
    planned("join", "CRUSH-zorro-15 (array methods)"),
    planned("reverse", "CRUSH-zorro-15 (array methods)"),
    planned("fill", "CRUSH-zorro-15 (array methods)"),
    planned("flat", "CRUSH-zorro-15 (array methods)"),
    planned("sort", "CRUSH-zorro-15/20 (sort, sort(cmp))"),
    planned("map", "CRUSH-zorro-20 (callbacks)"),
    planned("filter", "CRUSH-zorro-20 (callbacks)"),
    planned("reduce", "CRUSH-zorro-20 (callbacks)"),
    planned("forEach", "CRUSH-zorro-20 (callbacks)"),
    planned("find", "CRUSH-zorro-20 (callbacks)"),
    planned("findIndex", "CRUSH-zorro-20 (callbacks)"),
    planned("some", "CRUSH-zorro-20 (callbacks)"),
    planned("every", "CRUSH-zorro-20 (callbacks)"),
    // ── numbers and values (CRUSH-zorro-17) ──
    planned("toFixed", "CRUSH-zorro-17 (number formatting)"),
    planned("toString", "CRUSH-zorro-17 (conversions)"),
    // ── Map / Set (CRUSH-zorro-17) ──
    planned("get", "CRUSH-zorro-17 (Map/Set)"),
    planned("set", "CRUSH-zorro-17 (Map/Set)"),
    planned("has", "CRUSH-zorro-17 (Map/Set)"),
    planned("add", "CRUSH-zorro-17 (Map/Set)"),
    planned("delete", "CRUSH-zorro-17 (Map/Set)"),
    planned("keys", "CRUSH-zorro-16/17 (keys)"),
    planned("values", "CRUSH-zorro-16/17 (values)"),
    planned("entries", "CRUSH-zorro-16/17 (entries)"),
    // ── promises ──
    planned("then", "out of scope: async/Promise"),
    planned("catch", "out of scope: async/Promise"),
];

/// Global values read as names (`undefined`, `Math.PI`). None is supported
/// yet; a read of any other name that no scope binds is "not defined".
pub(super) static VALUES: &[Row] = &[
    planned("undefined", "CRUSH-zorro-12 (`undefined` as a value)"),
    planned("NaN", "CRUSH-zorro-12 (NaN)"),
    planned("Infinity", "CRUSH-zorro-12 (Infinity)"),
    planned("Math.PI", "CRUSH-zorro-17 (Math constants)"),
    planned("Math.E", "CRUSH-zorro-17 (Math constants)"),
    planned(
        "Number.MAX_SAFE_INTEGER",
        "CRUSH-zorro-13/17 (number model)",
    ),
    planned(
        "Number.MIN_SAFE_INTEGER",
        "CRUSH-zorro-13/17 (number model)",
    ),
    planned("globalThis", "out of scope: the global object"),
    planned("arguments", "CRUSH-zorro-18 (`arguments`)"),
];

pub(super) fn value(name: &str) -> Option<&'static Row> {
    VALUES.iter().find(|r| r.name == name)
}

pub(super) fn global(name: &str) -> Option<&'static Row> {
    GLOBALS.iter().find(|r| r.name == name)
}

pub(super) fn method(name: &str) -> Option<&'static Row> {
    METHODS.iter().find(|r| r.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique() {
        for table in [GLOBALS, METHODS, VALUES] {
            let mut names: Vec<&str> = table.iter().map(|r| r.name).collect();
            names.sort_unstable();
            let before = names.len();
            names.dedup();
            assert_eq!(before, names.len(), "duplicate row");
        }
    }
}
