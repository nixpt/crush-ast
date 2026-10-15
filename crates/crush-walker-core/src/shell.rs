//! Shared lowering for the POSIX-family shell walkers (bash, zsh) → CAST.
//!
//! CRUSH-202: the bash and zsh walkers each had their own lowerer, and both
//! produced programs that ran, exited 0 and printed something the real shell
//! never would (`[ $x -gt 3 ]` always true, `$((…))` printed literally,
//! `echo a b` → `ab`, `until` lowered as `while`, …). Both now convert their
//! parser's AST into the small shell IR below and call [`lower_script`], so a
//! construct means the same thing for both shells, and the few places where
//! the shells really differ are explicit [`Dialect`] checks.
//!
//! The rule is: every construct is either lowered so the program prints what
//! the shell prints, or rejected with `unsupported: <construct> at line N`.
//! Some checks need run-time values (a word that splits into several fields
//! where one is required, a non-integer in arithmetic); those throw an
//! `unsupported: …` error from the program instead of continuing.
//!
//! Model used by the generated program:
//! - Shell values are strings. Global variables live in one map, `__g`,
//!   created by `main` and passed to every shell function (maps are shared
//!   references in the VM, so a function's assignment is seen by its caller).
//!   `local` variables are CAST locals (`__l_<name>`).
//! - `__st` holds the exit status of the last command. `&&`, `||`, `!`, `if`,
//!   `while` and `until` test it; shell functions return it; `main` ends with
//!   `sys.exit(__st)` when it is non-zero.
//! - Positional parameters are the array `__a` (empty in `main`).

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{Result, anyhow};
use crush_cast::{CastType, Expression as E, Function, Program, Statement as S};

/// Which shell's rules apply where bash and zsh differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    Bash,
    Zsh,
}

impl Dialect {
    pub fn name(self) -> &'static str {
        match self {
            Dialect::Bash => "bash",
            Dialect::Zsh => "zsh",
        }
    }
}

/// One shell word, as written in the source (quotes and `$` included).
#[derive(Clone, Debug)]
pub struct Word {
    pub raw: String,
    pub line: usize,
}

/// `name=value` / `name+=value`.
#[derive(Clone, Debug)]
pub struct Assign {
    pub name: String,
    pub value: Word,
    pub append: bool,
    pub line: usize,
}

#[derive(Clone, Debug)]
pub enum Cmd {
    Simple { assigns: Vec<Assign>, words: Vec<Word>, line: usize },
    If { branches: Vec<(List, List)>, else_body: Option<List>, line: usize },
    Loop { cond: List, body: List, until: bool, line: usize },
    /// `for var in words`; `words: None` iterates `"$@"`.
    For { var: String, words: Option<Vec<Word>>, body: List, line: usize },
    ArithFor { init: Option<String>, cond: Option<String>, step: Option<String>, body: List, line: usize },
    Case { word: Word, arms: Vec<(Vec<Word>, List)>, line: usize },
    Group(List),
    Arith { expr: String, line: usize },
    FuncDef { name: String, body: List, line: usize },
    /// A construct the parser adapter already knows it cannot lower.
    Unsupported { what: String, line: usize },
}

#[derive(Clone, Debug)]
pub struct Pipeline {
    pub negated: bool,
    pub cmd: Cmd,
}

/// `first && p1 || p2 …`; `true` = `&&`.
#[derive(Clone, Debug)]
pub struct AndOr {
    pub first: Pipeline,
    pub rest: Vec<(bool, Pipeline)>,
}

pub type List = Vec<AndOr>;

/// Lower a whole script.
pub fn lower_script(dialect: Dialect, script: &List) -> Result<Program> {
    let mut lw = Lw::new(dialect);
    for item in script {
        if let Cmd::FuncDef { name, .. } = &item.first.cmd {
            lw.funcs.insert(name.clone());
        }
    }
    let mut body = Vec::new();
    for item in script {
        match &item.first.cmd {
            Cmd::FuncDef { name, body: fbody, line } if item.rest.is_empty() && !item.first.negated => {
                lw.lower_function(name, fbody, *line)?;
                lw.defined.insert(name.clone());
            }
            _ => lw.lower_and_or(item, &mut body)?,
        }
    }

    for (name, line) in &lw.global_reads {
        if !lw.globals.contains(name) {
            return Err(lw.err(
                &format!("`${name}` is never assigned in this script (environment variables are not supported)"),
                *line,
            ));
        }
    }
    for (name, line) in &lw.fn_global_refs {
        if lw.local_names.contains(name) {
            return Err(lw.err(
                &format!(
                    "`{name}` is `local` in one function and used without `local` in another \
                     (dynamic scoping of locals)"
                ),
                *line,
            ));
        }
    }

    let mut main = vec![decl("__g", E::ObjectLiteral { properties: vec![], meta: m() })];
    for g in &lw.globals {
        main.push(set_index("__g", g, s("")));
    }
    main.push(decl("__a", E::ArrayLiteral { elements: vec![], meta: m() }));
    main.push(decl("__st", int(0)));
    main.extend(body);
    main.push(if_(
        bin("!=", var("__st"), int(0)),
        vec![expr_stmt(cap("sys.exit", vec![var("__st")]))],
        None,
    ));

    let mut functions = std::mem::take(&mut lw.functions);
    functions.insert("main".to_string(), func(&[], main));
    for helper in &lw.helpers {
        functions.insert(helper.to_string(), helper_fn(helper));
    }
    Ok(Program {
        cast_version: "0.2".to_string(),
        entry: "main".to_string(),
        lang: Some(dialect.name().to_string()),
        functions,
        ai_meta: None,
        ..Default::default()
    })
}

// ── CAST builders ────────────────────────────────────────────────────────────

fn m() -> HashMap<String, serde_json::Value> {
    HashMap::new()
}
fn s(v: &str) -> E {
    E::StringLiteral { value: v.to_string(), meta: m() }
}
fn int(v: i64) -> E {
    E::IntLiteral { value: v, meta: m() }
}
fn boolean(v: bool) -> E {
    E::BoolLiteral { value: v, meta: m() }
}
fn var(n: &str) -> E {
    E::Var { name: n.to_string(), meta: m() }
}
fn bin(op: &str, l: E, r: E) -> E {
    E::BinaryOp { operator: op.to_string(), left: Box::new(l), right: Box::new(r), meta: m() }
}
fn not(e: E) -> E {
    E::UnaryOp { operator: "!".to_string(), operand: Box::new(e), meta: m() }
}
fn call(f: &str, args: Vec<E>) -> E {
    E::Call { function: f.to_string(), args, meta: m() }
}
fn cap(name: &str, args: Vec<E>) -> E {
    E::CapabilityCall { name: name.to_string(), args, meta: m() }
}
fn index(t: E, i: E) -> E {
    E::Index { target: Box::new(t), index: Box::new(i), meta: m() }
}
fn array(elements: Vec<E>) -> E {
    E::ArrayLiteral { elements, meta: m() }
}
fn tern(c: E, a: E, b: E) -> E {
    call("__crush_ternary__", vec![c, a, b])
}
/// `"" + e`: an integer as a shell string.
fn to_str(e: E) -> E {
    bin("+", s(""), e)
}
fn len(e: E) -> E {
    call("len", vec![e])
}
fn chars(e: E) -> E {
    call("str.split", vec![e, s("")])
}
fn decl(n: &str, v: E) -> S {
    S::VarDecl { name: n.to_string(), value: v, type_hint: CastType::Any, meta: m() }
}
fn set(n: &str, v: E) -> S {
    S::Assign { target: n.to_string(), value: v, meta: m() }
}
fn set_index(map: &str, key: &str, v: E) -> S {
    expr_stmt(call("__crush_setindex__", vec![var(map), s(key), v]))
}
fn expr_stmt(e: E) -> S {
    S::ExprStmt { expr: e, meta: m() }
}
fn if_(c: E, then_body: Vec<S>, else_body: Option<Vec<S>>) -> S {
    S::If { condition: c, then_body, else_body, meta: m() }
}
fn while_(c: E, body: Vec<S>) -> S {
    S::While { condition: Box::new(c), body, meta: m() }
}
fn for_(v: &str, iter: E, body: Vec<S>) -> S {
    S::For { variable: v.to_string(), iterable: Box::new(iter), body, meta: m() }
}
fn ret(e: E) -> S {
    S::Return { value: Some(e), meta: m() }
}
fn throw(e: E) -> S {
    S::Throw { value: e, meta: m() }
}
fn set_st(v: i64) -> S {
    set("__st", int(v))
}
fn st_ok() -> E {
    bin("==", var("__st"), int(0))
}
fn func(params: &[&str], body: Vec<S>) -> Function {
    Function {
        params: params.iter().map(|p| (p.to_string(), CastType::Any)).collect(),
        body,
        meta: m(),
        ..Default::default()
    }
}
fn concat(parts: Vec<E>) -> E {
    let mut it = parts.into_iter();
    match it.next() {
        None => s(""),
        Some(first) => it.fold(first, |acc, p| bin("+", acc, p)),
    }
}
fn is_ident(n: &str) -> bool {
    let mut cs = n.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
fn fn_name(n: &str) -> String {
    format!("__shfn_{n}")
}
fn local_name(n: &str) -> String {
    format!("__l_{n}")
}

// ── Words ────────────────────────────────────────────────────────────────────

/// One piece of a word after quote removal. `bool` = quoted.
#[derive(Clone, Debug)]
enum Seg {
    Lit(String, bool),
    Var(String, bool),
    Pos(usize, bool),
    /// `$#`
    Count(bool),
    /// `$?`
    Status(bool),
    /// `$@` (`star: false`) or `$*`.
    All { star: bool, quoted: bool },
    /// `${#name}`
    Len(String, bool),
    /// `$(( … ))`
    Arith(String, bool),
}

impl Seg {
    fn quoted(&self) -> bool {
        match self {
            Seg::Lit(_, q) | Seg::Var(_, q) | Seg::Pos(_, q) | Seg::Count(q) | Seg::Status(q) | Seg::Len(_, q) | Seg::Arith(_, q) => *q,
            Seg::All { quoted, .. } => *quoted,
        }
    }
    fn is_expansion(&self) -> bool {
        !matches!(self, Seg::Lit(..))
    }
    /// Expands to digits only: never empty, never split, never a glob.
    fn is_numeric(&self) -> bool {
        matches!(self, Seg::Count(_) | Seg::Status(_) | Seg::Len(..) | Seg::Arith(..))
    }
}

/// What a word expands to in an argument list.
enum Fields {
    /// Always exactly one field.
    One(E),
    /// An array of fields whose length is known only at run time.
    Many(E),
}

// ── Arithmetic ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(i64),
    Name(String),
    Pos(usize),
    Op(&'static str),
}

#[derive(Clone, Debug)]
enum Ref {
    Name(String),
    Pos(usize),
}

#[derive(Clone, Debug)]
enum A {
    Num(i64),
    Ref(Ref),
    Un(&'static str, Box<A>),
    Bin(&'static str, Box<A>, Box<A>),
    Tern(Box<A>, Box<A>, Box<A>),
    Assign(String, &'static str, Box<A>),
    Inc { name: String, delta: i64, post: bool },
}

const ARITH_OPS: &[&str] = &[
    "<<=", ">>=", "**", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<=", ">=", "==", "!=", "&&",
    "||", "<<", ">>", "+", "-", "*", "/", "%", "<", ">", "=", "!", "~", "?", ":", "&", "|", "^", ",", "(", ")",
];
const ARITH_UNSUPPORTED: &[&str] = &["<<=", ">>=", "**", "&=", "|=", "^=", "<<", ">>", "~", "&", "|", "^", ","];

struct ArithParser<'a> {
    toks: &'a [Tok],
    pos: usize,
}

impl ArithParser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn peek_op(&self) -> Option<&'static str> {
        match self.peek() {
            Some(Tok::Op(o)) => Some(o),
            _ => None,
        }
    }
    fn eat(&mut self, op: &str) -> bool {
        if self.peek_op() == Some(op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn assign(&mut self) -> Option<A> {
        if let (Some(Tok::Name(n)), Some(Tok::Op(op))) = (self.toks.get(self.pos), self.toks.get(self.pos + 1)) {
            if matches!(*op, "=" | "+=" | "-=" | "*=" | "/=" | "%=") {
                let (n, op) = (n.clone(), *op);
                self.pos += 2;
                let rhs = self.assign()?;
                return Some(A::Assign(n, op, Box::new(rhs)));
            }
        }
        self.ternary()
    }
    fn ternary(&mut self) -> Option<A> {
        let c = self.binary(0)?;
        if self.eat("?") {
            let a = self.assign()?;
            if !self.eat(":") {
                return None;
            }
            let b = self.ternary()?;
            return Some(A::Tern(Box::new(c), Box::new(a), Box::new(b)));
        }
        Some(c)
    }
    fn binary(&mut self, level: usize) -> Option<A> {
        const LEVELS: &[&[&str]] = &[&["||"], &["&&"], &["==", "!="], &["<", "<=", ">", ">="], &["+", "-"], &["*", "/", "%"]];
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut l = self.binary(level + 1)?;
        while let Some(op) = self.peek_op().filter(|o| LEVELS[level].contains(o)) {
            self.pos += 1;
            let r = self.binary(level + 1)?;
            l = A::Bin(op, Box::new(l), Box::new(r));
        }
        Some(l)
    }
    fn unary(&mut self) -> Option<A> {
        match self.peek_op() {
            Some(op @ ("!" | "-" | "+")) => {
                self.pos += 1;
                Some(A::Un(op, Box::new(self.unary()?)))
            }
            Some(op @ ("++" | "--")) => {
                self.pos += 1;
                match self.peek() {
                    Some(Tok::Name(n)) => {
                        let name = n.clone();
                        self.pos += 1;
                        Some(A::Inc { name, delta: if op == "++" { 1 } else { -1 }, post: false })
                    }
                    _ => None,
                }
            }
            _ => self.postfix(),
        }
    }
    fn postfix(&mut self) -> Option<A> {
        let p = self.primary()?;
        if let (A::Ref(Ref::Name(n)), Some(op @ ("++" | "--"))) = (&p, self.peek_op()) {
            self.pos += 1;
            return Some(A::Inc { name: n.clone(), delta: if op == "++" { 1 } else { -1 }, post: true });
        }
        Some(p)
    }
    fn primary(&mut self) -> Option<A> {
        let t = self.peek()?.clone();
        self.pos += 1;
        match t {
            Tok::Num(n) => Some(A::Num(n)),
            Tok::Name(n) => Some(A::Ref(Ref::Name(n))),
            Tok::Pos(n) => Some(A::Ref(Ref::Pos(n))),
            Tok::Op("(") => {
                let e = self.assign()?;
                if self.eat(")") { Some(e) } else { None }
            }
            Tok::Op(_) => None,
        }
    }
}

// ── test / [ ─────────────────────────────────────────────────────────────────

/// One `test` argument: its value, and its text when it is fixed at compile
/// time (operators are recognised by value, so only fixed words can be one).
struct TArg {
    lit: Option<String>,
    e: E,
}

const TEST_INT_OPS: &[(&str, &str)] = &[("-eq", "=="), ("-ne", "!="), ("-gt", ">"), ("-ge", ">="), ("-lt", "<"), ("-le", "<=")];

fn is_binary_test_op(op: &str) -> bool {
    matches!(op, "=" | "==" | "!=" | "-a" | "-o" | "<" | ">" | "=~" | "-nt" | "-ot" | "-ef")
        || TEST_INT_OPS.iter().any(|(o, _)| *o == op)
}

// ── Lowerer ──────────────────────────────────────────────────────────────────

struct Lw {
    d: Dialect,
    /// Every function defined at the top level of the script.
    funcs: HashSet<String>,
    /// Functions defined so far, in source order, while lowering `main`.
    defined: HashSet<String>,
    functions: HashMap<String, Function>,
    helpers: BTreeSet<&'static str>,
    tmp: usize,
    /// Every global variable the script assigns.
    globals: BTreeSet<String>,
    global_reads: Vec<(String, usize)>,
    /// Global variables used inside a function body.
    fn_global_refs: Vec<(String, usize)>,
    /// Names declared `local` in any function.
    local_names: HashSet<String>,
    in_fn: bool,
    locals: HashSet<String>,
    /// Compound-command nesting inside the current function body.
    depth: usize,
    /// Per enclosing loop: the variable holding the loop's exit status.
    loops: Vec<String>,
}

impl Lw {
    fn new(d: Dialect) -> Self {
        Lw {
            d,
            funcs: HashSet::new(),
            defined: HashSet::new(),
            functions: HashMap::new(),
            helpers: BTreeSet::new(),
            tmp: 0,
            globals: BTreeSet::new(),
            global_reads: Vec::new(),
            fn_global_refs: Vec::new(),
            local_names: HashSet::new(),
            in_fn: false,
            locals: HashSet::new(),
            depth: 0,
            loops: Vec::new(),
        }
    }

    fn err(&self, what: &str, line: usize) -> anyhow::Error {
        anyhow!("{} walker: unsupported: {what} at line {line}", self.d.name())
    }

    /// Location text passed to run-time checks for their error message.
    fn at(&self, what: &str, line: usize) -> E {
        s(&format!("{what} at line {line} ({} walker)", self.d.name()))
    }

    fn tmp(&mut self) -> String {
        self.tmp += 1;
        format!("__t{}", self.tmp)
    }

    fn helper(&mut self, name: &'static str, args: Vec<E>) -> E {
        self.helpers.insert(name);
        call(name, args)
    }

    fn read_var(&mut self, name: &str, line: usize) -> E {
        if self.in_fn {
            if self.locals.contains(name) {
                return var(&local_name(name));
            }
            self.fn_global_refs.push((name.to_string(), line));
        }
        self.global_reads.push((name.to_string(), line));
        index(var("__g"), s(name))
    }

    fn write_var(&mut self, name: &str, value: E, line: usize) -> Result<S> {
        if !is_ident(name) {
            return Err(self.err(&format!("assignment to `{name}`"), line));
        }
        if self.in_fn {
            if self.locals.contains(name) {
                return Ok(set(&local_name(name), value));
            }
            self.fn_global_refs.push((name.to_string(), line));
        }
        self.globals.insert(name.to_string());
        Ok(set_index("__g", name, value))
    }

    // ── structure ──

    fn lower_function(&mut self, name: &str, body: &List, line: usize) -> Result<()> {
        if self.in_fn || self.depth > 0 {
            return Err(self.err("a function defined inside a function or compound command", line));
        }
        if !is_ident(name) {
            return Err(self.err(&format!("function name `{name}`"), line));
        }
        self.in_fn = true;
        self.locals.clear();
        let mut stmts = vec![decl("__st", int(0))];
        let r = self.lower_list(body, &mut stmts);
        self.in_fn = false;
        self.locals.clear();
        r?;
        stmts.push(ret(var("__st")));
        self.functions.insert(fn_name(name), func(&["__g", "__a"], stmts));
        Ok(())
    }

    fn lower_list(&mut self, list: &List, out: &mut Vec<S>) -> Result<()> {
        for item in list {
            self.lower_and_or(item, out)?;
        }
        Ok(())
    }

    /// Lower a list nested inside a compound command.
    fn lower_nested(&mut self, list: &List) -> Result<Vec<S>> {
        let mut out = Vec::new();
        self.depth += 1;
        let r = self.lower_list(list, &mut out);
        self.depth -= 1;
        r.map(|_| out)
    }

    fn lower_and_or(&mut self, item: &AndOr, out: &mut Vec<S>) -> Result<()> {
        self.lower_pipeline(&item.first, out)?;
        for (is_and, p) in &item.rest {
            self.depth += 1;
            let mut body = Vec::new();
            let r = self.lower_pipeline(p, &mut body);
            self.depth -= 1;
            r?;
            let cond = if *is_and { st_ok() } else { not(st_ok()) };
            out.push(if_(cond, body, None));
        }
        Ok(())
    }

    fn lower_pipeline(&mut self, p: &Pipeline, out: &mut Vec<S>) -> Result<()> {
        self.lower_cmd(&p.cmd, out)?;
        if p.negated {
            out.push(if_(st_ok(), vec![set_st(1)], Some(vec![set_st(0)])));
        }
        Ok(())
    }

    fn lower_cmd(&mut self, cmd: &Cmd, out: &mut Vec<S>) -> Result<()> {
        match cmd {
            Cmd::Simple { assigns, words, line } => self.lower_simple(assigns, words, *line, out),
            Cmd::Unsupported { what, line } => Err(self.err(what, *line)),
            Cmd::FuncDef { line, .. } => Err(self.err("a function defined inside a function or compound command", *line)),
            Cmd::Group(list) => self.lower_list(list, out),
            Cmd::Arith { expr, line } => {
                let (pre, v) = self.arith(expr, *line, true)?;
                out.extend(pre);
                out.push(if_(bin("!=", v, int(0)), vec![set_st(0)], Some(vec![set_st(1)])));
                Ok(())
            }
            Cmd::If { branches, else_body, .. } => {
                let mut tail = match else_body {
                    Some(l) => self.lower_nested(l)?,
                    None => vec![set_st(0)],
                };
                for (cond, body) in branches.iter().rev() {
                    let mut blk = self.lower_nested(cond)?;
                    let then_body = self.lower_nested(body)?;
                    blk.push(if_(st_ok(), then_body, Some(tail)));
                    tail = blk;
                }
                out.extend(tail);
                Ok(())
            }
            Cmd::Loop { cond, body, until, .. } => {
                let ls = self.tmp();
                out.push(decl(&ls, int(0)));
                let mut loop_body = self.lower_nested(cond)?;
                let exit = if *until { st_ok() } else { not(st_ok()) };
                loop_body.push(if_(exit, vec![S::Break { meta: m() }], None));
                self.loops.push(ls.clone());
                let b = self.lower_nested(body);
                self.loops.pop();
                loop_body.extend(b?);
                loop_body.push(set(&ls, var("__st")));
                out.push(while_(boolean(true), loop_body));
                out.push(set("__st", var(&ls)));
                Ok(())
            }
            Cmd::For { var: name, words, body, line } => {
                if !is_ident(name) {
                    return Err(self.err(&format!("`for` variable `{name}`"), *line));
                }
                let items = match words {
                    Some(ws) => self.fields_list(ws)?,
                    None => var("__a"),
                };
                let (arr, item, ls) = (self.tmp(), self.tmp(), self.tmp());
                out.push(decl(&arr, items));
                out.push(decl(&ls, int(0)));
                let mut loop_body = vec![self.write_var(name, var(&item), *line)?];
                self.loops.push(ls.clone());
                let b = self.lower_nested(body);
                self.loops.pop();
                loop_body.extend(b?);
                loop_body.push(set(&ls, var("__st")));
                out.push(for_(&item, var(&arr), loop_body));
                out.push(set("__st", var(&ls)));
                Ok(())
            }
            Cmd::ArithFor { init, cond, step, body, line } => {
                if let Some(init) = init {
                    let (pre, _) = self.arith(init, *line, true)?;
                    out.extend(pre);
                }
                let (first, ls) = (self.tmp(), self.tmp());
                out.push(decl(&first, boolean(true)));
                out.push(decl(&ls, int(0)));
                let mut loop_body = Vec::new();
                if let Some(step) = step {
                    let (pre, _) = self.arith(step, *line, true)?;
                    loop_body.push(if_(not(var(&first)), pre, None));
                }
                loop_body.push(set(&first, boolean(false)));
                if let Some(cond) = cond {
                    let (pre, v) = self.arith(cond, *line, false)?;
                    loop_body.extend(pre);
                    loop_body.push(if_(bin("==", v, int(0)), vec![S::Break { meta: m() }], None));
                }
                self.loops.push(ls.clone());
                let b = self.lower_nested(body);
                self.loops.pop();
                loop_body.extend(b?);
                loop_body.push(set(&ls, var("__st")));
                out.push(while_(boolean(true), loop_body));
                out.push(set("__st", var(&ls)));
                Ok(())
            }
            Cmd::Case { word, arms, line } => {
                let subject = self.word_str(word)?;
                let t = self.tmp();
                out.push(decl(&t, subject));
                let mut tail = vec![set_st(0)];
                for (pats, body) in arms.iter().rev() {
                    let mut cond: Option<E> = None;
                    for p in pats {
                        let c = if p.raw == "*" {
                            boolean(true)
                        } else {
                            let segs = self.parse_word(p)?;
                            let mut pe = self.concat_segs(&segs, p.line)?;
                            if self.d == Dialect::Bash && segs.iter().any(|sg| sg.is_expansion() && !sg.quoted()) {
                                pe = self.helper("__sh_noglob", vec![pe, self.at("unquoted expansion in a `case` pattern", *line)]);
                            }
                            bin("==", var(&t), pe)
                        };
                        cond = Some(match cond {
                            None => c,
                            Some(acc) => bin("or", acc, c),
                        });
                    }
                    let then_body = self.lower_nested(body)?;
                    tail = vec![if_(cond.unwrap_or(boolean(false)), then_body, Some(tail))];
                }
                out.extend(tail);
                Ok(())
            }
        }
    }

    // ── simple commands ──

    fn lower_simple(&mut self, assigns: &[Assign], words: &[Word], line: usize, out: &mut Vec<S>) -> Result<()> {
        if words.is_empty() {
            for a in assigns {
                let mut v = self.word_str(&a.value)?;
                if a.append {
                    v = bin("+", self.read_var(&a.name, a.line), v);
                }
                out.push(self.write_var(&a.name, v, a.line)?);
            }
            out.push(set_st(0));
            return Ok(());
        }
        if !assigns.is_empty() {
            return Err(self.err("an assignment before a command (`NAME=value command`)", line));
        }
        let Some(name) = self.static_lit(&words[0])? else {
            return Err(self.err("a command name that comes from an expansion", line));
        };
        let args = &words[1..];
        match name.as_str() {
            "echo" => self.lower_echo(args, line, out),
            "printf" => self.lower_printf(args, line, out),
            "true" | ":" => {
                out.push(set_st(0));
                Ok(())
            }
            "false" => {
                out.push(set_st(1));
                Ok(())
            }
            "test" | "[" => {
                let mut targs: Vec<&Word> = args.iter().collect();
                if name == "[" {
                    match targs.pop() {
                        Some(w) if self.static_lit(w)?.as_deref() == Some("]") => {}
                        _ => return Err(self.err("`[` without a closing `]`", line)),
                    }
                }
                let mut ta = Vec::new();
                for w in targs {
                    let lit = self.static_lit(w)?;
                    let e = self.word_one(w)?;
                    ta.push(TArg { lit, e });
                }
                let cond = self.test_expr(&ta, line)?;
                out.push(if_(cond, vec![set_st(0)], Some(vec![set_st(1)])));
                Ok(())
            }
            "local" => self.lower_local(args, line, out),
            "return" => {
                if !self.in_fn {
                    return Err(self.err("`return` outside a function", line));
                }
                let v = match args {
                    [] => var("__st"),
                    [w] => self.status_arg(w, line)?,
                    _ => return Err(self.err("`return` with more than one argument", line)),
                };
                out.push(ret(v));
                Ok(())
            }
            "exit" => {
                let v = match args {
                    [] => var("__st"),
                    [w] => self.status_arg(w, line)?,
                    _ => return Err(self.err("`exit` with more than one argument", line)),
                };
                let t = self.tmp();
                out.push(decl(&t, v));
                out.push(expr_stmt(cap("sys.exit", vec![var(&t)])));
                Ok(())
            }
            "break" | "continue" => {
                match args {
                    [] => {}
                    [w] if self.static_lit(w)?.as_deref() == Some("1") => {}
                    _ => return Err(self.err(&format!("`{name} N` (leaving more than one loop)"), line)),
                }
                let Some(ls) = self.loops.last().cloned() else {
                    return Err(self.err(&format!("`{name}` outside a loop"), line));
                };
                out.push(set(&ls, int(0)));
                out.push(if name == "break" { S::Break { meta: m() } } else { S::Continue { meta: m() } });
                Ok(())
            }
            "unset" => {
                for w in args {
                    match self.static_lit(w)? {
                        Some(n) if is_ident(&n) => {
                            let st = self.write_var(&n, s(""), line)?;
                            out.push(st);
                        }
                        _ => return Err(self.err(&format!("`unset {}`", w.raw), line)),
                    }
                }
                out.push(set_st(0));
                Ok(())
            }
            _ if self.funcs.contains(&name) => {
                if !self.in_fn && !self.defined.contains(&name) {
                    return Err(self.err(&format!("calling `{name}` before the line that defines it"), line));
                }
                let a = self.fields_list(args)?;
                out.push(set("__st", call(&fn_name(&name), vec![var("__g"), a])));
                Ok(())
            }
            _ => Err(self.err(&format!("command `{name}` (only a fixed set of builtins and functions defined in the script can run)"), line)),
        }
    }

    /// `return N` / `exit N`: N & 255, like the shells.
    fn status_arg(&mut self, w: &Word, line: usize) -> Result<E> {
        let v = self.word_one(w)?;
        let n = self.helper("__sh_int", vec![v, self.at("exit status", line), boolean(true)]);
        Ok(bin("%", bin("+", bin("%", n, int(256)), int(256)), int(256)))
    }

    fn lower_echo(&mut self, args: &[Word], line: usize, out: &mut Vec<S>) -> Result<()> {
        let is_opt = |f: &str| f.len() > 1 && f.starts_with('-') && f[1..].chars().all(|c| matches!(c, 'n' | 'e' | 'E'));
        let mut rest = args;
        let mut newline = true;
        let first_opt = match rest.first() {
            Some(first) => self.static_lit(first)?.filter(|f| is_opt(f)),
            None => None,
        };
        if let Some(f) = first_opt {
            if f != "-n" {
                return Err(self.err(&format!("`echo {f}`"), line));
            }
            newline = false;
            rest = &rest[1..];
        }
        let mut dynamic_first = false;
        if let Some(first) = rest.first() {
            match self.static_lit(first)? {
                Some(f) if is_opt(&f) => return Err(self.err(&format!("`echo` with option `{f}` after `-n`"), line)),
                Some(_) => {}
                None => dynamic_first = true,
            }
        }
        let fields = self.fields_list(rest)?;
        let (arr, text) = (self.tmp(), self.tmp());
        out.push(decl(&arr, fields));
        if dynamic_first {
            let g = self.helper("__sh_echo_guard", vec![var(&arr), self.at("echo", line)]);
            out.push(expr_stmt(g));
        }
        out.push(decl(&text, call("str.join", vec![var(&arr), s(" ")])));
        if self.d == Dialect::Zsh {
            // zsh's echo interprets backslash escapes by default.
            out.push(if_(
                call("str.contains", vec![var(&text), s("\\")]),
                vec![throw(s(&format!(
                    "unsupported: zsh `echo` of text containing a backslash (escape sequences) at line {line} (zsh walker)"
                )))],
                None,
            ));
        }
        let c = if newline { "io.print" } else { "io.write" };
        out.push(expr_stmt(cap(c, vec![var(&text)])));
        out.push(set_st(0));
        Ok(())
    }

    fn lower_printf(&mut self, args: &[Word], line: usize, out: &mut Vec<S>) -> Result<()> {
        let Some(fmt_word) = args.first() else {
            return Err(self.err("`printf` without a format", line));
        };
        let Some(fmt) = self.static_lit(fmt_word)? else {
            return Err(self.err("`printf` with a format that comes from an expansion", line));
        };
        if fmt.starts_with('-') {
            return Err(self.err(&format!("`printf {fmt}` (options)"), line));
        }
        // Pieces: Ok(text) or Err(conversion char).
        let mut pieces: Vec<std::result::Result<String, char>> = Vec::new();
        let mut lit = String::new();
        let cs: Vec<char> = fmt.chars().collect();
        let mut i = 0;
        while i < cs.len() {
            match cs[i] {
                '\\' => {
                    let c = cs.get(i + 1).copied();
                    lit.push(match c {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('\\') => '\\',
                        Some('"') => '"',
                        Some('\'') => '\'',
                        _ => return Err(self.err(&format!("`printf` escape `\\{}`", c.map(|c| c.to_string()).unwrap_or_default()), line)),
                    });
                    i += 2;
                }
                '%' => {
                    match cs.get(i + 1) {
                        Some('%') => lit.push('%'),
                        Some(c @ ('s' | 'd')) => {
                            pieces.push(Ok(std::mem::take(&mut lit)));
                            pieces.push(Err(*c));
                        }
                        c => {
                            return Err(self.err(
                                &format!("`printf` conversion `%{}` (only %s, %d and %% are supported)", c.map(|c| c.to_string()).unwrap_or_default()),
                                line,
                            ));
                        }
                    }
                    i += 2;
                }
                c => {
                    lit.push(c);
                    i += 1;
                }
            }
        }
        pieces.push(Ok(lit));
        let mut values = Vec::new();
        for w in &args[1..] {
            values.push(self.word_one(w)?);
        }
        let n_conv = pieces.iter().filter(|p| p.is_err()).count();
        let mut parts = Vec::new();
        let mut next = 0;
        loop {
            for p in &pieces {
                match p {
                    Ok(t) if t.is_empty() => {}
                    Ok(t) => parts.push(s(t)),
                    Err(c) => {
                        let v = values.get(next).cloned();
                        next += 1;
                        parts.push(match (c, v) {
                            ('s', Some(v)) => v,
                            ('s', None) => s(""),
                            (_, Some(v)) => to_str(self.helper("__sh_int", vec![v, self.at("printf %d", line), boolean(false)])),
                            (_, None) => s("0"),
                        });
                    }
                }
            }
            if n_conv == 0 || next >= values.len() {
                break;
            }
        }
        out.push(expr_stmt(cap("io.write", vec![concat(parts)])));
        out.push(set_st(0));
        Ok(())
    }

    fn lower_local(&mut self, args: &[Word], line: usize, out: &mut Vec<S>) -> Result<()> {
        if !self.in_fn {
            return Err(self.err("`local` outside a function", line));
        }
        if self.depth > 0 {
            return Err(self.err("`local` inside a compound command (only at the top of a function body)", line));
        }
        for w in args {
            let (name, value) = match w.raw.split_once('=') {
                Some((n, v)) => (n.to_string(), Some(Word { raw: v.to_string(), line: w.line })),
                None => (w.raw.clone(), None),
            };
            if !is_ident(&name) {
                return Err(self.err(&format!("`local {}`", w.raw), line));
            }
            let v = match &value {
                Some(v) => self.word_str(v)?,
                None => s(""),
            };
            self.locals.insert(name.clone());
            self.local_names.insert(name.clone());
            out.push(decl(&local_name(&name), v));
        }
        out.push(set_st(0));
        Ok(())
    }

    // ── test ──

    fn test_expr(&mut self, a: &[TArg], line: usize) -> Result<E> {
        let lit = |i: usize| a[i].lit.as_deref();
        match a.len() {
            0 => Ok(boolean(false)),
            1 => Ok(bin("!=", a[0].e.clone(), s(""))),
            2 => {
                if lit(0) == Some("!") {
                    return Ok(not(self.test_expr(&a[1..], line)?));
                }
                match lit(0) {
                    Some("-z") => Ok(bin("==", a[1].e.clone(), s(""))),
                    Some("-n") => Ok(bin("!=", a[1].e.clone(), s(""))),
                    Some(op) => Err(self.err(&format!("test operator `{op}`"), line)),
                    None => Err(self.err("a test operator that comes from an expansion", line)),
                }
            }
            3 => {
                if let Some(op) = lit(1).filter(|op| is_binary_test_op(op)) {
                    return self.test_binary(op, &a[0], &a[2], line);
                }
                if lit(0) == Some("!") {
                    return Ok(not(self.test_expr(&a[1..], line)?));
                }
                if lit(0) == Some("(") && lit(2) == Some(")") {
                    return self.test_expr(&a[1..2], line);
                }
                match lit(1) {
                    Some(op) => Err(self.err(&format!("test operator `{op}`"), line)),
                    None => Err(self.err("a test operator that comes from an expansion", line)),
                }
            }
            4 => {
                if lit(0) == Some("!") {
                    return Ok(not(self.test_expr(&a[1..], line)?));
                }
                if lit(0) == Some("(") && lit(3) == Some(")") {
                    return self.test_expr(&a[1..3], line);
                }
                Err(self.err("this 4-argument test expression", line))
            }
            _ => Err(self.err("a test expression with more than 4 arguments (`-a`/`-o` chains)", line)),
        }
    }

    fn test_binary(&mut self, op: &str, l: &TArg, r: &TArg, line: usize) -> Result<E> {
        let (l, r) = (l.e.clone(), r.e.clone());
        Ok(match op {
            "=" | "==" => bin("==", l, r),
            "!=" => bin("!=", l, r),
            "-a" => bin("and", bin("!=", l, s("")), bin("!=", r, s(""))),
            "-o" => bin("or", bin("!=", l, s("")), bin("!=", r, s(""))),
            _ => match TEST_INT_OPS.iter().find(|(o, _)| *o == op) {
                Some((_, cmp)) => {
                    let at = self.at(&format!("test `{op}`"), line);
                    let li = self.helper("__sh_int", vec![l, at.clone(), boolean(true)]);
                    let ri = self.helper("__sh_int", vec![r, at, boolean(true)]);
                    bin(cmp, li, ri)
                }
                None => return Err(self.err(&format!("test operator `{op}`"), line)),
            },
        })
    }

    // ── arithmetic ──

    /// Lower an arithmetic expression to (statements to run first, integer
    /// value). Assignments are allowed only as the whole expression and only
    /// when `allow_assign` (`(( … ))`, `for ((…))`), so the statements never
    /// change evaluation order.
    fn arith(&mut self, text: &str, line: usize, allow_assign: bool) -> Result<(Vec<S>, E)> {
        let toks = self.arith_tokens(text, line)?;
        if toks.is_empty() {
            return Ok((vec![], int(0)));
        }
        let mut p = ArithParser { toks: &toks, pos: 0 };
        let a = match p.assign() {
            Some(a) if p.pos == toks.len() => a,
            _ => return Err(self.err(&format!("arithmetic expression `{}`", text.trim()), line)),
        };
        match a {
            A::Assign(name, op, rhs) if allow_assign => {
                let r = self.arith_expr(&rhs, line)?;
                let v = if op == "=" {
                    r
                } else {
                    let cur = self.arith_ref(&Ref::Name(name.clone()), line);
                    bin(&op[..1], cur, r)
                };
                let t = self.tmp();
                let mut pre = vec![decl(&t, v)];
                pre.push(self.write_var(&name, to_str(var(&t)), line)?);
                Ok((pre, var(&t)))
            }
            A::Inc { name, delta, post } if allow_assign => {
                let t = self.tmp();
                let cur = self.arith_ref(&Ref::Name(name.clone()), line);
                let mut pre = vec![decl(&t, cur)];
                pre.push(self.write_var(&name, to_str(bin("+", var(&t), int(delta))), line)?);
                let v = if post { var(&t) } else { bin("+", var(&t), int(delta)) };
                Ok((pre, v))
            }
            a => Ok((vec![], self.arith_expr(&a, line)?)),
        }
    }

    fn arith_ref(&mut self, r: &Ref, line: usize) -> E {
        let (v, what) = match r {
            Ref::Name(n) => (self.read_var(n, line), format!("`{n}` in arithmetic")),
            Ref::Pos(n) => (self.helper("__sh_arg", vec![var("__a"), int(*n as i64)]), format!("`${n}` in arithmetic")),
        };
        let at = self.at(&what, line);
        self.helper("__sh_int", vec![v, at, boolean(false)])
    }

    fn arith_expr(&mut self, a: &A, line: usize) -> Result<E> {
        Ok(match a {
            A::Num(n) => int(*n),
            A::Ref(r) => self.arith_ref(r, line),
            A::Un("-", x) => E::UnaryOp { operator: "-".into(), operand: Box::new(self.arith_expr(x, line)?), meta: m() },
            A::Un("!", x) => tern(bin("==", self.arith_expr(x, line)?, int(0)), int(1), int(0)),
            A::Un(_, x) => self.arith_expr(x, line)?,
            A::Bin(op, l, r) => {
                let (l, r) = (self.arith_expr(l, line)?, self.arith_expr(r, line)?);
                match *op {
                    "+" | "-" | "*" | "/" | "%" => bin(op, l, r),
                    "&&" => tern(bin("and", bin("!=", l, int(0)), bin("!=", r, int(0))), int(1), int(0)),
                    "||" => tern(bin("or", bin("!=", l, int(0)), bin("!=", r, int(0))), int(1), int(0)),
                    cmp => tern(bin(cmp, l, r), int(1), int(0)),
                }
            }
            A::Tern(c, x, y) => {
                let c = bin("!=", self.arith_expr(c, line)?, int(0));
                tern(c, self.arith_expr(x, line)?, self.arith_expr(y, line)?)
            }
            A::Assign(..) | A::Inc { .. } => {
                return Err(self.err("an assignment (`=`, `+=`, `++`, …) inside a larger arithmetic expression or in `$((…))`", line));
            }
        })
    }

    fn arith_tokens(&self, text: &str, line: usize) -> Result<Vec<Tok>> {
        let cs: Vec<char> = text.chars().collect();
        let mut toks = Vec::new();
        let mut i = 0;
        while i < cs.len() {
            let c = cs[i];
            if c.is_whitespace() {
                i += 1;
            } else if c.is_ascii_digit() {
                let st = i;
                while i < cs.len() && cs[i].is_ascii_alphanumeric() || (i < cs.len() && cs[i] == '#') {
                    i += 1;
                }
                let t: String = cs[st..i].iter().collect();
                if (t.len() > 1 && t.starts_with('0')) || !t.chars().all(|c| c.is_ascii_digit()) {
                    return Err(self.err(&format!("number `{t}` in arithmetic (octal, hex and base# forms)"), line));
                }
                let n = t.parse().map_err(|_| self.err(&format!("number `{t}` (too large)"), line))?;
                toks.push(Tok::Num(n));
            } else if c.is_ascii_alphabetic() || c == '_' {
                let st = i;
                while i < cs.len() && (cs[i].is_ascii_alphanumeric() || cs[i] == '_') {
                    i += 1;
                }
                let n: String = cs[st..i].iter().collect();
                if cs.get(i) == Some(&'[') {
                    return Err(self.err(&format!("array element `{n}[…]` in arithmetic"), line));
                }
                toks.push(Tok::Name(n));
            } else if c == '$' {
                let rest: String = cs[i + 1..].iter().collect();
                if let Some(d) = rest.chars().next().filter(|d| d.is_ascii_digit() && *d != '0') {
                    toks.push(Tok::Pos(d.to_digit(10).unwrap() as usize));
                    i += 2;
                } else if let Some(inner) =
                    rest.strip_prefix('{').and_then(|r| r.split_once('}')).map(|(n, _)| n.to_string()).filter(|n| is_ident(n))
                {
                    i += inner.len() + 3;
                    toks.push(Tok::Name(inner));
                } else {
                    let n: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                    if !is_ident(&n) {
                        return Err(self.err(&format!("`${}` in arithmetic", rest.chars().take(8).collect::<String>()), line));
                    }
                    i += n.len() + 1;
                    toks.push(Tok::Name(n));
                }
            } else {
                let rest: String = cs[i..].iter().collect();
                let Some(op) = ARITH_OPS.iter().find(|o| rest.starts_with(**o)) else {
                    return Err(self.err(&format!("`{c}` in arithmetic"), line));
                };
                if ARITH_UNSUPPORTED.contains(op) {
                    return Err(self.err(&format!("arithmetic operator `{op}`"), line));
                }
                toks.push(Tok::Op(op));
                i += op.len();
            }
        }
        Ok(toks)
    }

    // ── words ──

    fn parse_word(&self, w: &Word) -> Result<Vec<Seg>> {
        let line = w.line;
        let raw = w.raw.as_str();
        if raw == "[" || raw == "]" {
            return Ok(vec![Seg::Lit(raw.to_string(), false)]);
        }
        let cs: Vec<char> = raw.chars().collect();
        let mut segs: Vec<Seg> = Vec::new();
        let push_lit = |segs: &mut Vec<Seg>, c: char, q: bool| {
            if let Some(Seg::Lit(t, lq)) = segs.last_mut() {
                if *lq == q {
                    t.push(c);
                    return;
                }
            }
            segs.push(Seg::Lit(c.to_string(), q));
        };
        let mut i = 0;
        while i < cs.len() {
            let c = cs[i];
            match c {
                '\'' => {
                    let Some(end) = cs[i + 1..].iter().position(|&x| x == '\'') else {
                        return Err(self.err("an unterminated single quote", line));
                    };
                    let text: String = cs[i + 1..i + 1 + end].iter().collect();
                    segs.push(Seg::Lit(text, true));
                    i += end + 2;
                }
                '"' => {
                    i += 1;
                    // An empty "" still makes a (quoted, empty) field.
                    segs.push(Seg::Lit(String::new(), true));
                    loop {
                        let Some(&c) = cs.get(i) else {
                            return Err(self.err("an unterminated double quote", line));
                        };
                        match c {
                            '"' => {
                                i += 1;
                                break;
                            }
                            '\\' => {
                                match cs.get(i + 1) {
                                    Some('\n') => {}
                                    Some(&n @ ('$' | '`' | '"' | '\\')) => push_lit(&mut segs, n, true),
                                    Some(&n) => {
                                        push_lit(&mut segs, '\\', true);
                                        push_lit(&mut segs, n, true);
                                    }
                                    None => push_lit(&mut segs, '\\', true),
                                }
                                i += 2;
                            }
                            '$' => {
                                i = self.parse_dollar(&cs, i, true, line, &mut segs)?;
                            }
                            '`' => return Err(self.err("command substitution (backticks)", line)),
                            c => {
                                push_lit(&mut segs, c, true);
                                i += 1;
                            }
                        }
                    }
                }
                '\\' => {
                    match cs.get(i + 1) {
                        Some('\n') => {}
                        Some(&n) => push_lit(&mut segs, n, true),
                        None => push_lit(&mut segs, '\\', false),
                    }
                    i += 2;
                }
                '$' => i = self.parse_dollar(&cs, i, false, line, &mut segs)?,
                '`' => return Err(self.err("command substitution (backticks)", line)),
                '*' | '?' | '[' => return Err(self.err(&format!("pathname expansion (unquoted `{c}` in `{raw}`)"), line)),
                '~' if i == 0 => return Err(self.err(&format!("tilde expansion (`{raw}`)"), line)),
                '=' if i == 0 && cs.len() > 1 && self.d == Dialect::Zsh => {
                    return Err(self.err(&format!("zsh `=command` expansion (`{raw}`)"), line));
                }
                '{' if {
                    let rest: String = cs[i..].iter().collect();
                    rest.find('}').is_some_and(|e| rest[..e].contains(',') || rest[..e].contains(".."))
                } =>
                {
                    return Err(self.err(&format!("brace expansion (`{raw}`)"), line));
                }
                c => {
                    push_lit(&mut segs, c, false);
                    i += 1;
                }
            }
        }
        Ok(segs)
    }

    /// Parse the expansion starting at `cs[i] == '$'`; returns the index after it.
    fn parse_dollar(&self, cs: &[char], i: usize, quoted: bool, line: usize, segs: &mut Vec<Seg>) -> Result<usize> {
        let next = cs.get(i + 1).copied();
        let zsh_suffix_check = |j: usize| -> Result<()> {
            if self.d == Dialect::Zsh {
                match cs.get(j) {
                    Some('[') => return Err(self.err("a zsh subscript (`$name[…]`)", line)),
                    Some(':') => return Err(self.err("a zsh modifier (`$name:…`)", line)),
                    _ => {}
                }
            }
            Ok(())
        };
        match next {
            Some('(') if cs.get(i + 2) == Some(&'(') => {
                let mut depth = 0i32;
                let mut j = i + 1;
                while j < cs.len() {
                    match cs[j] {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                if j >= cs.len() || cs[j - 1] != ')' {
                    return Err(self.err("an unterminated or ambiguous `$((`", line));
                }
                let inner: String = cs[i + 3..j - 1].iter().collect();
                if inner.contains("$(") || inner.contains('`') {
                    return Err(self.err("command substitution inside `$((…))`", line));
                }
                segs.push(Seg::Arith(inner, quoted));
                Ok(j + 1)
            }
            Some('(') => Err(self.err("command substitution `$(…)` (running commands is not supported)", line)),
            Some('{') => {
                let Some(end) = cs[i + 2..].iter().position(|&c| c == '}') else {
                    return Err(self.err("an unterminated `${`", line));
                };
                let inner: String = cs[i + 2..i + 2 + end].iter().collect();
                let seg = if let Some(n) = inner.strip_prefix('#') {
                    if n.is_empty() || n == "@" || n == "*" {
                        Seg::Count(quoted)
                    } else if is_ident(n) {
                        Seg::Len(n.to_string(), quoted)
                    } else {
                        return Err(self.err(&format!("parameter expansion `${{{inner}}}`"), line));
                    }
                } else if inner == "@" || inner == "*" {
                    Seg::All { star: inner == "*", quoted }
                } else if inner == "?" {
                    Seg::Status(quoted)
                } else if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) && inner != "0" {
                    Seg::Pos(inner.parse().map_err(|_| self.err("a huge positional parameter", line))?, quoted)
                } else if is_ident(&inner) {
                    Seg::Var(inner.clone(), quoted)
                } else {
                    return Err(self.err(&format!("parameter expansion `${{{inner}}}`"), line));
                };
                segs.push(seg);
                Ok(i + 3 + end)
            }
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                let mut j = i + 1;
                while j < cs.len() && (cs[j].is_ascii_alphanumeric() || cs[j] == '_') {
                    j += 1;
                }
                zsh_suffix_check(j)?;
                segs.push(Seg::Var(cs[i + 1..j].iter().collect(), quoted));
                Ok(j)
            }
            Some('0') => Err(self.err("`$0`", line)),
            Some(c) if c.is_ascii_digit() => {
                segs.push(Seg::Pos(c.to_digit(10).unwrap() as usize, quoted));
                Ok(i + 2)
            }
            Some('#') => {
                segs.push(Seg::Count(quoted));
                Ok(i + 2)
            }
            Some('?') => {
                segs.push(Seg::Status(quoted));
                Ok(i + 2)
            }
            Some(c @ ('@' | '*')) => {
                segs.push(Seg::All { star: c == '*', quoted });
                Ok(i + 2)
            }
            Some(c @ ('$' | '!' | '-')) => Err(self.err(&format!("special parameter `${c}`"), line)),
            Some('\'') if !quoted => Err(self.err("ANSI-C quoting `$'…'`", line)),
            Some('"') if !quoted => Err(self.err("locale quoting `$\"…\"`", line)),
            _ => {
                segs.push(Seg::Lit("$".to_string(), quoted));
                Ok(i + 1)
            }
        }
    }

    /// The word's text, if it has no expansions.
    fn static_lit(&self, w: &Word) -> Result<Option<String>> {
        let segs = self.parse_word(w)?;
        let mut out = String::new();
        for sg in &segs {
            match sg {
                Seg::Lit(t, _) => out.push_str(t),
                _ => return Ok(None),
            }
        }
        Ok(Some(out))
    }

    fn seg_expr(&mut self, seg: &Seg, line: usize) -> Result<E> {
        Ok(match seg {
            Seg::Lit(t, _) => s(t),
            Seg::Var(n, _) => self.read_var(n, line),
            Seg::Pos(n, _) => self.helper("__sh_arg", vec![var("__a"), int(*n as i64)]),
            Seg::Count(_) => to_str(len(var("__a"))),
            Seg::Status(_) => to_str(var("__st")),
            Seg::All { .. } => call("str.join", vec![var("__a"), s(" ")]),
            Seg::Len(n, _) => {
                let v = self.read_var(n, line);
                to_str(len(chars(v)))
            }
            Seg::Arith(text, _) => {
                let (pre, v) = self.arith(text, line, false)?;
                debug_assert!(pre.is_empty());
                to_str(v)
            }
        })
    }

    fn concat_segs(&mut self, segs: &[Seg], line: usize) -> Result<E> {
        let mut parts = Vec::new();
        for sg in segs {
            if matches!(sg, Seg::Lit(t, _) if t.is_empty()) {
                continue;
            }
            parts.push(self.seg_expr(sg, line)?);
        }
        Ok(concat(parts))
    }

    /// The word as one string, without field splitting (assignment values,
    /// `case` subjects).
    fn word_str(&mut self, w: &Word) -> Result<E> {
        let segs = self.parse_word(w)?;
        self.concat_segs(&segs, w.line)
    }

    fn fields(&mut self, w: &Word) -> Result<Fields> {
        let segs = self.parse_word(w)?;
        let line = w.line;
        let real: Vec<&Seg> = segs.iter().filter(|sg| !matches!(sg, Seg::Lit(t, _) if t.is_empty())).collect();
        if segs.iter().any(|sg| matches!(sg, Seg::All { .. })) {
            return match real.as_slice() {
                [Seg::All { star: false, quoted: true }] => Ok(Fields::Many(var("__a"))),
                [Seg::All { star: true, quoted: true }] => Ok(Fields::One(call("str.join", vec![var("__a"), s(" ")]))),
                [Seg::All { quoted: false, .. }] => Ok(Fields::Many(match self.d {
                    Dialect::Bash => {
                        let at = self.at("unquoted `$@`", line);
                        self.helper("__sh_split", vec![call("str.join", vec![var("__a"), s(" ")]), at])
                    }
                    Dialect::Zsh => self.helper("__sh_drop_empty", vec![var("__a")]),
                })),
                _ => Err(self.err(&format!("`$@`/`$*` combined with other text in one word (`{}`)", w.raw), line)),
            };
        }
        let splits = segs.iter().any(|sg| sg.is_expansion() && !sg.quoted() && !sg.is_numeric());
        if !splits {
            return Ok(Fields::One(self.concat_segs(&segs, line)?));
        }
        match self.d {
            Dialect::Bash => {
                if segs.iter().any(|sg| sg.quoted()) {
                    return Err(self.err(
                        &format!("a word mixing quoted text with an unquoted expansion (`{}`; quote the whole word)", w.raw),
                        line,
                    ));
                }
                let v = self.concat_segs(&segs, line)?;
                let at = self.at(&format!("unquoted `{}`", w.raw), line);
                Ok(Fields::Many(self.helper("__sh_split", vec![v, at])))
            }
            Dialect::Zsh => {
                let v = self.concat_segs(&segs, line)?;
                if real.len() == 1 {
                    Ok(Fields::Many(self.helper("__sh_nonempty", vec![v])))
                } else {
                    Ok(Fields::One(v))
                }
            }
        }
    }

    /// The word where exactly one field is required (`test`, `printf`).
    fn word_one(&mut self, w: &Word) -> Result<E> {
        Ok(match self.fields(w)? {
            Fields::One(e) => e,
            Fields::Many(arr) => {
                let at = self.at(&format!("`{}`", w.raw), w.line);
                self.helper("__sh_one", vec![arr, at])
            }
        })
    }

    /// The argument list as an array of strings.
    fn fields_list(&mut self, words: &[Word]) -> Result<E> {
        let mut parts: Vec<E> = Vec::new();
        let mut run: Vec<E> = Vec::new();
        for w in words {
            match self.fields(w)? {
                Fields::One(e) => run.push(e),
                Fields::Many(a) => {
                    if !run.is_empty() {
                        parts.push(array(std::mem::take(&mut run)));
                    }
                    parts.push(a);
                }
            }
        }
        if !run.is_empty() || parts.is_empty() {
            parts.push(array(run));
        }
        // `[] + arr` copies, so a function can never alias the caller's `__a`.
        if parts.len() == 1 && !matches!(parts[0], E::ArrayLiteral { .. }) {
            parts.insert(0, array(vec![]));
        }
        Ok(concat(parts))
    }
}

// ── Run-time helpers (CAST functions added to programs that use them) ────────

fn is_ws(c: E) -> E {
    bin("or", bin("or", bin("==", c.clone(), s(" ")), bin("==", c.clone(), s("\t"))), bin("==", c, s("\n")))
}

fn helper_fn(name: &str) -> Function {
    match name {
        // __sh_arg(a, n): "$n", or "" past the end.
        "__sh_arg" => func(
            &["a", "n"],
            vec![
                if_(bin("<=", var("n"), len(var("a"))), vec![ret(index(var("a"), bin("-", var("n"), int(1))))], None),
                ret(s("")),
            ],
        ),
        // __sh_split(v, at): bash field splitting on space/tab/newline.
        // A glob character in a field would be pathname-expanded: refuse.
        "__sh_split" => func(
            &["v", "at"],
            vec![
                decl("fields", array(vec![])),
                decl("cur", s("")),
                for_(
                    "c",
                    chars(var("v")),
                    vec![if_(
                        is_ws(var("c")),
                        vec![if_(
                            bin("!=", var("cur"), s("")),
                            vec![set("fields", bin("+", var("fields"), array(vec![var("cur")]))), set("cur", s(""))],
                            None,
                        )],
                        Some(vec![
                            if_(
                                bin("or", bin("or", bin("==", var("c"), s("*")), bin("==", var("c"), s("?"))), bin("==", var("c"), s("["))),
                                vec![throw(bin(
                                    "+",
                                    bin("+", s("unsupported: pathname expansion of `"), var("v")),
                                    bin("+", s("` from "), var("at")),
                                ))],
                                None,
                            ),
                            set("cur", bin("+", var("cur"), var("c"))),
                        ]),
                    )],
                ),
                if_(bin("!=", var("cur"), s("")), vec![set("fields", bin("+", var("fields"), array(vec![var("cur")])))], None),
                ret(var("fields")),
            ],
        ),
        // __sh_nonempty(v): zsh drops an empty unquoted expansion.
        "__sh_nonempty" => func(
            &["v"],
            vec![if_(bin("==", var("v"), s("")), vec![ret(array(vec![]))], None), ret(array(vec![var("v")]))],
        ),
        "__sh_drop_empty" => func(
            &["a"],
            vec![
                decl("out", array(vec![])),
                for_("x", var("a"), vec![if_(bin("!=", var("x"), s("")), vec![set("out", bin("+", var("out"), array(vec![var("x")])))], None)]),
                ret(var("out")),
            ],
        ),
        // __sh_one(arr, at): the single field of a word that must be one word.
        "__sh_one" => func(
            &["arr", "at"],
            vec![
                if_(
                    bin("!=", len(var("arr")), int(1)),
                    vec![throw(bin(
                        "+",
                        bin("+", s("unsupported: an unquoted expansion that became "), to_str(len(var("arr")))),
                        bin("+", s(" words where one is needed: "), var("at")),
                    ))],
                    None,
                ),
                ret(index(var("arr"), int(0))),
            ],
        ),
        // __sh_int(v, at, test): a decimal integer with optional sign and
        // surrounding whitespace. In arithmetic (`test` false) an empty
        // value is 0 and a leading zero (octal) is refused; in `test` an
        // empty value is refused and leading zeros are decimal.
        "__sh_int" => {
            let c_at = |i: E| index(var("cs"), i);
            let skip_ws = || {
                while_(
                    bin("and", bin("<", var("i"), var("n")), is_ws(c_at(var("i")))),
                    vec![set("i", bin("+", var("i"), int(1)))],
                )
            };
            let bad = |what: &str| {
                throw(bin(
                    "+",
                    bin("+", bin("+", s(&format!("unsupported: {what} `")), var("v")), s("` for ")),
                    var("at"),
                ))
            };
            func(
                &["v", "at", "test"],
                vec![
                    decl("cs", chars(var("v"))),
                    decl("n", len(var("cs"))),
                    decl("i", int(0)),
                    skip_ws(),
                    if_(
                        bin("==", var("i"), var("n")),
                        vec![if_(var("test"), vec![bad("empty value instead of an integer")], None), ret(int(0))],
                        None,
                    ),
                    decl("neg", boolean(false)),
                    if_(
                        bin("==", c_at(var("i")), s("-")),
                        vec![set("neg", boolean(true)), set("i", bin("+", var("i"), int(1)))],
                        Some(vec![if_(bin("==", c_at(var("i")), s("+")), vec![set("i", bin("+", var("i"), int(1)))], None)]),
                    ),
                    decl("start", var("i")),
                    decl("val", int(0)),
                    while_(
                        bin(
                            "and",
                            bin("<", var("i"), var("n")),
                            bin("and", bin(">=", c_at(var("i")), s("0")), bin("<=", c_at(var("i")), s("9"))),
                        ),
                        vec![
                            set(
                                "val",
                                bin("+", bin("*", var("val"), int(10)), bin("-", cap("conv.ord", vec![c_at(var("i"))]), int(48))),
                            ),
                            set("i", bin("+", var("i"), int(1))),
                        ],
                    ),
                    decl("digits", bin("-", var("i"), var("start"))),
                    skip_ws(),
                    if_(bin("or", bin("==", var("digits"), int(0)), bin("!=", var("i"), var("n"))), vec![bad("non-integer value")], None),
                    if_(
                        bin("and", not(var("test")), bin("and", bin(">", var("digits"), int(1)), bin("==", c_at(var("start")), s("0")))),
                        vec![bad("leading-zero (octal) number")],
                        None,
                    ),
                    if_(var("neg"), vec![ret(bin("-", int(0), var("val")))], None),
                    ret(var("val")),
                ],
            )
        }
        // __sh_echo_guard(arr, at): an expansion that turns out to be an echo
        // option (`-n`, `-e`, …) would change what echo does: refuse.
        "__sh_echo_guard" => func(
            &["arr", "at"],
            vec![
                if_(
                    bin("==", len(var("arr")), int(0)),
                    vec![ret(int(0))],
                    None,
                ),
                decl("cs", chars(index(var("arr"), int(0)))),
                if_(
                    bin("and", bin(">", len(var("cs")), int(1)), bin("==", index(var("cs"), int(0)), s("-"))),
                    vec![
                        decl("opt", boolean(true)),
                        decl("i", int(1)),
                        while_(
                            bin("<", var("i"), len(var("cs"))),
                            vec![
                                decl("c", index(var("cs"), var("i"))),
                                if_(
                                    bin("and", bin("and", bin("!=", var("c"), s("n")), bin("!=", var("c"), s("e"))), bin("!=", var("c"), s("E"))),
                                    vec![set("opt", boolean(false))],
                                    None,
                                ),
                                set("i", bin("+", var("i"), int(1))),
                            ],
                        ),
                        if_(
                            var("opt"),
                            vec![throw(bin(
                                "+",
                                bin("+", s("unsupported: an expansion that is an echo option `"), index(var("arr"), int(0))),
                                bin("+", s("` in "), var("at")),
                            ))],
                            None,
                        ),
                    ],
                    None,
                ),
                ret(int(0)),
            ],
        ),
        // __sh_noglob(v, at): an unquoted expansion used as a case pattern.
        "__sh_noglob" => func(
            &["v", "at"],
            vec![
                if_(
                    bin(
                        "or",
                        bin("or", call("str.contains", vec![var("v"), s("*")]), call("str.contains", vec![var("v"), s("?")])),
                        call("str.contains", vec![var("v"), s("[")]),
                    ),
                    vec![throw(bin("+", bin("+", s("unsupported: glob pattern `"), var("v")), bin("+", s("` from "), var("at"))))],
                    None,
                ),
                ret(var("v")),
            ],
        ),
        other => unreachable!("unknown shell helper {other}"),
    }
}
