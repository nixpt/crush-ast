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

/// Stream state, kept in the global map `__g` so a shell function sees the
/// redirection its caller set up, the way a real shell's fds are inherited.
/// `__sh_stream`: 0 = the host's output, 1 = `__sh_buf` (a pipeline stage or a
/// redirected command). `__sh_in`: the text a command reads as stdin (a
/// pipeline's previous stage, a `<` file or a here-document); `""` means "no
/// redirection", and `read` then uses the host's stdin.
const STREAM: &str = "__sh_stream";
const BUF: &str = "__sh_buf";
const IN: &str = "__sh_in";
/// 1 when `__sh_in` is this command's stdin — including when it is empty, which
/// is not the same as "no redirection" (`grep none f | wc -l` must not read the
/// host's stdin just because the buffer is empty).
const IN_SET: &str = "__sh_in_set";
/// 1 when redirected stdin came from a file (`<`), 0 when it came from a pipe or
/// a here-document. `wc` is the only command that has to know the difference:
/// its multi-column padding for a file's stdin changed between coreutils
/// versions, so that case is refused instead of guessed.
const IN_FILE: &str = "__sh_in_file";
/// Where a `>` / `>>` command's output goes, and whether it appends.
const OUTF: &str = "__sh_outf";
const APPEND: &str = "__sh_append";
/// 1 while a pipeline stage runs: a stage is a subshell, so `exit` in one leaves
/// that stage (the pipeline, then the script, carries on) — not the script.
const STAGE: &str = "__sh_stage";
/// Pathname-expansion options, the ones that change what a pattern matching
/// nothing does — `shopt -s nullglob` / `shopt -s failglob` in bash,
/// `setopt null_glob` / `setopt nomatch` in zsh.
///
/// `nullglob`: a pattern that matches nothing expands to no word at all.
/// `failglob` (bash): the command is not run, prints `no match`, and its status
/// is 1 — the script carries on.
/// `nomatch` (zsh, on by default): `no matches found`, which in a script ends
/// it — bash's `failglob` alone does not do that, so the two are separate keys.
const NULLGLOB: &str = "__sh_nullglob";
const FAILGLOB: &str = "__sh_failglob";
const NOMATCH: &str = "__sh_nomatch";
/// Prefix of the error `failglob` raises: a command carrying a glob catches it,
/// sets status 1 and carries on, where every other error still ends the run.
const FAILGLOB_SENTINEL: &str = "__crush_failglob__";

/// Printable ASCII in codepoint order (space through `~`). `tr` expands a range
/// like `a-z` by slicing this, so a range never has to call `conv.ord`/`conv.chr`
/// (and a set naming anything outside it is refused).
const RANGE_ALPHA: &str = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";

/// One shell word, as written in the source (quotes and `$` included).
#[derive(Clone, Debug)]
pub struct Word {
    pub raw: String,
    pub line: usize,
}

/// `name=value` / `name+=value`, `name=(…)` / `name+=(…)`, and `name[i]=value`.
#[derive(Clone, Debug)]
pub struct Assign {
    pub name: String,
    /// `a[i]=…`: the subscript.
    pub index: Option<Word>,
    /// `a=(… …)`: the array elements. `Some(vec![])` is `a=()`.
    pub array: Option<Vec<Word>>,
    pub value: Word,
    pub append: bool,
    pub line: usize,
}

/// One redirection: `<`, `>`, `>>` or a here-document.
#[derive(Clone, Debug)]
pub struct Redir {
    pub kind: RedirKind,
    /// The path (`<`, `>`, `>>`) or the here-document's body.
    pub word: Word,
    /// A here-document with an unquoted delimiter expands `$…` in its body.
    pub expand: bool,
    pub line: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedirKind {
    In,
    Out,
    Append,
    HereDoc,
}

#[derive(Clone, Debug)]
pub enum Cmd {
    Simple { assigns: Vec<Assign>, words: Vec<Word>, line: usize },
    /// `cmd < f`, `cmd > f`, `cmd >> f`, `cmd <<EOT`: the command with its
    /// streams pointed somewhere else.
    Redirected { inner: Box<Cmd>, redirs: Vec<Redir>, line: usize },
    If { branches: Vec<(List, List)>, else_body: Option<List>, line: usize },
    Loop { cond: List, body: List, until: bool, line: usize },
    /// `for var in words`; `words: None` iterates `"$@"`.
    For { var: String, words: Option<Vec<Word>>, body: List, line: usize },
    ArithFor { init: Option<String>, cond: Option<String>, step: Option<String>, body: List, line: usize },
    Case { word: Word, arms: Vec<(Vec<Word>, List)>, line: usize },
    Group(List),
    /// `[[ … ]]`: a conditional expression that does no field splitting and
    /// no pathname expansion (its right-hand operands are patterns).
    ExtendedTest { cond: Cond, line: usize },
    Arith { expr: String, line: usize },
    FuncDef { name: String, body: List, line: usize },
    /// A construct the parser adapter already knows it cannot lower.
    Unsupported { what: String, line: usize },
}

/// One `[[ … ]]` expression.
#[derive(Clone, Debug)]
pub enum Cond {
    Not(Box<Cond>),
    And(Box<Cond>, Box<Cond>),
    Or(Box<Cond>, Box<Cond>),
    /// `-z word` / `-n word`.
    Unary(&'static str, Word),
    /// `left op right`. `op` is `==`/`=`/`!=` (pattern match), `<`/`>` (string
    /// order), or an integer operator (`-eq`, `-ne`, `-lt`, `-le`, `-gt`, `-ge`).
    Binary(&'static str, Word, Word),
}

#[derive(Clone, Debug)]
pub struct Pipeline {
    pub negated: bool,
    /// `a | b | c`: one entry per stage. A single stage is the plain command.
    pub stages: Vec<Cmd>,
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
        if let [Cmd::FuncDef { name, .. }] = item.first.stages.as_slice() {
            lw.funcs.insert(name.clone());
        }
    }
    let mut body = Vec::new();
    for item in script {
        match item.first.stages.as_slice() {
            [Cmd::FuncDef { name, body: fbody, line }] if item.rest.is_empty() && !item.first.negated => {
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

    // Globals start *unset* (null): `${x-y}` and `${x:=y}` can tell that apart
    // from a variable that was assigned the empty string.
    let mut main = vec![decl("__g", E::ObjectLiteral { properties: vec![], meta: m() })];
    for g in &lw.globals {
        main.push(set_index("__g", g, null()));
    }
    main.push(decl("__a", E::ArrayLiteral { elements: vec![], meta: m() }));
    main.push(decl("__st", int(0)));
    for (key, init) in [
        (STREAM, int(0)),
        (BUF, s("")),
        (IN, s("")),
        (IN_SET, int(0)),
        (IN_FILE, int(0)),
        (OUTF, s("")),
        (APPEND, int(0)),
        (STAGE, int(0)),
        (NULLGLOB, int(0)),
        (FAILGLOB, int(0)),
        (NOMATCH, int(if dialect == Dialect::Zsh { 1 } else { 0 })),
    ] {
        main.push(set_index("__g", key, init));
    }
    main.extend(body);
    main.push(if_(
        bin("!=", var("__st"), int(0)),
        vec![expr_stmt(cap("sys.exit", vec![var("__st")]))],
        None,
    ));

    let mut functions = std::mem::take(&mut lw.functions);
    functions.insert("main".to_string(), func(&[], main));
    // Helpers call other helpers (`__sh_esc` needs `__sh_dval`), so close the
    // set over references before emitting them.
    let mut helpers = lw.helpers.clone();
    loop {
        let mut added = false;
        for h in helpers.clone() {
            for dep in helper_deps(&helper_fn(h)) {
                if helpers.insert(dep) {
                    added = true;
                }
            }
        }
        if !added {
            break;
        }
    }
    for helper in helpers {
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
fn null() -> E {
    E::NullLiteral { meta: m() }
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
/// `s` with the pattern metacharacters escaped, for text that must stay
/// literal inside a pattern.
fn escape_pattern(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
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
    /// `${name<op>word}`, op ∈ `:- - := = :+ + :? ?`.
    Default { name: String, op: &'static str, word: Word, quoted: bool },
    /// `${name<op>pat}`, op ∈ `# ## % %%` (remove a prefix/suffix pattern).
    Trim { name: String, op: &'static str, pat: Word, quoted: bool },
    /// `${name/pat/rep}` (`//` replaces every occurrence); `anchor` is 0,
    /// 1 for `/#pat` (the start) or 2 for `/%pat` (the end).
    Subst { name: String, all: bool, anchor: u8, pat: Word, rep: Word, quoted: bool },
    /// `${name:off}` / `${name:off:len}` (a substring, by character).
    Substr { name: String, off: Word, len: Option<Word>, quoted: bool },
    /// `${name^^}` / `${name,,}`.
    Case { name: String, upper: bool, quoted: bool },
    /// `${a[i]}`; `len` is `${#a[i]}`.
    ArrElem { name: String, index: Word, len: bool, quoted: bool },
    /// `${a[@]}` / `${a[*]}`.
    ArrAll { name: String, star: bool, quoted: bool },
    /// `${#a[@]}` / `${#a[*]}`.
    ArrCount { name: String, quoted: bool },
}

impl Seg {
    fn quoted(&self) -> bool {
        match self {
            Seg::Lit(_, q)
            | Seg::Var(_, q)
            | Seg::Pos(_, q)
            | Seg::Count(q)
            | Seg::Status(q)
            | Seg::Len(_, q)
            | Seg::Arith(_, q)
            | Seg::Default { quoted: q, .. }
            | Seg::Trim { quoted: q, .. }
            | Seg::Subst { quoted: q, .. }
            | Seg::Substr { quoted: q, .. }
            | Seg::Case { quoted: q, .. }
            | Seg::ArrElem { quoted: q, .. }
            | Seg::ArrCount { quoted: q, .. } => *q,
            Seg::All { quoted, .. } | Seg::ArrAll { quoted, .. } => *quoted,
        }
    }
    fn is_expansion(&self) -> bool {
        !matches!(self, Seg::Lit(..))
    }
    /// Expands to digits only: never empty, never split, never a glob.
    fn is_numeric(&self) -> bool {
        matches!(self, Seg::Count(_) | Seg::Status(_) | Seg::Len(..) | Seg::Arith(..) | Seg::ArrCount { .. })
    }
    /// The variable this segment reads, if it reads exactly one.
    fn var_name(&self) -> Option<&str> {
        match self {
            Seg::Var(n, _)
            | Seg::Len(n, _)
            | Seg::Default { name: n, .. }
            | Seg::Trim { name: n, .. }
            | Seg::Subst { name: n, .. }
            | Seg::Substr { name: n, .. }
            | Seg::Case { name: n, .. }
            | Seg::ArrElem { name: n, .. }
            | Seg::ArrAll { name: n, .. }
            | Seg::ArrCount { name: n, .. } => Some(n),
            _ => None,
        }
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
const ARITH_UNSUPPORTED: &[&str] = &["<<=", ">>=", "&=", "|=", "^=", "<<", ">>", "~", "&", "|", "^", ","];

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
            return self.power();
        }
        let mut l = self.binary(level + 1)?;
        while let Some(op) = self.peek_op().filter(|o| LEVELS[level].contains(o)) {
            self.pos += 1;
            let r = self.binary(level + 1)?;
            l = A::Bin(op, Box::new(l), Box::new(r));
        }
        Some(l)
    }
    /// `**`, right-associative and binding looser than unary minus: bash
    /// evaluates `-2**2` as `(-2)**2 == 4`.
    fn power(&mut self) -> Option<A> {
        let base = self.unary()?;
        if self.eat("**") {
            let exp = self.power()?;
            return Some(A::Bin("**", Box::new(base), Box::new(exp)));
        }
        Some(base)
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

/// One piece of a `printf` format: literal text, or a conversion with its
/// flags, width and precision.
#[derive(Debug)]
enum Piece {
    Lit(String),
    Conv(Conv),
}

#[derive(Debug)]
struct Conv {
    conv: char,
    left: bool,
    zero: bool,
    plus: bool,
    space: bool,
    width: usize,
    prec: Option<usize>,
}

/// Split a `printf` format into literal text and conversions.
fn printf_pieces(fmt: &str) -> std::result::Result<Vec<Piece>, String> {
    let cs: Vec<char> = fmt.chars().collect();
    let mut pieces = Vec::new();
    let mut lit = String::new();
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
                    other => return Err(format!("`printf` escape `\\{}`", other.map(|c| c.to_string()).unwrap_or_default())),
                });
                i += 2;
            }
            '%' => {
                if cs.get(i + 1) == Some(&'%') {
                    lit.push('%');
                    i += 2;
                    continue;
                }
                let mut j = i + 1;
                let (mut left, mut zero, mut plus, mut space) = (false, false, false, false);
                while let Some(&c) = cs.get(j) {
                    match c {
                        '-' => left = true,
                        '0' => zero = true,
                        '+' => plus = true,
                        ' ' => space = true,
                        _ => break,
                    }
                    j += 1;
                }
                let mut width = 0usize;
                while let Some(d) = cs.get(j).and_then(|c| c.to_digit(10)) {
                    width = width.saturating_mul(10).saturating_add(d as usize);
                    j += 1;
                }
                let mut prec = None;
                if cs.get(j) == Some(&'.') {
                    j += 1;
                    let mut p = 0usize;
                    while let Some(d) = cs.get(j).and_then(|c| c.to_digit(10)) {
                        p = p.saturating_mul(10).saturating_add(d as usize);
                        j += 1;
                    }
                    prec = Some(p);
                }
                let Some(&conv) = cs.get(j) else {
                    return Err("`printf` format ending inside a conversion".to_string());
                };
                if !matches!(conv, 's' | 'd' | 'i' | 'u' | 'c' | 'x' | 'X' | 'o' | 'b' | 'f' | 'e' | 'E' | 'g' | 'G') {
                    return Err(format!(
                        "`printf` conversion `%{conv}` (only %s, %d, %i, %u, %c, %x, %X, %o, %b, %f, %e, %E, %g, %G and %% are supported)"
                    ));
                }
                pieces.push(Piece::Lit(std::mem::take(&mut lit)));
                pieces.push(Piece::Conv(Conv { conv, left, zero, plus, space, width, prec }));
                i = j + 1;
            }
            c => {
                lit.push(c);
                i += 1;
            }
        }
    }
    pieces.push(Piece::Lit(lit));
    Ok(pieces)
}

/// The index of the `}` matching the `{` at `open` in `${…}`, skipping nested
/// `${…}`.
fn find_brace_end(cs: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut j = open;
    while j < cs.len() {
        match cs[j] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// `$1`…`: all digits and not 0.
fn is_positional(name: &str) -> bool {
    !name.is_empty() && name != "0" && name.chars().all(|c| c.is_ascii_digit())
}

/// `name[…]` → (`name`, `…`).
fn split_subscript(s: &str) -> Option<(&str, &str)> {
    let (name, rest) = s.split_once('[')?;
    Some((name, rest.strip_suffix(']')?))
}

/// Split `${x:off:len}` at its top-level `:` (not inside parentheses).
fn split_top_colon(s: &str) -> (&str, Option<&str>) {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ':' if depth == 0 => return (&s[..i], Some(&s[i + 1..])),
            _ => {}
        }
    }
    (s, None)
}

/// Split `${x/pat/rep}` into `pat` and `rep` at the first top-level `/`.
fn split_top_slash(s: &str) -> (String, String) {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            '/' if depth == 0 => return (s[..i].to_string(), s[i + 1..].to_string()),
            _ => {}
        }
    }
    (s.to_string(), String::new())
}

/// Is `rest` one of the colon-less default operators (`-`, `=`, `+`, `?`)?
fn rest_matches_no_colon_op(rest: &[char]) -> bool {
    matches!(rest.first(), Some('-' | '=' | '+' | '?'))
}

/// Value of one digit in a `base#digits` literal (bash's ordering).
fn digit_value(c: char) -> Option<u32> {
    match c {
        '0'..='9' => Some(c as u32 - '0' as u32),
        'a'..='z' => Some(c as u32 - 'a' as u32 + 10),
        'A'..='Z' => Some(c as u32 - 'A' as u32 + 36),
        '@' => Some(62),
        '_' => Some(63),
        _ => None,
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
    /// Names that hold an array somewhere in the script, so reads of them use
    /// the array-aware string conversion.
    arrays: HashSet<String>,
    in_fn: bool,
    locals: HashSet<String>,
    /// Compound-command nesting inside the current function body.
    depth: usize,
    /// Per enclosing loop: the variable holding the loop's exit status.
    loops: Vec<String>,
    /// Globals written since this set was last taken. A pipeline stage's writes
    /// are rolled back with it: the shells run each stage in a subshell, so
    /// `printf x | read a` leaves `a` unset.
    written: BTreeSet<String>,
    /// Inside a command already wrapped for `failglob`, so the nested commands
    /// (a redirection's inner command) are not wrapped a second time.
    glob_guard: bool,
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
            arrays: HashSet::new(),
            in_fn: false,
            locals: HashSet::new(),
            depth: 0,
            loops: Vec::new(),
            written: BTreeSet::new(),
            glob_guard: false,
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

    /// `__g[key]`, the stream state set by a redirection or a pipeline stage.
    fn stream(&self, key: &str) -> E {
        index(var("__g"), s(key))
    }

    /// Write `text` where this command's stdout points: the host's output, or
    /// the buffer a pipeline stage or a redirected command fills. Everything
    /// that prints (and every builtin's output) goes through here, so `echo x >
    /// out` and `echo x | wc -l` are the same code path as a plain `echo x`.
    fn emit(&mut self, out: &mut Vec<S>, text: E, newline: bool) {
        // `io.print` already ends its line; `io.write` is the no-newline form.
        let host = if newline {
            cap("io.print", vec![text.clone()])
        } else {
            cap("io.write", vec![text.clone()])
        };
        let buffered = if newline { bin("+", text, s("\n")) } else { text };
        out.push(if_(
            bin("==", self.stream(STREAM), int(0)),
            vec![expr_stmt(host)],
            Some(vec![set_index("__g", BUF, bin("+", self.stream(BUF), buffered))]),
        ));
    }

    /// Remember the stdin a command starts with, so it can be put back after
    /// the command's own `<` / here-document / pipeline stage.
    fn push_stdin(&mut self, out: &mut Vec<S>) -> (String, String) {
        let (text, set) = (self.tmp(), self.tmp());
        out.push(decl(&text, self.stream(IN)));
        out.push(decl(&set, self.stream(IN_FILE)));
        (text, set)
    }

    fn pop_stdin(&mut self, out: &mut Vec<S>, saved: &(String, String)) {
        let (text, file) = saved;
        out.push(set_index("__g", IN_SET, int(0)));
        out.push(set_index("__g", IN_FILE, var(file)));
        out.push(set_index("__g", IN, var(text)));
    }

    /// Point this command's stdin at `text`; `from_file` is true for `< file`
    /// (which `wc` sizes its columns by) and false for a pipe or here-document.
    fn set_stdin(&mut self, out: &mut Vec<S>, text: E, from_file: bool) {
        out.push(set_index("__g", IN, text));
        out.push(set_index("__g", IN_SET, int(1)));
        out.push(set_index("__g", IN_FILE, int(if from_file { 1 } else { 0 })));
    }

    fn helper(&mut self, name: &'static str, args: Vec<E>) -> E {
        self.helpers.insert(name);
        call(name, args)
    }

    fn read_var(&mut self, name: &str, line: usize) -> E {
        if is_positional(name) {
            return self.positional(name);
        }
        if self.in_fn {
            if self.locals.contains(name) {
                return var(&local_name(name));
            }
            self.fn_global_refs.push((name.to_string(), line));
        }
        self.global_reads.push((name.to_string(), line));
        let raw = index(var("__g"), s(name));
        self.stringify(name, raw)
    }

    /// The variable's value as a shell string: an unset (`null`) global reads
    /// as `""`, and an array reads as its first element in bash / all its
    /// elements joined in zsh.
    fn stringify(&mut self, name: &str, raw: E) -> E {
        let one = boolean(self.d == Dialect::Zsh);
        if self.arrays.contains(name) {
            self.helper("__sh_arr_str", vec![raw, one])
        } else {
            self.helper("__sh_str", vec![raw])
        }
    }

    /// The raw value, for the unset test (`null`) and array operations. Locals
    /// only ever hold strings.
    fn read_var_raw(&mut self, name: &str) -> E {
        if is_positional(name) {
            return self.positional(name);
        }
        if self.in_fn && self.locals.contains(name) {
            return var(&local_name(name));
        }
        self.globals.insert(name.to_string());
        index(var("__g"), s(name))
    }

    /// `$1`…`$9` and `${10}`: positional parameters live in `__a`.
    fn positional(&mut self, name: &str) -> E {
        let n: i64 = name.parse().unwrap_or(0);
        self.helper("__sh_arg", vec![var("__a"), int(n)])
    }

    /// Note that `name` holds an array, so every read of it is array-aware.
    fn read_arr(&mut self, name: &str, line: usize) {
        self.arrays.insert(name.to_string());
        self.globals.insert(name.to_string());
        if self.in_fn {
            self.fn_global_refs.push((name.to_string(), line));
        }
    }

    /// The raw array value stored in `__g[name]`.
    fn arr_value(&mut self, name: &str) -> E {
        self.globals.insert(name.to_string());
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
        self.written.insert(name.to_string());
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
        if p.stages.len() == 1 {
            self.lower_cmd(&p.stages[0], out)?;
        } else {
            // `a | b | c`: each stage but the last fills `__sh_buf`, and that
            // buffer becomes the next stage's stdin. Nothing leaves the program
            // until the last stage prints, so a pipeline cannot interleave with
            // what the script already printed. The outer streams (a redirection
            // or an enclosing pipeline stage) are put back for the last stage.
            let saved_in = self.push_stdin(out);
            let (s_stream, s_buf, s_stage) = (self.tmp(), self.tmp(), self.tmp());
            out.push(decl(&s_stream, self.stream(STREAM)));
            out.push(decl(&s_buf, self.stream(BUF)));
            out.push(decl(&s_stage, self.stream(STAGE)));
            let mut carry: Option<String> = None;
            for (i, stage) in p.stages.iter().enumerate() {
                let last = i + 1 == p.stages.len();
                if let Some(prev) = carry.take() {
                    self.set_stdin(out, var(&prev), false);
                }
                if last {
                    out.push(set_index("__g", STREAM, var(&s_stream)));
                    out.push(set_index("__g", BUF, var(&s_buf)));
                } else {
                    out.push(set_index("__g", STREAM, int(1)));
                    out.push(set_index("__g", BUF, s("")));
                }
                // A pipeline stage runs in a subshell: whatever it assigns is
                // gone when it ends (`printf x | read a` leaves `a` unset). The
                // stage is lowered first so its writes are known, then wrapped
                // in a save/restore.
                out.push(set_index("__g", STAGE, int(1)));
                let mut stage_body = Vec::new();
                let outer = std::mem::take(&mut self.written);
                self.lower_cmd(stage, &mut stage_body)?;
                let writes = std::mem::replace(&mut self.written, outer);
                let mut saves = Vec::new();
                for name in &writes {
                    let slot = self.tmp();
                    out.push(decl(&slot, index(var("__g"), s(name))));
                    saves.push((slot, name.clone()));
                }
                out.extend(stage_body);
                // bash runs the last stage in a subshell too; zsh keeps it in
                // the current shell, so `printf x | read a` does set `a` there.
                if self.d == Dialect::Bash || !last {
                    for (slot, name) in saves {
                        out.push(set_index("__g", &name, var(&slot)));
                    }
                }
                out.push(set_index("__g", STAGE, var(&s_stage)));
                if !last {
                    // A stage with its own `> file` spent the buffer on the
                    // file, so the next stage reads nothing — as in the shell.
                    let buf = self.tmp();
                    out.push(decl(&buf, self.stream(BUF)));
                    out.push(set_index("__g", BUF, s("")));
                    carry = Some(buf);
                }
            }
            self.pop_stdin(out, &saved_in);
        }
        if p.negated {
            out.push(if_(st_ok(), vec![set_st(1)], Some(vec![set_st(0)])));
        }
        Ok(())
    }

    /// `cmd < in > out >> log <<EOT`: point the command's streams somewhere
    /// else for the duration of the command.
    fn lower_redirected(
        &mut self,
        inner: &Cmd,
        redirs: &[Redir],
        line: usize,
        out: &mut Vec<S>,
    ) -> Result<()> {
        // `cmd > a > b` in a shell creates `a` empty and writes `b`; here only
        // one of them would ever be touched, so it is refused rather than
        // quietly doing half of what the shell does. The same goes for two `<`.
        let outs = redirs
            .iter()
            .filter(|r| matches!(r.kind, RedirKind::Out | RedirKind::Append))
            .count();
        if outs > 1 {
            return Err(self.err(
                "two `>` / `>>` redirections on one command (only the last one's file would be written)",
                line,
            ));
        }
        if redirs.iter().filter(|r| r.kind == RedirKind::In).count() > 1 {
            return Err(self.err("two `<` redirections on one command", line));
        }
        let writes = outs == 1;
        if writes && !matches!(inner, Cmd::Simple { .. }) {
            // The buffer is written out after the command returns, so a compound
            // command that `break`s, `return`s or `exit`s would lose its output.
            return Err(self.err(
                "a `>` / `>>` on a loop, `if` or `{ }` (redirect the commands inside it instead)",
                line,
            ));
        }
        let saved_in = self.push_stdin(out);
        let (s_stream, s_buf) = (self.tmp(), self.tmp());
        if writes {
            out.push(decl(&s_stream, self.stream(STREAM)));
            out.push(decl(&s_buf, self.stream(BUF)));
        }
        for r in redirs {
            match r.kind {
                RedirKind::In => {
                    let path = self.word_one(&r.word)?;
                    let t = self.tmp();
                    out.push(decl(&t, cap("fs.read", vec![path])));
                    self.set_stdin(out, var(&t), true);
                }
                RedirKind::HereDoc => {
                    let body = if r.expand { self.word_str(&r.word)? } else { s(&r.word.raw) };
                    self.set_stdin(out, body, false);
                }
                RedirKind::Out | RedirKind::Append => {
                    let path = self.word_one(&r.word)?;
                    let t = self.tmp();
                    out.push(decl(&t, path));
                    out.push(set_index("__g", OUTF, var(&t)));
                    let append = r.kind == RedirKind::Append;
                    out.push(set_index("__g", APPEND, int(if append { 1 } else { 0 })));
                }
            }
        }
        if writes {
            out.push(set_index("__g", STREAM, int(1)));
            out.push(set_index("__g", BUF, s("")));
        }
        self.lower_cmd(inner, out)?;
        if writes {
            self.flush_buffer(out);
            out.push(set_index("__g", STREAM, var(&s_stream)));
            out.push(set_index("__g", BUF, var(&s_buf)));
        }
        self.pop_stdin(out, &saved_in);
        Ok(())
    }

    /// Write a redirected command's buffer to its file and clear it. Also used
    /// by `exit`, which must not drop what the command already printed.
    fn flush_buffer(&mut self, out: &mut Vec<S>) {
        // `>>` keeps what the file already holds: `fs.write` truncates, so the
        // old contents are read back first (a missing file is empty).
        let old = self.tmp();
        out.push(decl(
            &old,
            tern(
                bin("==", self.stream(APPEND), int(1)),
                tern(
                    bin("==", cap("fs.exists", vec![self.stream(OUTF)]), int(1)),
                    cap("fs.read", vec![self.stream(OUTF)]),
                    s(""),
                ),
                s(""),
            ),
        ));
        let text = bin("+", var(&old), self.stream(BUF));
        out.push(expr_stmt(cap("fs.write", vec![self.stream(OUTF), text])));
        out.push(set_index("__g", BUF, s("")));
    }

    /// Does this command itself (not the commands inside its body) contain a
    /// word that may glob? Only such a command needs the `failglob` wrapper; a
    /// glob deeper in an `if` or a loop is wrapped where that command is
    /// lowered.
    fn cmd_may_glob(&self, cmd: &Cmd) -> bool {
        let globbable = |w: &Word| w.raw.contains(['*', '?', '[']) || w.raw.contains('$');
        match cmd {
            Cmd::Simple { words, .. } => words.iter().any(globbable),
            Cmd::Redirected { inner, redirs, .. } => {
                self.cmd_may_glob(inner)
                    || redirs.iter().any(|r| globbable(&r.word))
            }
            Cmd::For { words: Some(ws), .. } => ws.iter().any(globbable),
            _ => false,
        }
    }

    /// One command, with an unmatched pattern under `shopt -s failglob` skipping
    /// the command (status 1) instead of ending the run.
    fn lower_cmd(&mut self, cmd: &Cmd, out: &mut Vec<S>) -> Result<()> {
        if self.d == Dialect::Bash && !self.glob_guard && self.cmd_may_glob(cmd) {
            let mut body = Vec::new();
            self.glob_guard = true;
            let lowered = self.lower_cmd_body(cmd, &mut body);
            self.glob_guard = false;
            lowered?;
            let (e, msg) = (self.tmp(), self.tmp());
            let head = self.helper(
                "__sh_substr",
                vec![
                    var(&msg),
                    int(0),
                    int(FAILGLOB_SENTINEL.len() as i64),
                    boolean(true),
                ],
            );
            out.push(S::TryCatch {
                body,
                error_var: e.clone(),
                handler: vec![
                    decl(&msg, to_str(var(&e))),
                    if_(bin("==", head, s(FAILGLOB_SENTINEL)),
                        vec![set_st(1)],
                        Some(vec![throw(var(&e))]),
                    ),
                ],
                meta: m(),
            });
            return Ok(());
        }
        self.lower_cmd_body(cmd, out)
    }

    fn lower_cmd_body(&mut self, cmd: &Cmd, out: &mut Vec<S>) -> Result<()> {
        match cmd {
            Cmd::Simple { assigns, words, line } => self.lower_simple(assigns, words, *line, out),
            Cmd::Redirected { inner, redirs, line } => {
                self.lower_redirected(inner, redirs, *line, out)
            }
            Cmd::ExtendedTest { cond, line } => {
                let e = self.cond_expr(cond, *line)?;
                out.push(if_(e, vec![set_st(0)], Some(vec![set_st(1)])));
                Ok(())
            }
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
            Cmd::Case { word, arms, .. } => {
                let subject = self.word_str(word)?;
                let t = self.tmp();
                out.push(decl(&t, subject));
                let mut tail = vec![set_st(0)];
                for (pats, body) in arms.iter().rev() {
                    let mut cond: Option<E> = None;
                    for p in pats {
                        let pat = self.pattern_of(p)?;
                        let c = self.helper("__sh_glob", vec![pat, var(&t)]);
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
                self.apply_assign(a, out)?;
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
            "read" => self.lower_read(args, line, out),
            "shopt" if self.d == Dialect::Bash => self.lower_shopt(args, line, out),
            "setopt" | "unsetopt" if self.d == Dialect::Zsh => {
                self.lower_setopt(&name, args, line, out)
            }
            "cat" | "ls" | "wc" | "head" | "tail" | "grep" | "cp" | "mv" | "rm" | "mkdir"
            | "touch" | "sort" | "uniq" | "cut" | "tr" => {
                self.lower_coreutil(&name, args, line, out)
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
                // A shell flushes the files it has open before leaving, so an
                // `exit` inside `f > log` keeps what `f` already printed.
                // A pipeline stage is a subshell, though: there `exit` leaves
                // only the stage and the script goes on. Nothing here can
                // resume a half-run stage, so that one is refused — after the
                // last stage too, where it would otherwise end the whole script
                // and silently skip what follows.
                let stage_loud = throw(concat(vec![
                    s("unsupported: `exit` inside a pipeline stage at "),
                    self.at("exit", line),
                    s(" — a shell would exit only that stage and continue"),
                ]));
                let mut flush_body = Vec::new();
                self.flush_buffer(&mut flush_body);
                out.push(if_(
                    bin("==", self.stream(STAGE), int(1)),
                    vec![stage_loud],
                    Some(vec![if_(
                        bin("==", self.stream(STREAM), int(1)),
                        vec![if_(bin("!=", self.stream(OUTF), s("")), flush_body, None)],
                        None,
                    )]),
                ));
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
                            let st = self.write_var(&n, null(), line)?;
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

    /// One `name=…` / `name+=…` / `name=(…)` / `name[i]=…` assignment.
    fn apply_assign(&mut self, a: &Assign, out: &mut Vec<S>) -> Result<()> {
        let name = a.name.as_str();
        if !is_ident(name) {
            return Err(self.err(&format!("assignment to `{name}`"), a.line));
        }
        if name == "IFS" {
            return Err(self.err("an `IFS` assignment (field splitting always uses space, tab and newline)", a.line));
        }
        if let Some(elems) = &a.array {
            self.note_array(name, a.line);
            let list = self.fields_list(elems)?;
            let v = if a.append {
                let cur = self.arr_value(name);
                bin("+", self.helper("__sh_arr_or_empty", vec![cur]), list)
            } else {
                list
            };
            out.push(self.write_var(name, v, a.line)?);
            return Ok(());
        }
        if let Some(index) = &a.index {
            self.note_array(name, a.line);
            let one = boolean(self.d == Dialect::Zsh);
            let (_, i) = self.arith(&index.raw, a.line, false)?;
            let mut v = self.word_str(&a.value)?;
            if a.append {
                let arr = self.arr_value(name);
                let cur = self.helper("__sh_arr_get", vec![arr, i.clone(), one.clone()]);
                v = bin("+", cur, v);
            }
            let arr = self.arr_value(name);
            let set = self.helper("__sh_arr_set", vec![arr, i, one, v]);
            out.push(self.write_var(name, set, a.line)?);
            return Ok(());
        }
        if self.arrays.contains(name) {
            return Err(self.err(
                &format!("`{name}` assigned both as an array and as a plain value"),
                a.line,
            ));
        }
        let mut v = self.word_str(&a.value)?;
        if a.append {
            v = bin("+", self.read_var(name, a.line), v);
        }
        out.push(self.write_var(name, v, a.line)?);
        Ok(())
    }

    /// Record that `name` holds an array.
    fn note_array(&mut self, name: &str, line: usize) {
        self.arrays.insert(name.to_string());
        self.globals.insert(name.to_string());
        if self.in_fn {
            self.fn_global_refs.push((name.to_string(), line));
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
        // Both shells read echo's options from the expanded argv, so only
        // literal leading options can be resolved here (an expansion that
        // turns out to be an option is caught by `__sh_echo_guard`).
        let mut rest = args;
        let mut newline = true;
        // zsh's echo interprets backslash escapes by default; bash's does not.
        let mut escapes = self.d == Dialect::Zsh;
        while let Some(f) = match rest.first() {
            Some(w) => self.static_lit(w)?.filter(|f| is_opt(f)),
            None => None,
        } {
            for c in f[1..].chars() {
                match c {
                    'n' => newline = false,
                    'e' => escapes = true,
                    'E' => escapes = false,
                    _ => unreachable!(),
                }
            }
            rest = &rest[1..];
        }
        let mut dynamic_first = false;
        if let Some(first) = rest.first() {
            if self.static_lit(first)?.is_none() {
                dynamic_first = true;
            }
        }
        let fields = self.fields_list(rest)?;
        let (arr, raw) = (self.tmp(), self.tmp());
        out.push(decl(&arr, fields));
        if dynamic_first {
            let g = self.helper("__sh_echo_guard", vec![var(&arr), self.at("echo", line)]);
            out.push(expr_stmt(g));
        }
        out.push(decl(&raw, call("str.join", vec![var(&arr), s(" ")])));
        if !escapes {
            self.emit(out, var(&raw), newline);
            out.push(set_st(0));
            return Ok(());
        }
        // `\c` ends the output early and suppresses the trailing newline.
        let (text, stop) = (self.tmp(), self.tmp());
        out.push(decl(&text, self.helper("__sh_esc", vec![var(&raw)])));
        out.push(decl(&stop, self.helper("__sh_esc_stop", vec![var(&raw)])));
        if newline {
            let mut with_nl = Vec::new();
            self.emit(&mut with_nl, var(&text), true);
            let mut no_nl = Vec::new();
            self.emit(&mut no_nl, var(&text), false);
            out.push(if_(not(var(&stop)), with_nl, Some(no_nl)));
        } else {
            self.emit(out, var(&text), false);
        }
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
        let pieces = printf_pieces(&fmt).map_err(|what| self.err(&what, line))?;
        let mut values = Vec::new();
        for w in &args[1..] {
            values.push(self.word_one(w)?);
        }
        let n_conv = pieces.iter().filter(|p| matches!(p, Piece::Conv(_))).count();
        let mut parts = Vec::new();
        let mut next = 0;
        loop {
            for p in &pieces {
                match p {
                    Piece::Lit(t) if t.is_empty() => {}
                    Piece::Lit(t) => parts.push(s(t)),
                    Piece::Conv(c) => {
                        let v = values.get(next).cloned();
                        next += 1;
                        parts.push(self.printf_one(c, v, line));
                    }
                }
            }
            // The format is reused while arguments remain (a trailing
            // conversion-less format prints once).
            if n_conv == 0 || next >= values.len() {
                break;
            }
        }
        self.emit(out, concat(parts), false);
        out.push(set_st(0));
        Ok(())
    }

    /// One conversion, with `None` for a missing argument.
    fn printf_one(&mut self, c: &Conv, v: Option<E>, line: usize) -> E {
        let val = match (c.conv, v) {
            (_, Some(v)) => v,
            ('s' | 'b' | 'c', None) => s(""),
            _ => s("0"),
        };
        if c.conv == 'b' {
            return self.helper("__sh_esc", vec![val]);
        }
        let at = self.at("printf", line);
        // Floats have their own formatter: they parse the argument as a
        // decimal number rather than an integer, and pad differently.
        let helper = if matches!(c.conv, 'f' | 'e' | 'E' | 'g' | 'G') { "__sh_float" } else { "__sh_fmt" };
        self.helper(
            helper,
            vec![
                val,
                s(&c.conv.to_string()),
                boolean(c.left),
                boolean(c.zero),
                boolean(c.plus),
                boolean(c.space),
                int(c.width as i64),
                int(c.prec.unwrap_or(0) as i64),
                boolean(c.prec.is_some()),
                at,
            ],
        )
    }

    /// `shopt -s nullglob` / `shopt -u nullglob` (bash), likewise `failglob`.
    /// Only these two options exist; everything else stays refused so a script
    /// that asks for an option this VM does not implement hears about it.
    fn lower_shopt(&mut self, args: &[Word], line: usize, out: &mut Vec<S>) -> Result<()> {
        let mut enable: Option<bool> = None;
        if args.is_empty() {
            return Err(self.err("`shopt` without an option", line));
        }
        for a in args {
            let Some(lit) = self.static_lit(a)? else {
                return Err(self.err("a `shopt` argument that comes from an expansion", line));
            };
            match lit.as_str() {
                "-s" => enable = Some(true),
                "-u" => enable = Some(false),
                "nullglob" | "failglob" => {
                    let Some(on) = enable else {
                        return Err(self.err(
                            &format!("`shopt {lit}` without `-s` or `-u`"),
                            line,
                        ));
                    };
                    let key = if lit == "nullglob" { NULLGLOB } else { FAILGLOB };
                    out.push(set_index("__g", key, int(if on { 1 } else { 0 })));
                }
                other => {
                    return Err(self.err(
                        &format!("`shopt {other}` (only `-s`/`-u nullglob|failglob`)"),
                        line,
                    ))
                }
            }
        }
        out.push(set_st(0));
        Ok(())
    }

    /// `setopt null_glob` / `setopt nomatch`, and their `unsetopt` forms (zsh).
    /// `nomatch` is the default in a script — an unmatched pattern ends the run
    /// with `no matches found`, the way zsh does — and `no_nomatch` turns it off
    /// so the word stays as written.
    fn lower_setopt(&mut self, name: &str, args: &[Word], line: usize, out: &mut Vec<S>) -> Result<()> {
        if args.is_empty() {
            return Err(self.err(&format!("`{name}` without an option"), line));
        }
        for a in args {
            let Some(lit) = self.static_lit(a)? else {
                return Err(self.err(&format!("a `{name}` argument that comes from an expansion"), line));
            };
            let (key, on) = match (name, lit.as_str()) {
                ("setopt", "null_glob" | "nullglob") => (NULLGLOB, true),
                ("unsetopt", "null_glob" | "nullglob") => (NULLGLOB, false),
                ("setopt", "nomatch") => (NOMATCH, true),
                ("setopt", "no_nomatch") => (NOMATCH, false),
                ("unsetopt", "nomatch") => (NOMATCH, false),
                ("unsetopt", "no_nomatch") => (NOMATCH, true),
                _ => {
                    return Err(self.err(
                        &format!("`{name} {lit}` (only `null_glob` and `nomatch`)"),
                        line,
                    ))
                }
            };
            out.push(set_index("__g", key, int(if on { 1 } else { 0 })));
        }
        out.push(set_st(0));
        Ok(())
    }

    /// `read [-r] [-s] [-p prompt] name…` over the `io.read` capability.
    ///
    /// One caveat: `io.read` reports end of input as an empty string, so an
    /// empty line is indistinguishable from EOF here. `read` therefore treats
    /// an empty line as end of input (status 1, variables empty) — `while read`
    /// loops terminate instead of spinning forever, at the cost of stopping at
    /// a blank line. `-t`, `-n`, `-d`, `-a` and reading from a file descriptor
    /// are refused.
    fn lower_read(&mut self, args: &[Word], line: usize, out: &mut Vec<S>) -> Result<()> {
        let mut rest = args;
        let mut raw = false;
        let mut prompt = None;
        while let Some(first) = rest.first() {
            match self.static_lit(first)? {
                Some(f) if f == "--" => {
                    rest = &rest[1..];
                    break;
                }
                Some(f) if f == "-p" => {
                    let Some(w) = rest.get(1) else {
                        return Err(self.err("`read -p` without a prompt", line));
                    };
                    prompt = Some(self.word_str(w)?);
                    rest = &rest[2..];
                }
                Some(f) if f.len() > 1 && f.starts_with('-') => {
                    if !f[1..].chars().all(|c| matches!(c, 'r' | 's')) {
                        return Err(self.err(&format!("`read {f}` (only -r, -s and -p are supported)"), line));
                    }
                    raw |= f[1..].contains('r');
                    rest = &rest[1..];
                }
                None => return Err(self.err("`read` with an option that comes from an expansion", line)),
                Some(_) => break,
            }
        }
        let mut names = Vec::new();
        for w in rest {
            match self.static_lit(w)? {
                Some(n) if is_ident(&n) => names.push(n),
                _ => return Err(self.err(&format!("`read {}` (only plain variable names)", w.raw), line)),
            }
        }
        if names.is_empty() {
            names.push("REPLY".to_string());
        }
        if let Some(p) = prompt {
            out.push(expr_stmt(cap("io.write", vec![p])));
        }
        let (text, fields, rest) = (self.tmp(), self.tmp(), self.tmp());
        // `while read …; done < file` and `cmd | read x`: the line comes from
        // the redirected text rather than the host's stdin.
        let (avail, isset) = (self.stream(IN), self.stream(IN_SET));
        out.push(decl(&rest, self.helper("__sh_in_rest", vec![avail.clone()])));
        out.push(decl(
            &text,
            self.helper("__sh_read_from", vec![avail, isset.clone(), boolean(raw)]),
        ));
        out.push(if_(
            bin("==", isset, int(1)),
            vec![set_index("__g", IN, var(&rest))],
            None,
        ));
        out.push(decl(&fields, self.helper("__sh_read_split", vec![var(&text), int(names.len() as i64)])));
        for (i, name) in names.iter().enumerate() {
            out.push(self.write_var(name, index(var(&fields), int(i as i64)), line)?);
        }
        out.push(if_(
            bin("==", var(&text), s("")),
            vec![set_st(1)],
            Some(vec![set_st(0)]),
        ));
        Ok(())
    }

    /// This command's whole stdin, marked consumed (a reader that read it leaves
    /// nothing for the next one, as in a shell).
    fn stdin_all(&mut self, out: &mut Vec<S>) -> String {
        let t = self.tmp();
        let (avail, isset) = (self.stream(IN), self.stream(IN_SET));
        out.push(decl(&t, self.helper("__sh_slurp", vec![avail, isset])));
        out.push(set_index("__g", IN, s("")));
        out.push(set_index("__g", IN_SET, int(0)));
        t
    }

    /// The coreutil subset the walkers run in-VM: `cat ls wc head tail grep cp
    /// mv rm mkdir touch sort uniq cut tr`. Each is a thin layer over the `fs.*`
    /// capabilities and the string helpers, so no process is spawned and the
    /// same script behaves the same way wherever crush runs it. `>`/`>>`/`|`
    /// work with them because their output goes through `emit`.
    fn lower_coreutil(
        &mut self,
        name: &str,
        args: &[Word],
        line: usize,
        out: &mut Vec<S>,
    ) -> Result<()> {
        // `cut` and `tr` take their operands next to the option (`-d :`,
        // `-f 2`, `SET1 SET2`), so they read the argv themselves.
        match name {
            "cut" => return self.lower_cut(args, line, out),
            "tr" => return self.lower_tr(args, line, out),
            _ => {}
        }
        let mut flags: Vec<String> = Vec::new();
        let mut ops: Vec<&Word> = Vec::new();
        for w in args {
            match self.static_lit(w)? {
                Some(lit) if lit.len() > 1 && lit.starts_with('-') => flags.push(lit),
                _ => ops.push(w),
            }
        }
        let no_flags = |flags: &[String]| !flags.is_empty();
        match name {
            "cat" => {
                if no_flags(&flags) {
                    return Err(self.err(&format!("`cat {}` (no options)", flags.join(" ")), line));
                }
                let text = if ops.is_empty() {
                    var(&self.stdin_all(out))
                } else {
                    let mut parts = Vec::new();
                    for f in &ops {
                        parts.push(cap("fs.read", vec![self.word_one(f)?]));
                    }
                    concat(parts)
                };
                self.emit(out, text, false);
            }
            "ls" => {
                if no_flags(&flags) {
                    return Err(self.err(&format!("`ls {}` (no options)", flags.join(" ")), line));
                }
                if ops.len() > 1 {
                    return Err(self.err("`ls` with more than one directory", line));
                }
                let dir = match ops.first() {
                    Some(w) => self.word_one(w)?,
                    None => s("."),
                };
                // `ls` hides dotfiles, sorts, and prints one name per line
                // when its output is not a terminal — which is always, here.
                // (`fs.list` returns directory order, `ls` never does.)
                let names = self.helper("__sh_nodot", vec![cap("fs.list", vec![dir])]);
                let sorted = self.helper("__sh_sort", vec![names]);
                let text = self.helper("__sh_lines_out", vec![sorted]);
                self.emit(out, text, false);
            }
            "wc" => {
                let (mut lines, mut words, mut bytes, mut chars) = (false, false, false, false);
                for f in &flags {
                    for c in f[1..].chars() {
                        match c {
                            'l' => lines = true,
                            'w' => words = true,
                            'c' => bytes = true,
                            'm' => chars = true,
                            other => return Err(self.err(&format!("`wc -{other}` (only -l, -w, -c, -m)"), line)),
                        }
                    }
                }
                if !ops.is_empty() {
                    // The multi-file column widths are a GNU `wc` detail; stdin
                    // (`wc -l < file`, `cat f | wc -l`) is the exact path here.
                    return Err(self.err("`wc` with a file name (use `wc -l < file` or a pipeline)", line));
                }
                if !(lines || words || bytes || chars) {
                    lines = true;
                    words = true;
                    bytes = true;
                }
                let text = var(&self.stdin_all(out));
                let mut cols: Vec<E> = Vec::new();
                if lines {
                    cols.push(self.helper("__sh_count", vec![text.clone(), s("\n")]));
                }
                if words {
                    let fields = self.helper("__sh_split", vec![text.clone(), self.at("wc", line)]);
                    cols.push(to_str(call("len", vec![fields])));
                }
                if bytes || chars {
                    cols.push(to_str(call("len", vec![text.clone()])));
                }
                let n = cols.len();
                // GNU `wc` pads several columns to 7 when it reads a pipe, and
                // older coreutils will also stat a `< file` stdin; newer ones
                // pad by the file's digit count instead. The width therefore
                // depends on the coreutils version, so a multi-column `wc` on a
                // redirected file is refused rather than padded the wrong way.
                // One column (the common `wc -l < f`) needs no padding at all.
                if n > 1 {
                    let at = self.at("wc", line);
                    out.push(if_(
                        bin("==", self.stream(IN_FILE), int(1)),
                        vec![throw(concat(vec![
                            s("unsupported: `wc` with several columns on a `< file` at "),
                            at,
                            s(
                                " — its column width depends on the coreutils version (use `wc -l < f`, or a pipeline)",
                            ),
                        ]))],
                        None,
                    ));
                }
                let mut parts = Vec::new();
                for (i, col) in cols.into_iter().enumerate() {
                    if i > 0 {
                        parts.push(s(" "));
                    }
                    let txt = to_str(col);
                    parts.push(if n > 1 {
                        self.helper("__sh_pad", vec![txt, int(7)])
                    } else {
                        txt
                    });
                }
                let joined = concat(parts);
                self.emit(out, joined, true);
            }
            "head" | "tail" => {
                let mut count: Option<E> = None;
                let mut rest: Vec<&Word> = ops.clone();
                for f in &flags {
                    // `-n N` and the `-nN` spelling; anything else is refused.
                    let digits = f.trim_start_matches("-n");
                    if f != "-n" && !f.starts_with("-n") {
                        return Err(self.err(&format!("`{name} {f}` (only -n N)"), line));
                    }
                    if f != "-n" {
                        match digits.parse::<i64>() {
                            Ok(v) => count = Some(int(v)),
                            Err(_) => return Err(self.err(&format!("`{name} {f}` (a numeric count)"), line)),
                        }
                        continue;
                    }
                    let Some(first) = rest.first().copied() else {
                        return Err(self.err(&format!("`{name} -n` without a count"), line));
                    };
                    // A literal count is checked here; a variable one is
                    // converted at run time, so `head -n $n` works and a
                    // non-numeric value is the loud error (not a count of 0).
                    let lit = self.static_lit(first)?.filter(|l| l.chars().all(|c| c.is_ascii_digit()));
                    count = Some(match lit {
                        Some(lit) => int(lit.parse::<i64>().unwrap_or(10)),
                        None => {
                            let word_expr = self.word_one(first)?;
                            let at = self.at(name, line);
                            self.helper("__sh_int", vec![word_expr, at, boolean(true)])
                        }
                    });
                    rest.remove(0);
                }
                if rest.len() > 1 {
                    return Err(self.err(&format!("`{name}` with more than one file"), line));
                }
                let text = match rest.first() {
                    Some(w) => cap("fs.read", vec![self.word_one(w)?]),
                    None => var(&self.stdin_all(out)),
                };
                let picked = self.helper("__sh_lines", vec![text]);
                let take = if name == "head" { "__sh_take" } else { "__sh_last" };
                let picked = self.helper(take, vec![picked, count.unwrap_or_else(|| int(10))]);
                let text = self.helper("__sh_lines_out", vec![picked]);
                self.emit(out, text, false);
            }
            "grep" => {
                let (mut count, mut invert) = (false, false);
                for f in &flags {
                    for c in f[1..].chars() {
                        match c {
                            'c' => count = true,
                            'v' => invert = true,
                            other => return Err(self.err(&format!("`grep -{other}` (only -c and -v)"), line)),
                        }
                    }
                }
                let Some(pat_word) = ops.first() else {
                    return Err(self.err("`grep` without a pattern", line));
                };
                if ops.len() > 2 {
                    return Err(self.err("`grep` with more than one file", line));
                }
                let pat = self.word_one(pat_word)?;
                let at = self.at("grep", line);
                // A BRE would need a regex engine; matching `.`/`*` literally
                // would silently disagree with the shell.
                out.push(if_(
                    self.helper("__sh_re_meta", vec![pat.clone()]),
                    vec![throw(concat(vec![
                        s("unsupported: a `grep` pattern with regular-expression syntax (`"),
                        pat.clone(),
                        s("`) at "),
                        at,
                        s(" — only literal text is matched"),
                    ]))],
                    None,
                ));
                let text = match ops.get(1) {
                    Some(w) => cap("fs.read", vec![self.word_one(w)?]),
                    None => var(&self.stdin_all(out)),
                };
                let lines = self.helper("__sh_lines", vec![text]);
                let matched = self.helper("__sh_grep_lines", vec![lines, pat, boolean(invert)]);
                if count {
                    let n = to_str(call("len", vec![matched.clone()]));
                    self.emit(out, n, true);
                } else {
                    let text = self.helper("__sh_lines_out", vec![matched.clone()]);
                    self.emit(out, text, false);
                }
                // `grep`'s status is what `&&` and `if` read.
                out.push(if_(
                    bin(">", call("len", vec![matched.clone()]), int(0)),
                    vec![set_st(0)],
                    Some(vec![set_st(1)]),
                ));
                return Ok(());
            }
            "cp" | "mv" => {
                if no_flags(&flags) {
                    return Err(self.err(&format!("`{name} {}` (no options)", flags.join(" ")), line));
                }
                if ops.len() != 2 {
                    return Err(self.err(&format!("`{name}` without exactly two paths"), line));
                }
                let (a, b) = (self.word_one(ops[0])?, self.word_one(ops[1])?);
                let c = if name == "cp" { "fs.cp" } else { "fs.mv" };
                out.push(expr_stmt(cap(c, vec![a, b])));
            }
            "rm" => {
                let mut recursive = false;
                for f in &flags {
                    for c in f[1..].chars() {
                        match c {
                            'r' | 'R' => recursive = true,
                            // `-f` only suppresses errors, and ours are loud.
                            'f' => {}
                            other => return Err(self.err(&format!("`rm -{other}` (only -r and -f)"), line)),
                        }
                    }
                }
                if ops.is_empty() {
                    return Err(self.err("`rm` without a path", line));
                }
                for w in &ops {
                    let path = self.word_one(w)?;
                    let args = if recursive { vec![path, boolean(true)] } else { vec![path] };
                    out.push(expr_stmt(cap("fs.rm", args)));
                }
            }
            "mkdir" => {
                let mut parents = false;
                for f in &flags {
                    for c in f[1..].chars() {
                        match c {
                            'p' => parents = true,
                            other => return Err(self.err(&format!("`mkdir -{other}` (only -p)"), line)),
                        }
                    }
                }
                if ops.is_empty() {
                    return Err(self.err("`mkdir` without a path", line));
                }
                for w in &ops {
                    let path = self.word_one(w)?;
                    let args = if parents { vec![path, boolean(true)] } else { vec![path] };
                    out.push(expr_stmt(cap("fs.mkdir", args)));
                }
            }
            "touch" => {
                if no_flags(&flags) {
                    return Err(self.err(&format!("`touch {}` (no options)", flags.join(" ")), line));
                }
                if ops.is_empty() {
                    return Err(self.err("`touch` without a path", line));
                }
                for w in &ops {
                    let path = self.word_one(w)?;
                    out.push(expr_stmt(cap("fs.touch", vec![path])));
                }
            }
            "sort" => {
                let (mut reverse, mut unique) = (false, false);
                for f in &flags {
                    for c in f[1..].chars() {
                        match c {
                            'r' => reverse = true,
                            'u' => unique = true,
                            // `-n` compares numeric prefixes with GNU's rules for
                            // blanks, signs and decimals; matching it only for
                            // plain integers would silently order a mixed input
                            // differently.
                            other => return Err(self.err(&format!("`sort -{other}` (only -r and -u)"), line)),
                        }
                    }
                }
                if !ops.is_empty() {
                    return Err(self.err("`sort` with a file name (use `< file` or a pipeline)", line));
                }
                let text = var(&self.stdin_all(out));
                let lines = self.helper("__sh_lines", vec![text]);
                let sorted = self.helper(
                    "__sh_sort_lines",
                    vec![lines, boolean(reverse), boolean(unique)],
                );
                let text = self.helper("__sh_lines_out", vec![sorted]);
                self.emit(out, text, false);
            }
            "uniq" => {
                let mut mode = 0i64;
                for f in &flags {
                    for c in f[1..].chars() {
                        let m = match c {
                            'c' => 1,
                            'd' => 2,
                            'u' => 3,
                            other => return Err(self.err(&format!("`uniq -{other}` (only -c, -d, -u)"), line)),
                        };
                        if mode != 0 && mode != m {
                            return Err(self.err("`uniq` with two of -c, -d, -u", line));
                        }
                        mode = m;
                    }
                }
                if !ops.is_empty() {
                    return Err(self.err("`uniq` with a file name (use `< file` or a pipeline)", line));
                }
                let text = var(&self.stdin_all(out));
                let lines = self.helper("__sh_lines", vec![text]);
                let uniq = self.helper("__sh_uniq", vec![lines, int(mode)]);
                let text = self.helper("__sh_lines_out", vec![uniq]);
                self.emit(out, text, false);
            }
            other => return Err(self.err(&format!("`{other}`"), line)),
        }
        out.push(set_st(0));
        Ok(())
    }

    /// `cut -d D -f N` over stdin. One delimiter character and one field
    /// number: GNU's field lists (`-f 1,3`, `-f 2-4`) select several columns,
    /// which is a different output shape and is refused.
    fn lower_cut(&mut self, args: &[Word], line: usize, out: &mut Vec<S>) -> Result<()> {
        let (mut delim, mut field): (Option<String>, Option<i64>) = (None, None);
        let mut i = 0;
        while i < args.len() {
            let lit = self.static_lit(&args[i])?;
            match lit.as_deref() {
                Some("-d") => {
                    let Some(w) = args.get(i + 1) else {
                        return Err(self.err("`cut -d` without a delimiter", line));
                    };
                    let Some(d) = self.static_lit(w)? else {
                        return Err(self.err("a `cut` delimiter that comes from an expansion", line));
                    };
                    if d.chars().count() != 1 {
                        return Err(self.err("`cut -d` with a delimiter that is not one character", line));
                    }
                    delim = Some(d);
                    i += 2;
                }
                Some("-f") => {
                    let Some(w) = args.get(i + 1) else {
                        return Err(self.err("`cut -f` without a field number", line));
                    };
                    let Some(n) = self.static_lit(w)? else {
                        return Err(self.err("a `cut` field number that comes from an expansion", line));
                    };
                    match n.parse::<i64>() {
                        Ok(v) if v >= 1 => field = Some(v),
                        _ => {
                            return Err(self.err(
                                &format!("`cut -f {n}` (one field number, no lists or ranges)"),
                                line,
                            ))
                        }
                    }
                    i += 2;
                }
                // The attached spellings `-d:` and `-f2`, which brush hands
                // over as one word.
                Some(lit) if lit.starts_with("-d") => {
                    let d = &lit[2..];
                    if d.chars().count() != 1 {
                        return Err(self.err("`cut -d` with a delimiter that is not one character", line));
                    }
                    delim = Some(d.to_string());
                    i += 1;
                }
                Some(lit) if lit.starts_with("-f") => {
                    let n = &lit[2..];
                    match n.parse::<i64>() {
                        Ok(v) if v >= 1 => field = Some(v),
                        _ => {
                            return Err(self.err(
                                &format!("`cut -f {n}` (one field number, no lists or ranges)"),
                                line,
                            ))
                        }
                    }
                    i += 1;
                }
                Some(other) => {
                    return Err(self.err(&format!("`cut {other}` (only -d and -f)"), line))
                }
                None => return Err(self.err("a `cut` argument that comes from an expansion", line)),
            }
        }
        let Some(field) = field else {
            return Err(self.err("`cut` without -f", line));
        };
        let delim = delim.unwrap_or_else(|| "\t".to_string());
        let text = var(&self.stdin_all(out));
        let lines = self.helper("__sh_lines", vec![text]);
        let cut = self.helper("__sh_cut", vec![lines, s(&delim), int(field)]);
        let text = self.helper("__sh_lines_out", vec![cut]);
        self.emit(out, text, false);
        Ok(())
    }

    /// `tr SET1 SET2` and `tr -d SET`. Ranges (`a-z`, `0-9`) expand; POSIX
    /// classes (`[:alpha:]`) and backslash escapes mean something else, so a set
    /// containing either is refused rather than translated literally.
    fn lower_tr(&mut self, args: &[Word], line: usize, out: &mut Vec<S>) -> Result<()> {
        let mut delete = false;
        let mut sets: Vec<String> = Vec::new();
        for w in args {
            match self.static_lit(w)? {
                Some(lit) if lit == "-d" => delete = true,
                Some(lit) if lit.len() > 1 && lit.starts_with('-') => {
                    return Err(self.err(&format!("`tr {lit}` (only -d)"), line))
                }
                Some(lit) => {
                    if lit.contains("[:") || lit.contains("\\") {
                        return Err(self.err(
                            "a `tr` set with a character class or an escape (only literal characters and ranges)",
                            line,
                        ));
                    }
                    sets.push(lit);
                }
                None => return Err(self.err("a `tr` set that comes from an expansion", line)),
            }
        }
        let text = var(&self.stdin_all(out));
        let result = if delete {
            if sets.len() != 1 {
                return Err(self.err("`tr -d` without exactly one set", line));
            }
            let set = self.helper("__sh_trset", vec![s(&sets[0])]);
            self.helper("__sh_tr_del", vec![text, set])
        } else {
            if sets.len() != 2 || sets[1].is_empty() {
                return Err(self.err("`tr` without two non-empty sets", line));
            }
            let from = self.helper("__sh_trset", vec![s(&sets[0])]);
            let to = self.helper("__sh_trset", vec![s(&sets[1])]);
            self.helper("__sh_tr", vec![text, from, to])
        };
        self.emit(out, result, false);
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

    // ── [[ … ]] ──

    fn cond_expr(&mut self, c: &Cond, line: usize) -> Result<E> {
        Ok(match c {
            Cond::Not(x) => not(self.cond_expr(x, line)?),
            Cond::And(a, b) => bin("and", self.cond_expr(a, line)?, self.cond_expr(b, line)?),
            Cond::Or(a, b) => bin("or", self.cond_expr(a, line)?, self.cond_expr(b, line)?),
            Cond::Unary("-z", w) => bin("==", self.word_str(w)?, s("")),
            Cond::Unary("-n", w) => bin("!=", self.word_str(w)?, s("")),
            Cond::Unary(op, w) if matches!(*op, "-e" | "-f" | "-d") => {
                let path = self.word_str(w)?;
                self.file_test(op, path).expect("a file test")
            }
            Cond::Unary(op, _) => return Err(self.err(&format!("`{op}` in `[[ … ]]` (file and shell-option tests)"), line)),
            Cond::Binary(op, l, r) => match *op {
                "==" | "=" | "!=" => {
                    let pat = self.pattern_of(r)?;
                    let l = self.word_str(l)?;
                    let m = self.helper("__sh_glob", vec![pat, l]);
                    if *op == "!=" { not(m) } else { m }
                }
                "<" | ">" => {
                    let (l, r) = (self.word_str(l)?, self.word_str(r)?);
                    let cmp = self.helper("__sh_strcmp", vec![l, r]);
                    let zero = int(0);
                    if *op == "<" { bin("<", cmp, zero) } else { bin(">", cmp, zero) }
                }
                op => {
                    let cmp = match TEST_INT_OPS.iter().find(|(o, _)| *o == op) {
                        Some((_, cmp)) => *cmp,
                        None => return Err(self.err(&format!("`{op}` in `[[ … ]]`"), line)),
                    };
                    let at = self.at(&format!("`[[ … {op} … ]]`"), line);
                    let (lv, rv) = (self.word_str(l)?, self.word_str(r)?);
                    let li = self.helper("__sh_int", vec![lv, at.clone(), boolean(true)]);
                    let ri = self.helper("__sh_int", vec![rv, at, boolean(true)]);
                    bin(cmp, li, ri)
                }
            },
        })
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
                    Some(op) if matches!(op, "-e" | "-f" | "-d") => {
                        let op = op.to_string();
                        Ok(self.file_test(&op, a[1].e.clone()).expect("a file test"))
                    }
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

    /// `-e`, `-f`, `-d` over the filesystem sandbox. `-f` is "exists and is not
    /// a directory", because the only two metadata answers the SDK has are
    /// `fs.exists` (follows symlinks) and `fs.is_dir`. `None` for every other
    /// unary operator, which the callers refuse by name.
    fn file_test(&mut self, op: &str, path: E) -> Option<E> {
        let exists = || cap("fs.exists", vec![path.clone()]);
        Some(match op {
            "-e" => bin("==", exists(), int(1)),
            "-d" => bin("==", cap("fs.is_dir", vec![path]), int(1)),
            "-f" => bin(
                "and",
                bin("==", exists(), int(1)),
                bin("==", cap("fs.is_dir", vec![path]), int(0)),
            ),
            _ => return None,
        })
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
                    "**" => {
                        let at = self.at("`**`", line);
                        self.helper("__sh_pow", vec![l, r, at])
                    }
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

    /// A numeric literal in arithmetic: `123`, `0x1f` / `0XFF`, `010` (octal,
    /// like bash), and `base#digits` with base 2..=64. Above base 36 the digits
    /// continue `A-Z`, `@`, `_` as bash defines them.
    fn arith_number(&self, t: &str, line: usize) -> Result<i64> {
        let bad = |why: &str| self.err(&format!("number `{t}` in arithmetic ({why})"), line);
        if let Some((base, digits)) = t.split_once('#') {
            let base: u32 = base.parse().map_err(|_| bad("invalid base"))?;
            // bash takes any base up to 64; zsh stops at 36.
            let max = if self.d == Dialect::Zsh { 36 } else { 64 };
            if !(2..=max).contains(&base) {
                return Err(bad(&format!("base out of the 2..{max} range for {}", self.d.name())));
            }
            if digits.is_empty() {
                return Err(bad("no digits"));
            }
            let mut v: i64 = 0;
            for c in digits.chars() {
                let d = digit_value(c).ok_or_else(|| bad("invalid digit"))?;
                if d >= base {
                    return Err(bad("digit out of range for the base"));
                }
                v = v.checked_mul(base as i64).and_then(|v| v.checked_add(d as i64)).ok_or_else(|| bad("too large"))?;
            }
            return Ok(v);
        }
        if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            if h.is_empty() {
                return Err(bad("no digits after `0x`"));
            }
            return i64::from_str_radix(h, 16).map_err(|_| bad("hex digits, or too large"));
        }
        // bash reads a leading zero as octal; zsh reads the same literal as
        // decimal (`$((010))` is 8 in bash and 10 in zsh).
        if t.len() > 1 && t.starts_with('0') && self.d == Dialect::Bash {
            return i64::from_str_radix(&t[1..], 8).map_err(|_| bad("octal digits (write 10#… for decimal)"));
        }
        t.parse().map_err(|_| bad("decimal digits, or too large"))
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
                while i < cs.len() && (cs[i].is_ascii_alphanumeric() || cs[i] == '#' || cs[i] == '@' || cs[i] == '_') {
                    i += 1;
                }
                let t: String = cs[st..i].iter().collect();
                toks.push(Tok::Num(self.arith_number(&t, line)?));
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

    /// Parse a word into quoted/unquoted literals and expansions. A word's
    /// parts are the same whether it is later used as a pattern or expanded as
    /// a pathname — what a `*` *means* is decided by the caller, which escapes
    /// quoted text and hands the result to `__sh_glob` or `__sh_globexp`.
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
                // An unquoted glob character stays in the word, and `fields`
                // expands the word against the filesystem afterwards (a `case`
                // pattern is not a pathname expansion, so it stays literal).
                '*' | '?' | '[' => {
                    push_lit(&mut segs, c, false);
                    i += 1;
                }
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
                let Some(end) = find_brace_end(cs, i + 1) else {
                    return Err(self.err("an unterminated `${`", line));
                };
                let inner: String = cs[i + 2..end].iter().collect();
                segs.push(self.param_seg(&inner, line, quoted)?);
                Ok(end + 1)
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

    /// One `${…}` expansion, from its contents between the braces.
    fn param_seg(&self, inner: &str, line: usize, quoted: bool) -> Result<Seg> {
        let w = |t: &str| Word { raw: t.to_string(), line };
        let bad = || self.err(&format!("parameter expansion `${{{inner}}}`"), line);
        // ${#…}: counts and lengths.
        if let Some(rest) = inner.strip_prefix('#') {
            if rest.is_empty() || rest == "@" || rest == "*" {
                return Ok(Seg::Count(quoted));
            }
            if let Some((name, sub)) = split_subscript(rest).filter(|(n, _)| is_ident(n)) {
                if sub == "@" || sub == "*" {
                    return Ok(Seg::ArrCount { name: name.to_string(), quoted });
                }
                return Ok(Seg::ArrElem { name: name.to_string(), index: w(sub), len: true, quoted });
            }
            if is_ident(rest) {
                return Ok(Seg::Len(rest.to_string(), quoted));
            }
            return Err(bad());
        }
        let cs: Vec<char> = inner.chars().collect();
        if matches!(cs.first(), Some('@' | '*' | '?')) && cs.len() == 1 {
            return Ok(match cs[0] {
                '?' => Seg::Status(quoted),
                c => Seg::All { star: c == '*', quoted },
            });
        }
        if !cs.is_empty() && cs.iter().all(|c| c.is_ascii_digit()) && cs.len() > 1 || (cs.len() == 1 && cs[0].is_ascii_digit() && cs[0] != '0') {
            let n: usize = inner.parse().map_err(|_| self.err("a huge positional parameter", line))?;
            return Ok(Seg::Pos(n, quoted));
        }
        // `${name…}`, including positional parameters (`${1:-x}`, `${10}`).
        let mut k = 0;
        while k < cs.len() && (cs[k].is_ascii_alphanumeric() || cs[k] == '_') {
            k += 1;
        }
        let name: String = cs[..k].iter().collect();
        if !is_ident(&name) && !is_positional(&name) {
            return Err(bad());
        }
        // `${1-y}` cannot tell unset from empty here, so only the `:` forms
        // of a positional default are lowered.
        if is_positional(&name) && rest_matches_no_colon_op(&cs[k..]) {
            return Err(self.err(&format!("`${{{name}-…}}` (use `${{{name}:-…}}` for a positional parameter)"), line));
        }
        let rest: String = cs[k..].iter().collect();
        if rest.is_empty() {
            return Ok(Seg::Var(name, quoted));
        }
        if let Some((_, sub)) = split_subscript(&rest).filter(|_| rest.starts_with('[')) {
            if sub == "@" || sub == "*" {
                return Ok(Seg::ArrAll { name, star: sub == "*", quoted });
            }
            return Ok(Seg::ArrElem { name, index: w(sub), len: false, quoted });
        }
        // ${name:-word} and friends, then ${name:off:len}.
        if let Some(r) = rest.strip_prefix(':') {
            let mut chars = r.chars();
            if let Some(op) = chars.next().filter(|c| matches!(c, '-' | '=' | '+' | '?')) {
                let op: &'static str = match op {
                    '-' => ":-",
                    '=' => ":=",
                    '+' => ":+",
                    _ => ":?",
                };
                return Ok(Seg::Default { name, op, word: w(chars.as_str()), quoted });
            }
            let (off, len) = split_top_colon(r);
            return Ok(Seg::Substr { name, off: w(off), len: len.map(w), quoted });
        }
        for (prefix, op) in [("-", "-"), ("=", "="), ("+", "+"), ("?", "?")] {
            if let Some(tail) = rest.strip_prefix(prefix) {
                return Ok(Seg::Default { name, op, word: w(tail), quoted });
            }
        }
        for (prefix, op) in [("##", "##"), ("#", "#"), ("%%", "%%"), ("%", "%")] {
            if let Some(tail) = rest.strip_prefix(prefix) {
                return Ok(Seg::Trim { name, op, pat: w(tail), quoted });
            }
        }
        if let Some(tail) = rest.strip_prefix("//").or_else(|| rest.strip_prefix('/')) {
            let all = rest.starts_with("//");
            // `/#pat` and `/%pat` match only at the start / end of the value.
            let (anchor, tail) = match tail.strip_prefix('#') {
                Some(t) => (1, t),
                None => match tail.strip_prefix('%') {
                    Some(t) => (2, t),
                    None => (0, tail),
                },
            };
            let (pat, rep) = split_top_slash(tail);
            return Ok(Seg::Subst { name, all, anchor, pat: w(&pat), rep: w(&rep), quoted });
        }
        match rest.as_str() {
            "^^" => Ok(Seg::Case { name, upper: true, quoted }),
            ",," => Ok(Seg::Case { name, upper: false, quoted }),
            _ => Err(bad()),
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
                if self.arrays.contains(n) && self.d == Dialect::Zsh {
                    // zsh's `${#a}` counts elements; bash's is the length of `$a`.
                    self.read_arr(n, line);
                    let one = boolean(true);
                    to_str(self.helper("__sh_arr_len", vec![index(var("__g"), s(n)), one]))
                } else {
                    let v = self.read_var(n, line);
                    to_str(len(chars(v)))
                }
            }
            Seg::Arith(text, _) => {
                let (pre, v) = self.arith(text, line, false)?;
                debug_assert!(pre.is_empty());
                to_str(v)
            }
            Seg::Default { name, op, word: w, .. } => {
                let colon = op.starts_with(':');
                let raw = self.read_var_raw(name);
                let cur = self.stringify(name, raw.clone());
                // With a colon, empty and unset are the same; without one only
                // a variable that was never set (or was `unset`) triggers it.
                let has = if colon { bin("!=", cur.clone(), s("")) } else { bin("!=", raw, null()) };
                match *op {
                    ":-" | "-" => {
                        let dflt = self.word_str(w)?;
                        tern(has, cur, dflt)
                    }
                    ":+" | "+" => {
                        let alt = self.word_str(w)?;
                        tern(has, alt, s(""))
                    }
                    ":=" | "=" => {
                        if is_positional(name) {
                            return Err(self.err(&format!("assignment to positional parameter `${{{name}}}" ), line));
                        }
                        if self.arrays.contains(name) || self.locals.contains(name) {
                            return Err(self.err(&format!("`${{{name}{op}…}}` on a local or array variable"), line));
                        }
                        let dflt = self.word_str(w)?;
                        self.helper("__sh_assign_default", vec![var("__g"), s(name), dflt, has])
                    }
                    _ => {
                        let msg = self.word_str(w)?;
                        self.helper("__sh_err_if_unset", vec![has, s(name), msg, cur])
                    }
                }
            }
            Seg::Trim { name, op, pat, quoted: _ } => {
                let v = self.read_var(name, line);
                let pat = self.pattern_of(pat)?;
                let longest = boolean(op.len() == 2);
                let prefix = boolean(op.starts_with('#'));
                self.helper("__sh_trim", vec![v, pat, longest, prefix])
            }
            Seg::Subst { name, all, anchor, pat, rep, .. } => {
                let v = self.read_var(name, line);
                let pat = self.pattern_of(pat)?;
                let at = self.at("`${…/pat/rep}`", line);
                let rep = self.word_str(rep)?;
                let zsh = boolean(self.d == Dialect::Zsh);
                self.helper(
                    "__sh_replace",
                    vec![v, pat, rep, boolean(*all), int(*anchor as i64), zsh, at],
                )
            }
            Seg::Substr { name, off, len: n, .. } => {
                let v = self.read_var(name, line);
                let (pre, off) = self.arith(&off.raw, line, false)?;
                debug_assert!(pre.is_empty());
                let (len_e, has_len) = match n {
                    Some(w) => {
                        let (pre, e) = self.arith(&w.raw, line, false)?;
                        debug_assert!(pre.is_empty());
                        (e, boolean(true))
                    }
                    None => (int(0), boolean(false)),
                };
                self.helper("__sh_substr", vec![v, off, len_e, has_len])
            }
            Seg::Case { name, upper, .. } => {
                if self.d == Dialect::Zsh {
                    return Err(self.err("`${x^^}` / `${x,,}` (zsh writes `${(U)x}` / `${(L)x}`)", line));
                }
                let v = self.read_var(name, line);
                call(if *upper { "str.to_upper" } else { "str.to_lower" }, vec![v])
            }
            Seg::ArrElem { name, index, len: is_len, .. } => {
                self.read_arr(name, line);
                let arr = self.arr_value(name);
                let one = boolean(self.d == Dialect::Zsh);
                let (_, i) = self.arith(&index.raw, line, false)?;
                let e = self.helper("__sh_arr_get", vec![arr, i, one]);
                if *is_len { to_str(len(chars(e))) } else { e }
            }
            Seg::ArrCount { name, .. } => {
                self.read_arr(name, line);
                let one = boolean(self.d == Dialect::Zsh);
                to_str(self.helper("__sh_arr_len", vec![index(var("__g"), s(name)), one]))
            }
            Seg::ArrAll { name, .. } => {
                self.read_arr(name, line);
                call("str.join", vec![self.arr_value(name), s(" ")])
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

    /// The fields of a word: quote removal, field splitting, then pathname
    /// expansion of any unquoted glob character (`echo *.txt`, `for f in *`).
    ///
    /// Pathname expansion reads the filesystem through the `fs.exists` and
    /// `fs.list` capabilities, so a host that grants no filesystem gets a loud
    /// error rather than a guess. bash's rules apply: a name starting with `.`
    /// is only matched by a pattern that starts with one, the matches are
    /// sorted, and a pattern that matches nothing stays as written.
    fn fields(&mut self, w: &Word) -> Result<Fields> {
        let mut segs = self.parse_word(w)?;
        let lit_glob = segs
            .iter()
            .any(|sg| matches!(sg, Seg::Lit(t, false) if t.contains(['*', '?', '['])));
        // bash also globs the text an expansion contributes (`x='*'; echo $x`);
        // zsh needs `GLOB_SUBST` for that, so it expands only what was written.
        let exp_glob = self.d == Dialect::Bash
            && segs.iter().any(|sg| sg.is_expansion() && !sg.quoted() && !sg.is_numeric());
        if !lit_glob && !exp_glob {
            return self.fields_plain(&segs, w);
        }
        // Quoted text inside a glob word must not act as a pattern: escape its
        // metacharacters here, and `__sh_globexp` drops the escapes again for a
        // pattern that matches nothing (`echo a"*"b*` prints `a*b*`).
        if segs.iter().any(|sg| sg.is_expansion() && sg.quoted() && !sg.is_numeric()) {
            return Err(self.err(
                &format!(
                    "a quoted expansion mixed with an unquoted glob (`{}`; quote the whole word)",
                    w.raw
                ),
                w.line,
            ));
        }
        for sg in segs.iter_mut() {
            if let Seg::Lit(t, true) = sg {
                *t = escape_pattern(t);
            }
        }
        let at = self.at(&format!("`{}`", w.raw), w.line);
        let arr = match self.fields_plain(&segs, w)? {
            Fields::One(e) => array(vec![e]),
            Fields::Many(a) => a,
        };
        // What a pattern matching nothing does (literal word, no word, or an
        // error) depends on `nullglob`/`failglob`/`nomatch`, read at runtime
        // because `shopt`/`setopt` can turn them on anywhere in the script. The
        // helpers get them as arguments: `__g` is main's, not theirs.
        let opts = vec![
            self.stream(NULLGLOB),
            self.stream(FAILGLOB),
            self.stream(NOMATCH),
        ];
        Ok(Fields::Many(self.helper("__sh_globmap", vec![arr, at].into_iter().chain(opts).collect())))
    }

    /// The fields of a word before pathname expansion.
    fn fields_plain(&mut self, segs: &[Seg], w: &Word) -> Result<Fields> {
        let line = w.line;
        let real: Vec<&Seg> = segs.iter().filter(|sg| !matches!(sg, Seg::Lit(t, _) if t.is_empty())).collect();
        if let [Seg::ArrAll { name, star: false, quoted }] = real.as_slice() {
            let (name, quoted) = (name.clone(), *quoted);
            let arr = self.arr_value(&name);
            return Ok(if quoted {
                Fields::Many(arr)
            } else {
                let at = self.at(&format!("unquoted `${{{name}[@]}}`"), line);
                Fields::Many(match self.d {
                    Dialect::Bash => self.helper("__sh_split_arr", vec![arr, at]),
                    Dialect::Zsh => self.helper("__sh_drop_empty", vec![arr]),
                })
            });
        }
        // A quoted `"${a[@]}"` mixed with other text is just the joined array
        // (both shells print `"[${a[@]}]"` as `[one two]`); unquoted it would
        // need bash's prefix-on-first / suffix-on-last rule, so refuse.
        if segs.iter().any(|sg| matches!(sg, Seg::ArrAll { star: false, quoted: false, .. })) {
            let name = segs.iter().find_map(|s| s.var_name()).unwrap_or("?");
            return Err(self.err(
                &format!("`${{{name}[@]}}` combined with other text in one word (`{}`)", w.raw),
                line,
            ));
        }
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
            return Ok(Fields::One(self.concat_segs(segs, line)?));
        }
        match self.d {
            Dialect::Bash => {
                if segs.iter().any(|sg| sg.quoted()) {
                    return Err(self.err(
                        &format!("a word mixing quoted text with an unquoted expansion (`{}`; quote the whole word)", w.raw),
                        line,
                    ));
                }
                let v = self.concat_segs(segs, line)?;
                let at = self.at(&format!("unquoted `{}`", w.raw), line);
                Ok(Fields::Many(self.helper("__sh_split", vec![v, at])))
            }
            Dialect::Zsh => {
                let v = self.concat_segs(segs, line)?;
                if real.len() == 1 {
                    Ok(Fields::Many(self.helper("__sh_nonempty", vec![v])))
                } else {
                    Ok(Fields::One(v))
                }
            }
        }
    }

    /// The word as a shell pattern (`case` arms, `[[ … == … ]]`), with quoted
    /// text escaped so it stays literal. A literal unquoted word is a pattern;
    /// the text of an *expansion* is a pattern in bash but literal in zsh
    /// (`p='a*'; [[ abc == $p ]]` is true in bash, false in zsh).
    fn pattern_of(&mut self, w: &Word) -> Result<E> {
        let segs = self.parse_word(w)?;
        let mut parts: Vec<E> = Vec::new();
        for sg in &segs {
            match sg {
                Seg::Lit(t, false) => {
                    if !t.is_empty() {
                        parts.push(s(t));
                    }
                }
                Seg::Lit(t, true) => {
                    if !t.is_empty() {
                        parts.push(s(&escape_pattern(t)));
                    }
                }
                other => {
                    let e = self.seg_expr(other, w.line)?;
                    if self.d == Dialect::Bash && !other.quoted() {
                        parts.push(e);
                    } else {
                        parts.push(self.helper("__sh_quote_pat", vec![e]));
                    }
                }
            }
        }
        Ok(concat(parts))
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

/// Every runtime helper, so a helper's references to other helpers can be
/// resolved back to `&'static str` (and so a misspelt name fails here rather
/// than as an "unknown capability" at run time).
const SHELL_HELPERS: &[&str] = &[
    "__sh_arg",
    "__sh_bracket",
    "__sh_class",
    "__sh_dval",
    "__sh_glob",
    "__sh_globmap",
    "__sh_globexp",
    "__sh_meta",
    "__sh_dstar",
    "__sh_join",
    "__sh_sort",
    "__sh_dironly",
    "__sh_slurp",
    "__sh_in_line",
    "__sh_in_rest",
    "__sh_read_from",
    "__sh_unbackslash",
    "__sh_lines",
    "__sh_lines_out",
    "__sh_nodot",
    "__sh_take",
    "__sh_last",
    "__sh_find",
    "__sh_grep_lines",
    "__sh_re_meta",
    "__sh_count",
    "__sh_pad",
    "__sh_rev",
    "__sh_dedupe",
    "__sh_sort_lines",
    "__sh_uniq",
    "__sh_cut",
    "__sh_trset",
    "__sh_tr",
    "__sh_tr_del",
    "__sh_unglob",
    "__sh_quote_pat",
    "__sh_read_line",
    "__sh_read_split",
    "__sh_esc",
    "__sh_esc_simple",
    "__sh_esc_stop",
    "__sh_echo_guard",
    "__sh_int",
    "__sh_nonempty",
    "__sh_drop_empty",
    "__sh_one",
    "__sh_pow",
    "__sh_assign_default",
    "__sh_arr_get",
    "__sh_arr_or_empty",
    "__sh_arr_set",
    "__sh_arr_len",
    "__sh_arr_str",
    "__sh_base",
    "__sh_err_if_unset",
    "__sh_fmt",
    "__sh_float",
    "__sh_fparse",
    "__sh_fround",
    "__sh_finc",
    "__sh_ffix",
    "__sh_fexp",
    "__sh_fztrim",
    "__sh_repeat",
    "__sh_isdigit",
    "__sh_isws",
    "__sh_number",
    "__sh_join_range",
    "__sh_replace",
    "__sh_split",
    "__sh_split_arr",
    "__sh_str",
    "__sh_strcmp",
    "__sh_substr",
    "__sh_trim",
];

/// Helper names called anywhere in `f`'s body.
fn helper_deps(f: &Function) -> Vec<&'static str> {
    fn expr(e: &E, out: &mut Vec<&'static str>) {
        match e {
            E::Call { function, args, .. } => {
                if let Some(name) = SHELL_HELPERS.iter().copied().find(|h| h == function) {
                    out.push(name);
                }
                args.iter().for_each(|a| expr(a, out));
            }
            E::CapabilityCall { args, .. } | E::ArrayLiteral { elements: args, .. } => {
                args.iter().for_each(|a| expr(a, out));
            }
            E::BinaryOp { left, right, .. } => {
                expr(left, out);
                expr(right, out);
            }
            E::UnaryOp { operand, .. } => expr(operand, out),
            E::Index { target, index, .. } => {
                expr(target, out);
                expr(index, out);
            }
            E::ObjectLiteral { properties, .. } => properties.iter().for_each(|(_, v)| expr(v, out)),
            _ => {}
        }
    }
    fn stmt(s: &S, out: &mut Vec<&'static str>) {
        match s {
            S::VarDecl { value, .. } | S::Assign { value, .. } | S::ExprStmt { expr: value, .. } | S::Throw { value, .. } => {
                expr(value, out)
            }
            S::Return { value: Some(v), .. } => expr(v, out),
            S::If { condition, then_body, else_body, .. } => {
                expr(condition, out);
                then_body.iter().for_each(|s| stmt(s, out));
                else_body.iter().flatten().for_each(|s| stmt(s, out));
            }
            S::While { condition, body, .. } => {
                expr(condition, out);
                body.iter().for_each(|s| stmt(s, out));
            }
            S::For { iterable, body, .. } => {
                expr(iterable, out);
                body.iter().for_each(|s| stmt(s, out));
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    f.body.iter().for_each(|s| stmt(s, &mut out));
    out.sort_unstable();
    out.dedup();
    out
}

fn helper_fn(name: &str) -> Function {
    debug_assert!(SHELL_HELPERS.contains(&name), "unknown shell helper {name}");
    match name {
        // __sh_arg(a, n): "$n", or "" past the end.
        "__sh_arg" => func(
            &["a", "n"],
            vec![
                if_(bin("<=", var("n"), len(var("a"))), vec![ret(index(var("a"), bin("-", var("n"), int(1))))], None),
                ret(s("")),
            ],
        ),
        // __sh_class(name, ch): a POSIX character class inside `[…]`.
        "__sh_class" => {
            let o = || cap("conv.ord", vec![var("ch")]);
            let range = |lo: i64, hi: i64| bin("and", bin(">=", o(), int(lo)), bin("<=", o(), int(hi)));
            let alpha = bin("or", range(65, 90), range(97, 122));
            let digit = range(48, 57);
            let mut body = vec![
                if_(bin("==", var("name"), s("alpha")), vec![ret(alpha.clone())], None),
                if_(bin("==", var("name"), s("digit")), vec![ret(digit.clone())], None),
                if_(bin("==", var("name"), s("alnum")), vec![ret(bin("or", alpha, digit.clone()))], None),
                if_(bin("==", var("name"), s("upper")), vec![ret(range(65, 90))], None),
                if_(bin("==", var("name"), s("lower")), vec![ret(range(97, 122))], None),
                if_(
                    bin("==", var("name"), s("space")),
                    vec![ret(bin("or", bin("==", o(), int(32)), range(9, 13)))],
                    None,
                ),
                if_(
                    bin("==", var("name"), s("blank")),
                    vec![ret(bin("or", bin("==", o(), int(32)), bin("==", o(), int(9))))],
                    None,
                ),
                if_(
                    bin("==", var("name"), s("punct")),
                    vec![ret(bin(
                        "or",
                        bin("or", range(33, 47), range(58, 64)),
                        bin("or", range(91, 96), range(123, 126)),
                    ))],
                    None,
                ),
                if_(bin("==", var("name"), s("print")), vec![ret(range(32, 126))], None),
                if_(bin("==", var("name"), s("graph")), vec![ret(range(33, 126))], None),
                if_(
                    bin("==", var("name"), s("cntrl")),
                    vec![ret(bin("or", range(0, 31), bin("==", o(), int(127))))],
                    None,
                ),
                if_(
                    bin("==", var("name"), s("xdigit")),
                    vec![ret(bin("or", digit, bin("or", range(65, 70), range(97, 102))))],
                    None,
                ),
            ];
            body.push(ret(boolean(false)));
            func(&["name", "ch"], body)
        }
        // __sh_quote_pat(s): escape the pattern metacharacters in `s`, for a
        // quoted word (or a zsh expansion) used where a pattern is expected.
        "__sh_quote_pat" => func(
            &["s"],
            vec![
                decl("out", s("")),
                for_("c", chars(var("s")), vec![
                    if_(
                        bin(
                            "or",
                            bin("or", bin("==", var("c"), s("*")), bin("==", var("c"), s("?"))),
                            bin("or", bin("or", bin("==", var("c"), s("[")), bin("==", var("c"), s("]"))), bin("==", var("c"), s("\\"))),
                        ),
                        vec![set("out", bin("+", var("out"), s("\\")))],
                        None,
                    ),
                    set("out", bin("+", var("out"), var("c"))),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_unglob(s): drop the pattern escapes `\x` → `x` from a word, the
        // way quote removal shows text that was quoted inside a glob word.
        "__sh_unglob" => func(
            &["s"],
            vec![
                decl("cs", chars(var("s"))),
                decl("n", len(var("cs"))),
                decl("i", int(0)),
                decl("out", s("")),
                while_(bin("<", var("i"), var("n")), vec![
                    if_(
                        bin(
                            "and",
                            bin("==", index(var("cs"), var("i")), s("\\")),
                            bin("<", bin("+", var("i"), int(1)), var("n")),
                        ),
                        vec![
                            set("out", bin("+", var("out"), index(var("cs"), bin("+", var("i"), int(1))))),
                            set("i", bin("+", var("i"), int(2))),
                        ],
                        Some(vec![
                            set("out", bin("+", var("out"), index(var("cs"), var("i")))),
                            set("i", bin("+", var("i"), int(1))),
                        ]),
                    ),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_globmap(arr, at, nullglob, failglob, nomatch): pathname expansion
        // of each field.
        "__sh_globmap" => func(
            &["arr", "at", "nullglob", "failglob", "nomatch"],
            vec![
                decl("out", array(vec![])),
                for_("f", var("arr"), vec![set(
                    "out",
                    bin(
                        "+",
                        var("out"),
                        call(
                            "__sh_globexp",
                            vec![
                                var("f"),
                                var("at"),
                                var("nullglob"),
                                var("failglob"),
                                var("nomatch"),
                            ],
                        ),
                    ),
                )]),
                ret(var("out")),
            ],
        ),
        // __sh_meta(s): does `s` hold an unescaped `*`, `?` or `[`?
        "__sh_meta" => func(
            &["s"],
            vec![
                decl("cs", chars(var("s"))),
                decl("n", len(var("cs"))),
                decl("i", int(0)),
                while_(bin("<", var("i"), var("n")), vec![
                    decl("c", index(var("cs"), var("i"))),
                    if_(
                        bin("==", var("c"), s("\\")),
                        vec![set("i", bin("+", var("i"), int(2)))],
                        Some(vec![
                            if_(
                                bin(
                                    "or",
                                    bin("or", bin("==", var("c"), s("*")), bin("==", var("c"), s("?"))),
                                    bin("==", var("c"), s("[")),
                                ),
                                vec![ret(boolean(true))],
                                None,
                            ),
                            set("i", bin("+", var("i"), int(1))),
                        ]),
                    ),
                ]),
                ret(boolean(false)),
            ],
        ),
        // __sh_dstar(s): does `s` hold an unescaped `**`? bash reads it as `*`
        // without `globstar`, zsh recurses by default, so it is refused rather
        // than guessed at in either dialect.
        "__sh_dstar" => func(
            &["s"],
            vec![
                decl("cs", chars(var("s"))),
                decl("n", len(var("cs"))),
                decl("i", int(0)),
                while_(bin("<", var("i"), var("n")), vec![
                    decl("c", index(var("cs"), var("i"))),
                    if_(
                        bin("==", var("c"), s("\\")),
                        vec![set("i", bin("+", var("i"), int(2)))],
                        Some(vec![
                            if_(
                                bin(
                                    "and",
                                    bin("==", var("c"), s("*")),
                                    bin(
                                        "and",
                                        bin("<", bin("+", var("i"), int(1)), var("n")),
                                        bin("==", index(var("cs"), bin("+", var("i"), int(1))), s("*")),
                                    ),
                                ),
                                vec![ret(boolean(true))],
                                None,
                            ),
                            set("i", bin("+", var("i"), int(1))),
                        ]),
                    ),
                ]),
                ret(boolean(false)),
            ],
        ),
        // __sh_join(dir, name): `dir/name`, where "" is the working directory and
        // "/" is the root the pattern started from.
        "__sh_join" => func(
            &["dir", "name"],
            vec![
                if_(bin("==", var("dir"), s("")), vec![ret(var("name"))], None),
                if_(bin("==", var("dir"), s("/")), vec![ret(bin("+", s("/"), var("name")))], None),
                ret(bin("+", bin("+", var("dir"), s("/")), var("name"))),
            ],
        ),
        // __sh_sort(a): `a` in byte order, the order both shells print glob
        // matches in. Insertion sort, so a word matching hundreds of names
        // costs O(n²) string comparisons and can reach the step quota — loudly,
        // never by printing a different order.
        // __sh_sort(a): merge sort in byte order. It used to be an insertion
        // sort that rebuilt the array on every insertion, so a directory of a
        // few hundred matches cost O(n²) comparisons *and* copies; this is
        // O(n log n) and appends in place (`append` is the VM's native cap).
        "__sh_sort" => func(
            &["a"],
            vec![
                decl("n", len(var("a"))),
                if_(bin("<=", var("n"), int(1)), vec![ret(var("a"))], None),
                decl("mid", bin("/", var("n"), int(2))),
                decl("left", array(vec![])),
                decl("right", array(vec![])),
                decl("i", int(0)),
                while_(bin("<", var("i"), var("n")), vec![
                    if_(
                        bin("<", var("i"), var("mid")),
                        vec![expr_stmt(cap("append", vec![var("left"), index(var("a"), var("i"))]))],
                        Some(vec![expr_stmt(cap("append", vec![var("right"), index(var("a"), var("i"))]))]),
                    ),
                    set("i", bin("+", var("i"), int(1))),
                ]),
                decl("l", call("__sh_sort", vec![var("left")])),
                decl("r", call("__sh_sort", vec![var("right")])),
                decl("out", array(vec![])),
                set("i", int(0)),
                decl("j", int(0)),
                while_(
                    bin("and", bin("<", var("i"), len(var("l"))), bin("<", var("j"), len(var("r")))),
                    vec![if_(
                        bin(
                            "<=",
                            call("__sh_strcmp", vec![index(var("l"), var("i")), index(var("r"), var("j"))]),
                            int(0),
                        ),
                        vec![
                            expr_stmt(cap("append", vec![var("out"), index(var("l"), var("i"))])),
                            set("i", bin("+", var("i"), int(1))),
                        ],
                        Some(vec![
                            expr_stmt(cap("append", vec![var("out"), index(var("r"), var("j"))])),
                            set("j", bin("+", var("j"), int(1))),
                        ]),
                    )],
                ),
                while_(bin("<", var("i"), len(var("l"))), vec![
                    expr_stmt(cap("append", vec![var("out"), index(var("l"), var("i"))])),
                    set("i", bin("+", var("i"), int(1))),
                ]),
                while_(bin("<", var("j"), len(var("r"))), vec![
                    expr_stmt(cap("append", vec![var("out"), index(var("r"), var("j"))])),
                    set("j", bin("+", var("j"), int(1))),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_dironly(a): the matches that are directories, each keeping the
        // `/` the word asked for (`echo */` lists directories only).
        "__sh_dironly" => func(
            &["a"],
            vec![
                decl("out", array(vec![])),
                for_("p", var("a"), vec![
                    if_(
                        bin("==", cap("fs.is_dir", vec![var("p")]), int(1)),
                        vec![set("out", bin("+", var("out"), array(vec![bin("+", var("p"), s("/"))])))],
                        None,
                    ),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_globexp(field, at, nullglob, failglob, nomatch): the pathnames one
        // word expands to, or — when the pattern matches nothing — whatever the
        // shell's options say: the word as written (both shells' default), no
        // word at all (`nullglob`), or an error (`failglob` in bash, `nomatch`
        // in zsh).
        //
        // Every path component is matched on its own against the directories
        // built so far, so `*/*.txt`, `sub/*.log` and `a?/*/[xy]*` all work:
        //
        // - a component with no metacharacter is appended as written, but it has
        //   to exist, so `a*/missing` stays one word instead of inventing paths;
        // - a component is only listed when it is a directory (`fs.is_dir`), so
        //   a file matched by `*` does not turn into a "not a directory" error;
        // - a leading `.` in the component is what lets `.`-names match, as in
        //   both shells;
        // - a trailing `/` keeps the slash and keeps only directories;
        // - the result is sorted in byte order, and an absolute pattern is
        //   refused: the filesystem sandbox is relative to the script's
        //   directory, so `/usr/*` could not be answered honestly.
        "__sh_globexp" => func(
            &["field", "at", "nullglob", "failglob", "nomatch"],
            vec![
                // No metacharacter at all: quote removal only, and no filesystem
                // access — `echo /etc/hosts` must not need a granted capability.
                if_(
                    not(call("__sh_meta", vec![var("field")])),
                    vec![ret(array(vec![call("__sh_unglob", vec![var("field")])]))],
                    None,
                ),
                if_(
                    call("__sh_dstar", vec![var("field")]),
                    vec![throw(concat(vec![
                        s("unsupported: `**` in `"),
                        var("field"),
                        s("` at "),
                        var("at"),
                        s("` — `**` is `*` in bash and recursive in zsh"),
                    ]))],
                    None,
                ),
                decl("lit", call("__sh_unglob", vec![var("field")])),
                // A trailing `/` asks for directories; strip one, remember it.
                decl("w", var("field")),
                decl(
                    "slash",
                    bin(
                        "==",
                        call("__sh_substr", vec![var("field"), int(-1), int(1), boolean(true)]),
                        s("/"),
                    ),
                ),
                if_(
                    var("slash"),
                    vec![set("w", call("__sh_substr", vec![var("field"), int(0), int(-1), boolean(true)]))],
                    None,
                ),
                // Split into components on unescaped `/`, keeping the escapes.
                decl("cs", chars(var("w"))),
                decl("n", len(var("cs"))),
                decl("comps", array(vec![])),
                decl("cur", s("")),
                decl("i", int(0)),
                decl("rooted", boolean(false)),
                while_(bin("<", var("i"), var("n")), vec![
                    decl("c", index(var("cs"), var("i"))),
                    if_(
                        bin("==", var("c"), s("\\")),
                        vec![
                            set("cur", bin("+", var("cur"), var("c"))),
                            if_(
                                bin("<", bin("+", var("i"), int(1)), var("n")),
                                vec![
                                    set(
                                        "cur",
                                        bin("+", var("cur"), index(var("cs"), bin("+", var("i"), int(1)))),
                                    ),
                                    set("i", bin("+", var("i"), int(1))),
                                ],
                                None,
                            ),
                            set("i", bin("+", var("i"), int(1))),
                        ],
                        Some(vec![
                            if_(
                                bin("==", var("c"), s("/")),
                                vec![if_(
                                    bin("and", bin("==", var("i"), int(0)), bin("==", var("cur"), s(""))),
                                    vec![set("rooted", boolean(true))],
                                    Some(vec![
                                        set("comps", bin("+", var("comps"), array(vec![var("cur")]))),
                                        set("cur", s("")),
                                    ]),
                                )],
                                Some(vec![set("cur", bin("+", var("cur"), var("c")))]),
                            ),
                            set("i", bin("+", var("i"), int(1))),
                        ]),
                    ),
                ]),
                set("comps", bin("+", var("comps"), array(vec![var("cur")]))),
                if_(
                    var("rooted"),
                    vec![throw(concat(vec![
                        s("unsupported: an absolute path with a glob in it `"),
                        var("field"),
                        s("` at "),
                        var("at"),
                        s("` — the filesystem sandbox is relative to the script's directory"),
                    ]))],
                    None,
                ),
                // Walk the components left to right. `cands` holds the paths built
                // so far: "" is the working directory, "/" only appears for a
                // rooted pattern (refused above).
                decl("cands", array(vec![s("")])),
                decl("j", int(0)),
                while_(bin("<", var("j"), len(var("comps"))), vec![
                    decl("comp", index(var("comps"), var("j"))),
                    decl("next", array(vec![])),
                    for_("cand", var("cands"), vec![
                        if_(
                            bin("==", var("comp"), s("")),
                            // `a//b`, or the empty component a trailing `/` leaves.
                            vec![expr_stmt(cap("append", vec![var("next"), var("cand")]))],
                            Some(vec![if_(
                                not(call("__sh_meta", vec![var("comp")])),
                                vec![
                                    decl(
                                        "q",
                                        call(
                                            "__sh_join",
                                            vec![var("cand"), call("__sh_unglob", vec![var("comp")])],
                                        ),
                                    ),
                                    if_(
                                        bin("==", cap("fs.exists", vec![var("q")]), int(1)),
                                        vec![expr_stmt(cap("append", vec![var("next"), var("q")]))],
                                        None,
                                    ),
                                ],
                                Some(vec![
                                    decl(
                                        "dirp",
                                        tern(bin("==", var("cand"), s("")), s("."), var("cand")),
                                    ),
                                    if_(
                                        bin(
                                            "and",
                                            bin("==", cap("fs.exists", vec![var("dirp")]), int(1)),
                                            bin("==", cap("fs.is_dir", vec![var("dirp")]), int(1)),
                                        ),
                                        vec![
                                            // A leading `.` in the pattern is what lets
                                            // `.`-names match; otherwise they are skipped.
                                            decl(
                                                "hidden",
                                                bin(
                                                    "or",
                                                    bin(
                                                        "==",
                                                        call(
                                                            "__sh_substr",
                                                            vec![var("comp"), int(0), int(1), boolean(true)],
                                                        ),
                                                        s("."),
                                                    ),
                                                    bin(
                                                        "==",
                                                        call(
                                                            "__sh_substr",
                                                            vec![var("comp"), int(0), int(2), boolean(true)],
                                                        ),
                                                        s("\\."),
                                                    ),
                                                ),
                                            ),
                                            for_("nm", cap("fs.list", vec![var("dirp")]), vec![
                                                if_(
                                                    bin(
                                                        "and",
                                                        bin(
                                                            "==",
                                                            call(
                                                                "__sh_substr",
                                                                vec![var("nm"), int(0), int(1), boolean(true)],
                                                            ),
                                                            s("."),
                                                        ),
                                                        not(var("hidden")),
                                                    ),
                                                    vec![S::Continue { meta: m() }],
                                                    None,
                                                ),
                                                if_(
                                                    bin(
                                                        "==",
                                                        call("__sh_glob", vec![var("comp"), var("nm")]),
                                                        boolean(true),
                                                    ),
                                                    vec![expr_stmt(cap(
                                                        "append",
                                                        vec![
                                                            var("next"),
                                                            call("__sh_join", vec![var("cand"), var("nm")]),
                                                        ],
                                                    ))],
                                                    None,
                                                ),
                                            ]),
                                        ],
                                        None,
                                    ),
                                ]),
                            )]),
                        ),
                    ]),
                    set("cands", var("next")),
                    set("j", bin("+", var("j"), int(1))),
                ]),
                decl(
                    "shown",
                    tern(var("slash"), call("__sh_dironly", vec![var("cands")]), var("cands")),
                ),
                // Nothing matched. `failglob` is checked first: with both set,
                // bash reports the error rather than dropping the word.
                if_(bin("==", len(var("shown")), int(0)), vec![
                    if_(var("failglob"), vec![throw(concat(vec![
                        s(FAILGLOB_SENTINEL),
                        s("no match: "),
                        var("field"),
                    ]))], None),
                    if_(var("nullglob"), vec![ret(array(vec![]))], None),
                    if_(var("nomatch"), vec![throw(concat(vec![
                        s("no matches found: "),
                        var("field"),
                        s(" (at "),
                        var("at"),
                        s(")"),
                    ]))], None),
                    ret(array(vec![var("lit")])),
                ], None),
                ret(call("__sh_sort", vec![var("shown")])),
            ],
        ),

        // ── stdin / stdout plumbing for redirections and pipelines ──────────

        // __sh_slurp(avail, isset): everything this command reads. When its
        // stdin is redirected (a pipeline stage, `<`, a here-document) that is
        // exactly `avail` — even when empty. Otherwise the host's stdin, a line
        // at a time, which is the same source `read` uses.
        "__sh_slurp" => func(
            &["avail", "isset"],
            vec![
                if_(bin("==", var("isset"), int(1)), vec![ret(var("avail"))], None),
                decl("out", s("")),
                while_(boolean(true), vec![
                    decl("line", call("__sh_read_line", vec![boolean(true)])),
                    if_(bin("==", var("line"), s("")), vec![S::Break { meta: m() }], None),
                    set("out", bin("+", bin("+", var("out"), var("line")), s("\n"))),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_in_line(avail): the first line of redirected stdin.
        "__sh_in_line" => func(
            &["avail"],
            vec![
                decl("i", call("__sh_find", vec![var("avail"), s("\n")])),
                if_(
                    bin("<", var("i"), int(0)),
                    vec![ret(var("avail"))],
                    None,
                ),
                ret(call("__sh_substr", vec![var("avail"), int(0), var("i"), boolean(true)])),
            ],
        ),
        // __sh_in_rest(avail): what is left after that first line.
        "__sh_in_rest" => func(
            &["avail"],
            vec![
                decl("i", call("__sh_find", vec![var("avail"), s("\n")])),
                if_(bin("<", var("i"), int(0)), vec![ret(s(""))], None),
                ret(call(
                    "__sh_substr",
                    vec![var("avail"), bin("+", var("i"), int(1)), int(0), boolean(false)],
                )),
            ],
        ),
        // __sh_read_from(avail, isset, raw): one line of stdin, from the
        // redirection when there is one. `raw` (read -r) keeps backslashes.
        "__sh_read_from" => func(
            &["avail", "isset", "raw"],
            vec![
                if_(bin("==", var("isset"), int(1)), vec![
                    decl("line", call("__sh_in_line", vec![var("avail")])),
                    if_(
                        not(var("raw")),
                        vec![ret(call("__sh_unbackslash", vec![var("line")]))],
                        None,
                    ),
                    ret(var("line")),
                ], None),
                ret(call("__sh_read_line", vec![var("raw")])),
            ],
        ),
        // __sh_unbackslash(s): drop the `\` of every `\x`, as `read` does
        // without `-r` (a trailing `\` is dropped too, not continued).
        "__sh_unbackslash" => func(
            &["s"],
            vec![
                decl("cs", chars(var("s"))),
                decl("n", len(var("cs"))),
                decl("i", int(0)),
                decl("out", s("")),
                while_(bin("<", var("i"), var("n")), vec![
                    if_(
                        bin("==", index(var("cs"), var("i")), s("\\")),
                        vec![if_(
                            bin("<", bin("+", var("i"), int(1)), var("n")),
                            vec![
                                set(
                                    "out",
                                    bin("+", var("out"), index(var("cs"), bin("+", var("i"), int(1)))),
                                ),
                                set("i", bin("+", var("i"), int(2))),
                            ],
                            Some(vec![set("i", bin("+", var("i"), int(1)))]),
                        )],
                        Some(vec![
                            set("out", bin("+", var("out"), index(var("cs"), var("i")))),
                            set("i", bin("+", var("i"), int(1))),
                        ]),
                    ),
                ]),
                ret(var("out")),
            ],
        ),

        // ── the builtin command set ─────────────────────────────────────────

        // __sh_lines(s): `s` split into lines, without the empty "line" a final
        // newline would leave behind.
        "__sh_lines" => func(
            &["s"],
            vec![
                decl("out", array(vec![])),
                decl("cur", s("")),
                for_("c", chars(var("s")), vec![if_(
                    bin("==", var("c"), s("\n")),
                    vec![
                        set("out", bin("+", var("out"), array(vec![var("cur")]))),
                        set("cur", s("")),
                    ],
                    Some(vec![set("cur", bin("+", var("cur"), var("c")))]),
                )]),
                if_(
                    bin("!=", var("cur"), s("")),
                    vec![set("out", bin("+", var("out"), array(vec![var("cur")])))],
                    None,
                ),
                ret(var("out")),
            ],
        ),
        // __sh_lines_out(a): lines back to text, each with its newline.
        "__sh_lines_out" => func(
            &["a"],
            vec![
                decl("out", s("")),
                for_("l", var("a"), vec![set(
                    "out",
                    bin("+", bin("+", var("out"), var("l")), s("\n")),
                )]),
                ret(var("out")),
            ],
        ),
        // __sh_nodot(a): the names that do not start with `.` — `ls` hides those.
        "__sh_nodot" => func(
            &["a"],
            vec![
                decl("out", array(vec![])),
                for_("n", var("a"), vec![if_(
                    bin(
                        "!=",
                        call("__sh_substr", vec![var("n"), int(0), int(1), boolean(true)]),
                        s("."),
                    ),
                    vec![expr_stmt(cap("append", vec![var("out"), var("n")]))],
                    None,
                )]),
                ret(var("out")),
            ],
        ),
        // __sh_take(a, n): the first `n` lines.
        "__sh_take" => func(
            &["a", "n"],
            vec![
                decl("out", array(vec![])),
                decl("i", int(0)),
                while_(
                    bin(
                        "and",
                        bin("<", var("i"), var("n")),
                        bin("<", var("i"), len(var("a"))),
                    ),
                    vec![
                        expr_stmt(cap("append", vec![var("out"), index(var("a"), var("i"))])),
                        set("i", bin("+", var("i"), int(1))),
                    ],
                ),
                ret(var("out")),
            ],
        ),
        // __sh_last(a, n): the last `n` lines.
        "__sh_last" => func(
            &["a", "n"],
            vec![
                decl("out", array(vec![])),
                decl("i", bin("-", len(var("a")), var("n"))),
                if_(bin("<", var("i"), int(0)), vec![set("i", int(0))], None),
                while_(bin("<", var("i"), len(var("a"))), vec![
                    expr_stmt(cap("append", vec![var("out"), index(var("a"), var("i"))])),
                    set("i", bin("+", var("i"), int(1))),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_rev(a): the array reversed (`sort -r`, and handy on its own).
        "__sh_rev" => func(
            &["a"],
            vec![
                decl("out", array(vec![])),
                decl("i", bin("-", len(var("a")), int(1))),
                while_(bin(">=", var("i"), int(0)), vec![
                    expr_stmt(cap("append", vec![var("out"), index(var("a"), var("i"))])),
                    set("i", bin("-", var("i"), int(1))),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_dedupe(a): drop repeated *adjacent* lines (the array is sorted
        // where `sort -u` uses this; `uniq` needs the runs, not this).
        "__sh_dedupe" => func(
            &["a"],
            vec![
                decl("out", array(vec![])),
                if_(bin(">", len(var("a")), int(0)), vec![expr_stmt(cap("append", vec![var("out"), index(var("a"), int(0))]))], None),
                decl("i", int(1)),
                while_(bin("<", var("i"), len(var("a"))), vec![
                    if_(
                        bin(
                            "!=",
                            call(
                                "__sh_strcmp",
                                vec![index(var("a"), bin("-", var("i"), int(1))), index(var("a"), var("i"))],
                            ),
                            int(0),
                        ),
                        vec![expr_stmt(cap("append", vec![var("out"), index(var("a"), var("i"))]))],
                        None,
                    ),
                    set("i", bin("+", var("i"), int(1))),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_sort_lines(a, rev, uniq): `sort`. Byte order (which for UTF-8 is
        // also codepoint order), `-r` after the sort, `-u` drops duplicates.
        "__sh_sort_lines" => func(
            &["a", "rev", "uniq"],
            vec![
                decl("s", call("__sh_sort", vec![var("a")])),
                if_(var("uniq"), vec![set("s", call("__sh_dedupe", vec![var("s")]))], None),
                if_(var("rev"), vec![set("s", call("__sh_rev", vec![var("s")]))], None),
                ret(var("s")),
            ],
        ),
        // __sh_uniq(a, mode): `uniq` over adjacent runs. mode 0 keeps one of
        // each run, 1 is `-c` (a count and a space before the line, the count
        // padded to 7), 2 is `-d` (only runs longer than one), 3 is `-u`.
        "__sh_uniq" => func(
            &["a", "mode"],
            vec![
                decl("out", array(vec![])),
                decl("i", int(0)),
                while_(bin("<", var("i"), len(var("a"))), vec![
                    decl("n", int(1)),
                    while_(
                        bin("and", bin("<", bin("+", var("i"), var("n")), len(var("a"))),
                            bin(
                                "==",
                                call(
                                    "__sh_strcmp",
                                    vec![index(var("a"), var("i")), index(var("a"), bin("+", var("i"), var("n")))],
                                ),
                                int(0),
                            )),
                        vec![set("n", bin("+", var("n"), int(1)))],
                    ),
                    if_(
                        bin("==", var("mode"), int(1)),
                        vec![expr_stmt(cap(
                            "append",
                            vec![
                                var("out"),
                                bin(
                                    "+",
                                    bin("+", call("__sh_pad", vec![to_str(var("n")), int(7)]), s(" ")),
                                    index(var("a"), var("i")),
                                ),
                            ],
                        ))],
                        Some(vec![if_(
                            bin(
                                "or",
                                bin("and", bin("==", var("mode"), int(2)), bin(">", var("n"), int(1))),
                                bin("and", bin("==", var("mode"), int(3)), bin("==", var("n"), int(1))),
                            ),
                            vec![expr_stmt(cap("append", vec![var("out"), index(var("a"), var("i"))]))],
                            Some(vec![if_(
                                bin("==", var("mode"), int(0)),
                                vec![expr_stmt(cap("append", vec![var("out"), index(var("a"), var("i"))]))],
                                None,
                            )]),
                        )]),
                    ),
                    set("i", bin("+", var("i"), var("n"))),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_cut(a, delim, field): `cut -d D -f N` over lines. A line without
        // the delimiter has no field N, which GNU `cut` prints whole.
        "__sh_cut" => func(
            &["a", "delim", "field"],
            vec![
                decl("out", array(vec![])),
                for_("l", var("a"), vec![
                    if_(
                        bin("<", call("__sh_find", vec![var("l"), var("delim")]), int(0)),
                        vec![expr_stmt(cap("append", vec![var("out"), var("l")]))],
                        Some(vec![
                            decl("parts", call("str.split", vec![var("l"), var("delim")])),
                            if_(
                                bin("<=", var("field"), len(var("parts"))),
                                vec![expr_stmt(cap(
                                    "append",
                                    vec![var("out"), index(var("parts"), bin("-", var("field"), int(1)))],
                                ))],
                                Some(vec![expr_stmt(cap("append", vec![var("out"), s("")]))]),
                            ),
                        ]),
                    ),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_trset(s): expand the ranges in a `tr` set (`a-z`, `0-9`) into a
        // plain string of characters, the form both `tr` helpers want.
        //
        // Ranges are read off printable ASCII (`RANGE_ALPHA`), where position is
        // `codepoint - 0x20`: this avoids a `conv.ord` call, and a set naming a
        // character outside that alphabet (or an inverted range) is refused
        // rather than guessed at.
        "__sh_trset" => func(
            &["s"],
            vec![
                decl("cs", chars(var("s"))),
                decl("out", s("")),
                decl("i", int(0)),
                // Declared up front: a `decl` inside a branch does not get a
                // fresh slot the VM can load from later in the same branch.
                decl("a", int(0)),
                decl("b", int(0)),
                while_(bin("<", var("i"), len(var("cs"))), vec![
                    if_(
                        bin(
                            "and",
                            bin("and", bin("<", bin("+", var("i"), int(2)), len(var("cs"))),
                                bin("==", index(var("cs"), bin("+", var("i"), int(1))), s("-"))),
                            bin(
                                "and",
                                bin(">=", call("__sh_find", vec![s(RANGE_ALPHA), index(var("cs"), var("i"))]), int(0)),
                                bin(">=", call("__sh_find", vec![s(RANGE_ALPHA), index(var("cs"), bin("+", var("i"), int(2)))]), int(0)),
                            ),
                        ),
                        vec![
                            set("a", call("__sh_find", vec![s(RANGE_ALPHA), index(var("cs"), var("i"))])),
                            set("b", call("__sh_find", vec![s(RANGE_ALPHA), index(var("cs"), bin("+", var("i"), int(2)))])),
                            if_(
                                bin("<", var("b"), var("a")),
                                vec![throw(concat(vec![
                                    s("unsupported: an inverted `tr` range `"),
                                    index(var("cs"), var("i")),
                                    s("-"),
                                    index(var("cs"), bin("+", var("i"), int(2))),
                                    s("`"),
                                ]))],
                                None,
                            ),
                            set(
                                "out",
                                bin(
                                    "+",
                                    var("out"),
                                    call(
                                        "__sh_substr",
                                        vec![
                                            s(RANGE_ALPHA),
                                            var("a"),
                                            bin("+", bin("-", var("b"), var("a")), int(1)),
                                            boolean(true),
                                        ],
                                    ),
                                ),
                            ),
                            set("i", bin("+", var("i"), int(3))),
                        ],
                        Some(vec![
                            set("out", bin("+", var("out"), index(var("cs"), var("i")))),
                            set("i", bin("+", var("i"), int(1))),
                        ]),
                    ),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_tr(text, from, to): map each character of `text` that occurs in
        // `from` to the character at the same position in `to` (past the end
        // of `to`, the last one, as `tr` does).
        "__sh_tr" => func(
            &["text", "from", "to"],
            vec![
                decl("cs", chars(var("text"))),
                decl("ts", chars(var("to"))),
                decl("out", s("")),
                decl("i", int(0)),
                while_(bin("<", var("i"), len(var("cs"))), vec![
                    decl("k", call("__sh_find", vec![var("from"), index(var("cs"), var("i"))])),
                    if_(
                        bin(">=", var("k"), int(0)),
                        vec![set(
                            "out",
                            bin(
                                "+",
                                var("out"),
                                tern(
                                    bin("<", var("k"), len(var("ts"))),
                                    index(var("ts"), var("k")),
                                    index(var("ts"), bin("-", len(var("ts")), int(1))),
                                ),
                            ),
                        )],
                        Some(vec![set("out", bin("+", var("out"), index(var("cs"), var("i"))))]),
                    ),
                    set("i", bin("+", var("i"), int(1))),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_tr_del(text, set): drop every character of `set` (`tr -d`).
        "__sh_tr_del" => func(
            &["text", "set"],
            vec![
                decl("cs", chars(var("text"))),
                decl("out", s("")),
                decl("i", int(0)),
                while_(bin("<", var("i"), len(var("cs"))), vec![
                    if_(
                        bin("<", call("__sh_find", vec![var("set"), index(var("cs"), var("i"))]), int(0)),
                        vec![set("out", bin("+", var("out"), index(var("cs"), var("i"))))],
                        None,
                    ),
                    set("i", bin("+", var("i"), int(1))),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_find(hay, needle): where `needle` first occurs in `hay`, or -1.
        "__sh_find" => func(
            &["hay", "needle"],
            vec![
                decl("h", chars(var("hay"))),
                decl("nd", chars(var("needle"))),
                decl("i", int(0)),
                while_(
                    bin("<=", var("i"), bin("-", len(var("h")), len(var("nd")))),
                    vec![
                        decl("ok", boolean(true)),
                        decl("j", int(0)),
                        while_(bin("<", var("j"), len(var("nd"))), vec![
                            if_(
                                bin(
                                    "!=",
                                    index(var("h"), bin("+", var("i"), var("j"))),
                                    index(var("nd"), var("j")),
                                ),
                                vec![set("ok", boolean(false)), S::Break { meta: m() }],
                                None,
                            ),
                            set("j", bin("+", var("j"), int(1))),
                        ]),
                        if_(var("ok"), vec![ret(var("i"))], None),
                        set("i", bin("+", var("i"), int(1))),
                    ],
                ),
                ret(int(-1)),
            ],
        ),
        // __sh_grep_lines(a, pat, invert): the lines that contain `pat`.
        "__sh_grep_lines" => func(
            &["a", "pat", "invert"],
            vec![
                decl("out", array(vec![])),
                for_("l", var("a"), vec![
                    decl(
                        "hit",
                        bin(">=", call("__sh_find", vec![var("l"), var("pat")]), int(0)),
                    ),
                    if_(
                        bin("!=", var("hit"), var("invert")),
                        vec![expr_stmt(cap("append", vec![var("out"), var("l")]))],
                        None,
                    ),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_re_meta(pat): does `pat` use regular-expression syntax? `grep`
        // here matches literal text only, so such a pattern is refused rather
        // than matched as if the metacharacters were plain characters.
        "__sh_re_meta" => func(
            &["pat"],
            vec![
                decl("cs", chars(var("pat"))),
                decl("i", int(0)),
                while_(bin("<", var("i"), len(var("cs"))), vec![
                    decl("c", index(var("cs"), var("i"))),
                    if_(
                        bin(
                            "or",
                            bin(
                                "or",
                                bin("or", bin("==", var("c"), s("\\")), bin("==", var("c"), s("."))),
                                bin("or", bin("==", var("c"), s("*")), bin("==", var("c"), s("["))),
                            ),
                            bin(
                                "or",
                                bin("or", bin("==", var("c"), s("]")), bin("==", var("c"), s("^"))),
                                bin("==", var("c"), s("$")),
                            ),
                        ),
                        vec![ret(boolean(true))],
                        None,
                    ),
                    set("i", bin("+", var("i"), int(1))),
                ]),
                ret(boolean(false)),
            ],
        ),
        // __sh_count(s, ch): how many times `ch` occurs in `s` (`wc -l`).
        "__sh_count" => func(
            &["s", "ch"],
            vec![
                decl("n", int(0)),
                for_("c", chars(var("s")), vec![if_(
                    bin("==", var("c"), var("ch")),
                    vec![set("n", bin("+", var("n"), int(1)))],
                    None,
                )]),
                ret(var("n")),
            ],
        ),
        // __sh_pad(s, n): `s` right-aligned in `n` columns, the width GNU `wc`
        // uses for its columns.
        "__sh_pad" => func(
            &["s", "n"],
            vec![
                if_(
                    bin("<", len(var("s")), var("n")),
                    vec![ret(bin(
                        "+",
                        call("__sh_repeat", vec![s(" "), bin("-", var("n"), len(var("s")))]),
                        var("s"),
                    ))],
                    None,
                ),
                ret(var("s")),
            ],
        ),
        // __sh_glob(pat, s): does the whole of `s` match the shell pattern
        // `pat`? `*`, `?`, `[…]` (ranges, POSIX classes, `!`/`^` negation) and
        // `\` for a literal character; greedy match with backtracking.
        "__sh_glob" => func(
            &["pat", "s"],
            vec![
                decl("cs", chars(var("s"))),
                decl("n", len(var("cs"))),
                decl("ps", chars(var("pat"))),
                decl("m", len(var("ps"))),
                decl("i", int(0)),
                decl("p", int(0)),
                decl("star", int(-1)),
                decl("mark", int(0)),
                while_(bin("<", var("i"), var("n")), vec![
                    decl("ok", boolean(false)),
                    if_(bin("<", var("p"), var("m")), vec![
                        decl("c", index(var("ps"), var("p"))),
                        if_(
                            bin("==", var("c"), s("*")),
                            vec![
                                set("star", var("p")),
                                set("p", bin("+", var("p"), int(1))),
                                set("mark", var("i")),
                                set("ok", boolean(true)),
                            ],
                            Some(vec![
                                if_(
                                    bin("==", var("c"), s("?")),
                                    vec![
                                        set("p", bin("+", var("p"), int(1))),
                                        set("i", bin("+", var("i"), int(1))),
                                        set("ok", boolean(true)),
                                    ],
                                    Some(vec![
                                        if_(
                                            bin("==", var("c"), s("\\")),
                                            vec![
                                                if_(
                                                    bin("<", bin("+", var("p"), int(1)), var("m")),
                                                    vec![if_(
                                                        bin("==", index(var("ps"), bin("+", var("p"), int(1))), index(var("cs"), var("i"))),
                                                        vec![
                                                            set("p", bin("+", var("p"), int(2))),
                                                            set("i", bin("+", var("i"), int(1))),
                                                            set("ok", boolean(true)),
                                                        ],
                                                        None,
                                                    )],
                                                    Some(vec![if_(
                                                        bin("==", var("c"), index(var("cs"), var("i"))),
                                                        vec![
                                                            set("p", bin("+", var("p"), int(1))),
                                                            set("i", bin("+", var("i"), int(1))),
                                                            set("ok", boolean(true)),
                                                        ],
                                                        None,
                                                    )]),
                                                ),
                                            ],
                                            Some(vec![
                                                if_(
                                                    bin("==", var("c"), s("[")),
                                                    vec![
                                                        decl(
                                                            "r",
                                                            call("__sh_bracket", vec![var("ps"), var("p"), index(var("cs"), var("i"))]),
                                                        ),
                                                        if_(
                                                            bin("==", var("r"), int(-1)),
                                                            vec![if_(
                                                                bin("==", var("c"), index(var("cs"), var("i"))),
                                                                vec![
                                                                    set("p", bin("+", var("p"), int(1))),
                                                                    set("i", bin("+", var("i"), int(1))),
                                                                    set("ok", boolean(true)),
                                                                ],
                                                                None,
                                                            )],
                                                            Some(vec![if_(
                                                                bin(">=", var("r"), int(0)),
                                                                vec![
                                                                    set("p", var("r")),
                                                                    set("i", bin("+", var("i"), int(1))),
                                                                    set("ok", boolean(true)),
                                                                ],
                                                                None,
                                                            )]),
                                                        ),
                                                    ],
                                                    Some(vec![if_(
                                                        bin("==", var("c"), index(var("cs"), var("i"))),
                                                        vec![
                                                            set("p", bin("+", var("p"), int(1))),
                                                            set("i", bin("+", var("i"), int(1))),
                                                            set("ok", boolean(true)),
                                                        ],
                                                        None,
                                                    )]),
                                                ),
                                            ]),
                                        ),
                                    ]),
                                ),
                            ]),
                        ),
                    ], None),
                    // No pattern element matched here: retry with the last `*`
                    // consuming one more character, or fail.
                    if_(
                        not(var("ok")),
                        vec![
                            if_(bin("<", var("star"), int(0)), vec![ret(boolean(false))], None),
                            set("mark", bin("+", var("mark"), int(1))),
                            set("i", var("mark")),
                            set("p", bin("+", var("star"), int(1))),
                        ],
                        None,
                    ),
                ]),
                // Trailing `*`s match the empty tail.
                while_(bin("<", var("p"), var("m")), vec![
                    if_(bin("!=", index(var("ps"), var("p")), s("*")), vec![S::Break { meta: m() }], None),
                    set("p", bin("+", var("p"), int(1))),
                ]),
                ret(bin("==", var("p"), var("m"))),
            ],
        ),
        // __sh_bracket(ps, p, ch): `ps[p]` is a `[`. Returns the index just
        // past the bracket when `ch` matches, -2 when it does not, -1 when the
        // bracket is unterminated (so the `[` is literal).
        "__sh_bracket" => func(
            &["ps", "p", "ch"],
            vec![
                decl("m", len(var("ps"))),
                decl("j", bin("+", var("p"), int(1))),
                decl("neg", boolean(false)),
                decl("first", boolean(true)),
                decl("hit", boolean(false)),
                decl("closed", boolean(false)),
                if_(
                    bin(
                        "and",
                        bin("<", var("j"), var("m")),
                        bin(
                            "or",
                            bin("==", index(var("ps"), var("j")), s("!")),
                            bin("==", index(var("ps"), var("j")), s("^")),
                        ),
                    ),
                    vec![set("neg", boolean(true)), set("j", bin("+", var("j"), int(1)))],
                    None,
                ),
                while_(bin("<", var("j"), var("m")), vec![
                    decl("d", index(var("ps"), var("j"))),
                    if_(
                        bin("and", bin("==", var("d"), s("]")), not(var("first"))),
                        vec![set("closed", boolean(true)), S::Break { meta: m() }],
                        Some(vec![
                            // [:name:]
                            if_(
                                bin(
                                    "and",
                                    bin("and", bin("==", var("d"), s("[")), bin("<", bin("+", var("j"), int(1)), var("m"))),
                                    bin("==", index(var("ps"), bin("+", var("j"), int(1))), s(":")),
                                ),
                                vec![
                                    decl("k", bin("+", var("j"), int(2))),
                                    decl("name", s("", )),
                                    while_(
                                        bin(
                                            "and",
                                            bin("<", bin("+", var("k"), int(1)), var("m")),
                                            not(bin(
                                                "and",
                                                bin("==", index(var("ps"), var("k")), s(":")),
                                                bin("==", index(var("ps"), bin("+", var("k"), int(1))), s("]")),
                                            )),
                                        ),
                                        vec![
                                            set("name", bin("+", var("name"), index(var("ps"), var("k")))),
                                            set("k", bin("+", var("k"), int(1))),
                                        ],
                                    ),
                                    if_(
                                        bin("<", bin("+", var("k"), int(1)), var("m")),
                                        vec![
                                            if_(
                                                call("__sh_class", vec![var("name"), var("ch")]),
                                                vec![set("hit", boolean(true))],
                                                None,
                                            ),
                                            set("j", bin("+", var("k"), int(2))),
                                            set("first", boolean(false)),
                                        ],
                                        Some(vec![
                                            if_(bin("==", var("d"), var("ch")), vec![set("hit", boolean(true))], None),
                                            set("j", bin("+", var("j"), int(1))),
                                            set("first", boolean(false)),
                                        ]),
                                    ),
                                ],
                                Some(vec![
                                    // a-z
                                    if_(
                                        bin(
                                            "and",
                                            bin(
                                                "and",
                                                bin("<", bin("+", var("j"), int(2)), var("m")),
                                                bin("==", index(var("ps"), bin("+", var("j"), int(1))), s("-")),
                                            ),
                                            bin("!=", index(var("ps"), bin("+", var("j"), int(2))), s("]")),
                                        ),
                                        vec![
                                            decl("o", cap("conv.ord", vec![var("ch")])),
                                            if_(
                                                bin(
                                                    "and",
                                                    bin(">=", var("o"), cap("conv.ord", vec![var("d")])),
                                                    bin("<=", var("o"), cap("conv.ord", vec![index(var("ps"), bin("+", var("j"), int(2)))])),
                                                ),
                                                vec![set("hit", boolean(true))],
                                                None,
                                            ),
                                            set("j", bin("+", var("j"), int(3))),
                                            set("first", boolean(false)),
                                        ],
                                        Some(vec![
                                            if_(bin("==", var("d"), var("ch")), vec![set("hit", boolean(true))], None),
                                            set("j", bin("+", var("j"), int(1))),
                                            set("first", boolean(false)),
                                        ]),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                ]),
                if_(not(var("closed")), vec![ret(int(-1))], None),
                if_(bin("!=", var("hit"), var("neg")), vec![ret(bin("+", var("j"), int(1)))], None),
                ret(int(-2)),
            ],
        ),
        // __sh_dval(c): the value of one digit character, -1 when it is not one
        // (`0-9`, `a-f`, `A-F`), so `\0NNN` and `\xHH` can be parsed.
        "__sh_dval" => func(
            &["c"],
            vec![
                decl("o", cap("conv.ord", vec![var("c")])),
                if_(bin("and", bin(">=", var("o"), int(48)), bin("<=", var("o"), int(57))), vec![ret(bin("-", var("o"), int(48)))], None),
                if_(bin("and", bin(">=", var("o"), int(97)), bin("<=", var("o"), int(102))), vec![ret(bin("-", var("o"), int(87)))], None),
                if_(bin("and", bin(">=", var("o"), int(65)), bin("<=", var("o"), int(70))), vec![ret(bin("-", var("o"), int(55)))], None),
                ret(int(-1)),
            ],
        ),
        // __sh_esc_simple(e): the one-character escape `\e` (after the
        // backslash); anything unrecognised keeps its backslash, as in bash.
        "__sh_esc_simple" => {
            let map: &[(&str, i64)] = &[("a", 7), ("b", 8), ("e", 27), ("E", 27), ("f", 12), ("n", 10), ("r", 13), ("t", 9), ("v", 11), ("\\\\", 92)];
            let mut body = Vec::new();
            for (ch, code) in map {
                body.push(if_(
                    bin("==", var("e"), s(ch)),
                    vec![ret(cap("conv.chr", vec![int(*code)]))],
                    None,
                ));
            }
            body.push(ret(bin("+", s("\\"), var("e"))));
            func(&["e"], body)
        }
        // __sh_esc(s): echo/printf %b backslash escapes. `\c` ends the text.
        "__sh_esc" => func(
            &["s"],
            vec![
                decl("cs", chars(var("s"))),
                decl("n", len(var("cs"))),
                decl("i", int(0)),
                decl("out", s("")),
                while_(
                    bin("<", var("i"), var("n")),
                    vec![
                        decl("c", index(var("cs"), var("i"))),
                        if_(
                            bin("!=", var("c"), s("\\")),
                            vec![
                                set("out", bin("+", var("out"), var("c"))),
                                set("i", bin("+", var("i"), int(1))),
                            ],
                            Some(vec![
                                if_(
                                    bin(">=", bin("+", var("i"), int(1)), var("n")),
                                    vec![
                                        set("out", bin("+", var("out"), s("\\"))),
                                        set("i", bin("+", var("i"), int(1))),
                                    ],
                                    Some(vec![
                                        decl("e", index(var("cs"), bin("+", var("i"), int(1)))),
                                        if_(
                                            bin("==", var("e"), s("c")),
                                            vec![S::Break { meta: m() }],
                                            Some(vec![
                                                if_(
                                                    bin("==", var("e"), s("0")),
                                                    vec![
                                                        decl("j", bin("+", var("i"), int(2))),
                                                        decl("v", int(0)),
                                                        decl("k", int(0)),
                                                        while_(
                                                            bin(
                                                                "and",
                                                                bin("<", var("k"), int(3)),
                                                                bin("<", var("j"), var("n")),
                                                            ),
                                                            vec![
                                                                decl("d", call("__sh_dval", vec![index(var("cs"), var("j"))])),
                                                                if_(
                                                                    bin("and", bin(">=", var("d"), int(0)), bin("<=", var("d"), int(7))),
                                                                    vec![
                                                                        set("v", bin("+", bin("*", var("v"), int(8)), var("d"))),
                                                                        set("j", bin("+", var("j"), int(1))),
                                                                        set("k", bin("+", var("k"), int(1))),
                                                                    ],
                                                                    Some(vec![S::Break { meta: m() }]),
                                                                ),
                                                            ],
                                                        ),
                                                        set("out", bin("+", var("out"), cap("conv.chr", vec![var("v")]))),
                                                        set("i", var("j")),
                                                    ],
                                                    Some(vec![
                                                        if_(
                                                            bin("==", var("e"), s("x")),
                                                            vec![
                                                                decl("j", bin("+", var("i"), int(2))),
                                                                decl("v", int(0)),
                                                                decl("k", int(0)),
                                                                while_(
                                                                    bin(
                                                                        "and",
                                                                        bin("<", var("k"), int(2)),
                                                                        bin("<", var("j"), var("n")),
                                                                    ),
                                                                    vec![
                                                                        decl("d", call("__sh_dval", vec![index(var("cs"), var("j"))])),
                                                                        if_(
                                                                            bin("and", bin(">=", var("d"), int(0)), bin("<=", var("d"), int(15))),
                                                                            vec![
                                                                                set("v", bin("+", bin("*", var("v"), int(16)), var("d"))),
                                                                                set("j", bin("+", var("j"), int(1))),
                                                                                set("k", bin("+", var("k"), int(1))),
                                                                            ],
                                                                            Some(vec![S::Break { meta: m() }]),
                                                                        ),
                                                                    ],
                                                                ),
                                                                if_(
                                                                    bin("==", var("k"), int(0)),
                                                                    vec![
                                                                        set("out", bin("+", var("out"), s("\\x"))),
                                                                        set("i", bin("+", var("i"), int(2))),
                                                                    ],
                                                                    Some(vec![
                                                                        set("out", bin("+", var("out"), cap("conv.chr", vec![var("v")]))),
                                                                        set("i", var("j")),
                                                                    ]),
                                                                ),
                                                            ],
                                                            Some(vec![
                                                                set("out", bin("+", var("out"), call("__sh_esc_simple", vec![var("e")]))),
                                                                set("i", bin("+", var("i"), int(2))),
                                                            ]),
                                                        ),
                                                    ]),
                                                ),
                                            ]),
                                        ),
                                    ]),
                                ),
                            ]),
                        ),
                    ],
                ),
                ret(var("out")),
            ],
        ),
        // __sh_esc_stop(s): does the text contain `\c` (which ends echo's output)?
        "__sh_esc_stop" => func(
            &["s"],
            vec![
                decl("cs", chars(var("s"))),
                decl("n", len(var("cs"))),
                decl("i", int(0)),
                while_(bin("<", var("i"), var("n")), vec![
                    if_(
                        bin("==", index(var("cs"), var("i")), s("\\")),
                        vec![
                            if_(
                                bin("and", bin("<", bin("+", var("i"), int(1)), var("n")), bin("==", index(var("cs"), bin("+", var("i"), int(1))), s("c"))),
                                vec![ret(boolean(true))],
                                None,
                            ),
                            set("i", bin("+", var("i"), int(2))),
                        ],
                        Some(vec![set("i", bin("+", var("i"), int(1)))]),
                    ),
                ]),
                ret(boolean(false)),
            ],
        ),
        // __sh_arr_or_empty(a): an array value, with unset reading as empty.
        "__sh_arr_or_empty" => func(
            &["a"],
            vec![if_(bin("==", var("a"), null()), vec![ret(array(vec![]))], None), ret(var("a"))],
        ),
        // __sh_arr_set(a, i, one_based, v): `a[i]=v`. bash appends past the end
        // (indices stay dense); zsh grows with empty slots.
        "__sh_arr_set" => func(
            &["a", "i", "one", "v"],
            vec![
                decl("arr", var("a")),
                if_(bin("==", var("arr"), null()), vec![set("arr", array(vec![]))], None),
                decl("n", len(var("arr"))),
                decl("k", int(0)),
                if_(
                    bin("<", var("i"), int(0)),
                    vec![set("k", bin("+", bin("+", var("n"), var("i")), int(1)))],
                    Some(vec![
                        if_(var("one"), vec![set("k", var("i"))], Some(vec![set("k", bin("+", var("i"), int(1)))])),
                    ]),
                ),
                if_(
                    bin("<", var("k"), int(1)),
                    vec![throw(concat(vec![s("unsupported: array index `"), var("i"), s("` out of range")]))],
                    None,
                ),
                if_(
                    bin(">", var("k"), var("n")),
                    vec![
                        if_(
                            var("one"),
                            vec![while_(
                                bin("<", len(var("arr")), var("k")),
                                vec![set("arr", bin("+", var("arr"), array(vec![s("")])))],
                            )],
                            None,
                        ),
                        ret(bin("+", var("arr"), array(vec![var("v")]))),
                    ],
                    Some(vec![ret(cap("arr_set", vec![var("arr"), bin("-", var("k"), int(1)), var("v")]))]),
                ),
            ],
        ),
        // __sh_read_line(raw): one line from stdin via `io.read`. Without -r,
        // a trailing backslash continues the line and backslashes are removed.
        "__sh_read_line" => func(
            &["raw"],
            vec![
                decl("line", cap("io.read", vec![])),
                if_(var("raw"), vec![ret(var("line"))], None),
                while_(
                    bin("and", bin(">", len(chars(var("line"))), int(0)), bin("==", index(chars(var("line")), bin("-", len(chars(var("line"))), int(1))), s("\\"))),
                    vec![set(
                        "line",
                        bin(
                            "+",
                            call("__sh_substr", vec![var("line"), int(0), int(-1), boolean(true)]),
                            cap("io.read", vec![]),
                        ),
                    )],
                ),
                // `\x` → `x`; a lone trailing backslash is dropped.
                decl("cs", chars(var("line"))),
                decl("n", len(var("cs"))),
                decl("i", int(0)),
                decl("out", s("")),
                while_(bin("<", var("i"), var("n")), vec![
                    if_(
                        bin("==", index(var("cs"), var("i")), s("\\")),
                        vec![
                            if_(
                                bin("<", bin("+", var("i"), int(1)), var("n")),
                                vec![
                                    set("out", bin("+", var("out"), index(var("cs"), bin("+", var("i"), int(1))))),
                                    set("i", bin("+", var("i"), int(2))),
                                ],
                                Some(vec![set("i", bin("+", var("i"), int(1)))]),
                            ),
                        ],
                        Some(vec![
                            set("out", bin("+", var("out"), index(var("cs"), var("i")))),
                            set("i", bin("+", var("i"), int(1))),
                        ]),
                    ),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_read_split(line, n): `n` fields, the last one the rest of the
        // line with its internal spacing kept (as bash's `read a b` does).
        "__sh_read_split" => func(
            &["line", "n"],
            vec![
                decl("cs", chars(var("line"))),
                decl("m", len(var("cs"))),
                decl("i", int(0)),
                decl("out", array(vec![])),
                decl("k", int(1)),
                while_(bin("<", var("k"), var("n")), vec![
                    while_(
                        bin("and", bin("<", var("i"), var("m")), call("__sh_isws", vec![index(var("cs"), var("i"))])),
                        vec![set("i", bin("+", var("i"), int(1)))],
                    ),
                    decl("start", var("i")),
                    while_(
                        bin("and", bin("<", var("i"), var("m")), not(call("__sh_isws", vec![index(var("cs"), var("i"))]))),
                        vec![set("i", bin("+", var("i"), int(1)))],
                    ),
                    set("out", bin("+", var("out"), array(vec![call("__sh_join_range", vec![var("cs"), var("start"), var("i")])]))),
                    set("k", bin("+", var("k"), int(1))),
                ]),
                while_(
                    bin("and", bin("<", var("i"), var("m")), call("__sh_isws", vec![index(var("cs"), var("i"))])),
                    vec![set("i", bin("+", var("i"), int(1)))],
                ),
                decl("end", var("m")),
                while_(
                    bin("and", bin(">", var("end"), var("i")), call("__sh_isws", vec![index(var("cs"), bin("-", var("end"), int(1)))])),
                    vec![set("end", bin("-", var("end"), int(1)))],
                ),
                set("out", bin("+", var("out"), array(vec![call("__sh_join_range", vec![var("cs"), var("i"), var("end")])]))),
                ret(var("out")),
            ],
        ),
        // __sh_str(v): a shell string, with an unset (null) variable reading as "".
        "__sh_str" => func(
            &["v"],
            vec![if_(bin("==", var("v"), null()), vec![ret(s(""))], None), ret(var("v"))],
        ),
        // __sh_arr_str(a, one_based): an array as one string: element 0 in
        // bash, every element joined in zsh.
        "__sh_arr_str" => func(
            &["a", "one"],
            vec![
                if_(bin("==", var("a"), null()), vec![ret(s(""))], None),
                if_(var("one"), vec![ret(call("str.join", vec![var("a"), s(" ")]))], None),
                if_(bin(">", len(var("a")), int(0)), vec![ret(index(var("a"), int(0)))], None),
                ret(s("")),
            ],
        ),
        // __sh_arr_get(a, i, one_based): one element, "" when out of range.
        // bash indexes from 0, zsh from 1 (and `a[0]` is out of range there);
        // a negative index counts back from the end in both.
        "__sh_arr_get" => func(
            &["a", "i", "one"],
            vec![
                if_(bin("==", var("a"), null()), vec![ret(s(""))], None),
                decl("n", len(var("a"))),
                decl("k", int(0)),
                if_(
                    bin("<", var("i"), int(0)),
                    vec![set("k", bin("+", bin("+", var("n"), var("i")), int(1)))],
                    Some(vec![
                        if_(var("one"), vec![set("k", var("i"))], Some(vec![set("k", bin("+", var("i"), int(1)))])),
                    ]),
                ),
                if_(bin("or", bin("<", var("k"), int(1)), bin(">", var("k"), var("n"))), vec![ret(s(""))], None),
                ret(index(var("a"), bin("-", var("k"), int(1)))),
            ],
        ),
        // __sh_arr_len(a, one_based): the number of elements.
        "__sh_arr_len" => func(
            &["a", "one"],
            vec![if_(bin("==", var("a"), null()), vec![ret(int(0))], None), ret(len(var("a")))],
        ),
        // __sh_split_arr(a, at): bash's unquoted `${a[@]}` — every element is
        // itself field-split.
        "__sh_split_arr" => func(
            &["a", "at"],
            vec![
                decl("out", array(vec![])),
                if_(bin("==", var("a"), null()), vec![ret(var("out"))], None),
                for_("e", var("a"), vec![set(
                    "out",
                    bin("+", var("out"), call("__sh_split", vec![var("e"), var("at")])),
                )]),
                ret(var("out")),
            ],
        ),
        // __sh_join_range(cs, from, to): characters `[from, to)` as a string.
        "__sh_join_range" => func(
            &["cs", "from", "to"],
            vec![
                decl("out", s("")),
                decl("i", var("from")),
                while_(bin("<", var("i"), var("to")), vec![
                    set("out", bin("+", var("out"), index(var("cs"), var("i")))),
                    set("i", bin("+", var("i"), int(1))),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_replace(s, pat, rep, all, anchor, zsh, at): `${s/pat/rep}` and
        // `${s//pat/rep}` with a shell glob pattern (`*`, `?`, `[…]`, `\`), the
        // same matcher `${s#pat}` uses. Matching is leftmost-longest, as in bash
        // and zsh: the leftmost position wins, then the longest match there.
        // `all` replaces every non-overlapping match, skipping an empty one
        // (bash's `${x//a*/Y}` on `bab` is `bY`, not `Yb Y`); `anchor` 1
        // (`/#pat`) and 2 (`/%pat`) allow a match only at the start or the end.
        "__sh_replace" => func(
            &["s", "pat", "rep", "all", "anchor", "zsh", "at"],
            vec![
                if_(
                    bin("==", var("pat"), s("")),
                    vec![
                        // An empty pattern with an anchor inserts the
                        // replacement at that end (`${x/#/pre}`, `${x/%/post}`).
                        if_(bin("==", var("anchor"), int(1)), vec![ret(bin("+", var("rep"), var("s")))], None),
                        if_(bin("==", var("anchor"), int(2)), vec![ret(bin("+", var("s"), var("rep")))], None),
                        // bash: an empty pattern leaves the value alone. zsh
                        // instead inserts the replacement around every
                        // character, which is not lowered here.
                        if_(
                            var("zsh"),
                            vec![throw(concat(vec![
                                s("unsupported: `${x//\"\"/…}` (an empty pattern) in zsh at "),
                                var("at"),
                            ]))],
                            None,
                        ),
                        ret(var("s")),
                    ],
                    None,
                ),
                decl("cs", chars(var("s"))),
                decl("n", len(var("cs"))),
                decl("out", s("")),
                decl("i", int(0)),
                decl("done", boolean(false)),
                while_(bin("<=", var("i"), var("n")), vec![
                    // `advance`: copy `cs[i]` and move on by one character.
                    decl("advance", boolean(true)),
                    decl("mj", int(-1)),
                    if_(
                        not(var("done")),
                        vec![
                            // Longest match starting at `i`.
                            decl("j", var("n")),
                            while_(bin(">=", var("j"), var("i")), vec![
                                if_(
                                    bin(
                                        "==",
                                        call(
                                            "__sh_glob",
                                            vec![var("pat"), call("__sh_join_range", vec![var("cs"), var("i"), var("j")])],
                                        ),
                                        boolean(true),
                                    ),
                                    vec![set("mj", var("j")), S::Break { meta: m() }],
                                    None,
                                ),
                                set("j", bin("-", var("j"), int(1))),
                            ]),
                            if_(
                                bin(
                                    "and",
                                    bin(">=", var("mj"), int(0)),
                                    not(bin(
                                        "or",
                                        bin(
                                            "or",
                                            // A start-anchored pattern only at 0.
                                            bin(
                                                "and",
                                                bin("==", var("anchor"), int(1)),
                                                bin("!=", var("i"), int(0)),
                                            ),
                                            // An end-anchored one only at the end.
                                            bin(
                                                "and",
                                                bin("==", var("anchor"), int(2)),
                                                bin("!=", var("mj"), var("n")),
                                            ),
                                        ),
                                        // An empty match is never replaced.
                                        bin("==", var("mj"), var("i")),
                                    )),
                                ),
                                vec![
                                    set("out", bin("+", var("out"), var("rep"))),
                                    set("i", var("mj")),
                                    set("advance", boolean(false)),
                                    // A single replacement, and an anchored one,
                                    // end the scan.
                                    if_(
                                        bin("or", not(var("all")), bin("!=", var("anchor"), int(0))),
                                        vec![set("done", boolean(true))],
                                        None,
                                    ),
                                ],
                                None,
                            ),
                        ],
                        None,
                    ),
                    if_(
                        var("advance"),
                        vec![
                            // At the end there is no character left to copy:
                            // only a `/%pat` anchor looks for a match there.
                            if_(
                                bin("<", var("i"), var("n")),
                                vec![set("out", bin("+", var("out"), index(var("cs"), var("i"))))],
                                None,
                            ),
                            set("i", bin("+", var("i"), int(1))),
                        ],
                        None,
                    ),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_substr(s, off, len, has_len): `${s:off:len}`, by character.
        "__sh_substr" => func(
            &["s", "off", "ln", "has_len"],
            vec![
                decl("cs", chars(var("s"))),
                decl("n", len(var("cs"))),
                decl("o", var("off")),
                if_(bin("<", var("o"), int(0)), vec![set("o", bin("+", var("n"), var("o")))], None),
                if_(bin("<", var("o"), int(0)), vec![set("o", int(0))], None),
                if_(bin(">", var("o"), var("n")), vec![ret(s(""))], None),
                if_(not(var("has_len")), vec![ret(call("__sh_join_range", vec![var("cs"), var("o"), var("n")]))], None),
                decl("e", var("ln")),
                if_(bin("<", var("e"), int(0)), vec![set("e", bin("-", bin("+", var("n"), var("e")), var("o")))], None),
                if_(bin("<=", var("e"), int(0)), vec![ret(s(""))], None),
                if_(bin(">", bin("+", var("o"), var("e")), var("n")), vec![set("e", bin("-", var("n"), var("o")))], None),
                ret(call("__sh_join_range", vec![var("cs"), var("o"), bin("+", var("o"), var("e"))])),
            ],
        ),
        // __sh_trim(s, pat, longest, prefix): `${s#pat}` and friends.
        "__sh_trim" => {
            let cs_var = || var("cs");
            let cand = |from: E, to: E| call("__sh_join_range", vec![cs_var(), from, to]);
            let n = || var("n");
            // prefix: try the longest (i = n → 0) or shortest (i = 0 → n)
            // matching prefix and drop it.
            let prefix_long = vec![
                decl("i", var("n")),
                while_(bin(">=", var("i"), int(0)), vec![
                    if_(bin("==", call("__sh_glob", vec![var("pat"), cand(int(0), var("i"))]), boolean(true)), vec![ret(cand(var("i"), n()))], None),
                    set("i", bin("-", var("i"), int(1))),
                ]),
            ];
            let prefix_short = vec![
                decl("i", int(0)),
                while_(bin("<=", var("i"), var("n")), vec![
                    if_(bin("==", call("__sh_glob", vec![var("pat"), cand(int(0), var("i"))]), boolean(true)), vec![ret(cand(var("i"), n()))], None),
                    set("i", bin("+", var("i"), int(1))),
                ]),
            ];
            let suffix_long = vec![
                decl("k", var("n")),
                while_(bin(">=", var("k"), int(0)), vec![
                    if_(
                        bin("==", call("__sh_glob", vec![var("pat"), cand(bin("-", n(), var("k")), n())]), boolean(true)),
                        vec![ret(cand(int(0), bin("-", n(), var("k"))))],
                        None,
                    ),
                    set("k", bin("-", var("k"), int(1))),
                ]),
            ];
            let suffix_short = vec![
                decl("k", int(0)),
                while_(bin("<=", var("k"), var("n")), vec![
                    if_(
                        bin("==", call("__sh_glob", vec![var("pat"), cand(bin("-", n(), var("k")), n())]), boolean(true)),
                        vec![ret(cand(int(0), bin("-", n(), var("k"))))],
                        None,
                    ),
                    set("k", bin("+", var("k"), int(1))),
                ]),
            ];
            func(
                &["s", "pat", "longest", "prefix"],
                vec![
                    decl("cs", chars(var("s"))),
                    decl("n", len(var("cs"))),
                    if_(
                        var("prefix"),
                        vec![if_(var("longest"), prefix_long, Some(prefix_short))],
                        Some(vec![if_(var("longest"), suffix_long, Some(suffix_short))]),
                    ),
                    ret(var("s")),
                ],
            )
        }
        // __sh_assign_default(g, name, val, has): `${x:=y}`.
        "__sh_assign_default" => func(
            &["g", "name", "val", "has"],
            vec![
                if_(var("has"), vec![ret(call("__sh_str", vec![index(var("g"), var("name"))]))], None),
                expr_stmt(call("__crush_setindex__", vec![var("g"), var("name"), var("val")])),
                ret(var("val")),
            ],
        ),
        // __sh_err_if_unset(has, name, msg, val): `${x?msg}` / `${x:?msg}`.
        "__sh_err_if_unset" => func(
            &["has", "name", "msg", "val"],
            vec![
                if_(
                    not(var("has")),
                    vec![throw(concat(vec![s("unsupported: `"), var("name"), s("` is needed here (`${…?}`): "), var("msg")]))],
                    None,
                ),
                ret(var("val")),
            ],
        ),
        // __sh_number(v, at): a whole number the way printf reads one —
        // surrounding space, an optional sign, then `0x…` hex, `'c` for a
        // character code, or decimal digits. Anything else is refused.
        "__sh_number" => func(
            &["v", "at"],
            vec![
                decl("cs", chars(call("__sh_str", vec![var("v")]))),
                decl("n", len(var("cs"))),
                decl("i", int(0)),
                while_(
                    bin("and", bin("<", var("i"), var("n")), call("__sh_isws", vec![index(var("cs"), var("i"))])),
                    vec![set("i", bin("+", var("i"), int(1)))],
                ),
                decl("neg", boolean(false)),
                if_(
                    bin("and", bin("<", var("i"), var("n")), bin("==", index(var("cs"), var("i")), s("-"))),
                    vec![set("neg", boolean(true)), set("i", bin("+", var("i"), int(1)))],
                    Some(vec![if_(
                        bin("and", bin("<", var("i"), var("n")), bin("==", index(var("cs"), var("i")), s("+"))),
                        vec![set("i", bin("+", var("i"), int(1)))],
                        None,
                    )]),
                ),
                // 'c: the character code of the next character.
                if_(
                    bin("and", bin("<", bin("+", var("i"), int(1)), var("n")), bin("==", index(var("cs"), var("i")), s("'"))),
                    vec![ret(tern(
                        var("neg"),
                        bin("-", int(0), cap("conv.ord", vec![index(var("cs"), bin("+", var("i"), int(1)))])),
                        cap("conv.ord", vec![index(var("cs"), bin("+", var("i"), int(1)))]),
                    ))],
                    None,
                ),
                decl("base", int(10)),
                // 0x / 0X
                if_(
                    bin(
                        "and",
                        bin("<", bin("+", var("i"), int(1)), var("n")),
                        bin("and", bin("==", index(var("cs"), var("i")), s("0")), bin(
                            "or",
                            bin("==", index(var("cs"), bin("+", var("i"), int(1))), s("x")),
                            bin("==", index(var("cs"), bin("+", var("i"), int(1))), s("X")),
                        )),
                    ),
                    vec![set("base", int(16)), set("i", bin("+", var("i"), int(2)))],
                    None,
                ),
                decl("start", var("i")),
                decl("val", int(0)),
                while_(bin("<", var("i"), var("n")), vec![
                    decl("d", call("__sh_dval", vec![index(var("cs"), var("i"))])),
                    if_(
                        bin("or", bin("<", var("d"), int(0)), bin(">=", var("d"), var("base"))),
                        vec![S::Break { meta: m() }],
                        None,
                    ),
                    set("val", bin("+", bin("*", var("val"), var("base")), var("d"))),
                    set("i", bin("+", var("i"), int(1))),
                ]),
                while_(
                    bin("and", bin("<", var("i"), var("n")), call("__sh_isws", vec![index(var("cs"), var("i"))])),
                    vec![set("i", bin("+", var("i"), int(1)))],
                ),
                if_(
                    bin("or", bin("==", var("i"), var("start")), bin("!=", var("i"), var("n"))),
                    vec![throw(concat(vec![s("unsupported: printf needs a whole number, got `"), var("v"), s("` at "), var("at")]))],
                    None,
                ),
                if_(var("neg"), vec![ret(bin("-", int(0), var("val")))], None),
                ret(var("val")),
            ],
        ),
        // __sh_isws(c): a space, tab or newline.
        "__sh_isws" => func(
            &["c"],
            vec![
                decl("o", cap("conv.ord", vec![var("c")])),
                ret(bin(
                    "or",
                    bin("or", bin("==", var("o"), int(32)), bin("==", var("o"), int(9))),
                    bin("or", bin("==", var("o"), int(10)), bin("==", var("o"), int(13))),
                )),
            ],
        ),
        // __sh_base(n, base, upper): a non-negative number in another base.
        "__sh_base" => func(
            &["n", "base", "upper"],
            vec![
                decl("cs", chars(s("0123456789abcdefghijklmnopqrstuvwxyz"))),
                if_(var("upper"), vec![set("cs", chars(s("0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ")))], None),
                decl("v", var("n")),
                if_(bin("==", var("v"), int(0)), vec![ret(s("0"))], None),
                decl("out", s("")),
                while_(bin(">", var("v"), int(0)), vec![
                    set(
                        "out",
                        bin(
                            "+",
                            index(var("cs"), bin("%", var("v"), var("base"))),
                            var("out"),
                        ),
                    ),
                    set("v", bin("/", var("v"), var("base"))),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_repeat(c, n): `c` repeated `n` times (padding, zero fill).
        "__sh_repeat" => func(
            &["c", "n"],
            vec![
                decl("out", s("")),
                decl("i", int(0)),
                while_(bin("<", var("i"), var("n")), vec![
                    set("out", bin("+", var("out"), var("c"))),
                    set("i", bin("+", var("i"), int(1))),
                ]),
                ret(var("out")),
            ],
        ),
        // __sh_isdigit(c): is this one character an ASCII decimal digit?
        "__sh_isdigit" => func(
            &["c"],
            vec![
                decl("o", cap("conv.ord", vec![var("c")])),
                ret(bin("and", bin(">=", var("o"), int(48)), bin("<=", var("o"), int(57)))),
            ],
        ),
        // __sh_fparse(v, at): the number printf reads for a float conversion, as
        // {neg, d, p} with |v| = 0.d1d2… × 10^p (`d` holds the significant
        // digits with no leading or trailing zeros; zero is `d = "0"`,
        // `p = 1`). Surrounding space, a sign, `.5`, `5.` and `e`/`E` exponents
        // are accepted like C's strtod; anything else after the number is
        // refused rather than read as a partial value.
        //
        // The value stays decimal: bash converts to `double` first, so where a
        // decimal sits exactly on a rounding tie and is not representable in
        // binary (`2.675` with `%.2f`, which bash prints as `2.67`), this
        // rounds the number as written (`2.68`). Exact `double` behaviour needs
        // strtod plus exact binary→decimal conversion; see CRUSH-310.
        "__sh_fparse" => func(
            &["v", "at"],
            vec![
                decl("text", call("__sh_str", vec![var("v")])),
                decl("cs", chars(var("text"))),
                decl("n", len(var("cs"))),
                decl("i", int(0)),
                while_(bin("<", var("i"), var("n")), vec![
                    if_(
                        call("__sh_isws", vec![index(var("cs"), var("i"))]),
                        vec![set("i", bin("+", var("i"), int(1)))],
                        Some(vec![S::Break { meta: m() }]),
                    ),
                ]),
                decl("neg", boolean(false)),
                if_(bin("<", var("i"), var("n")), vec![
                    if_(
                        bin(
                            "or",
                            bin("==", index(var("cs"), var("i")), s("-")),
                            bin("==", index(var("cs"), var("i")), s("+")),
                        ),
                        vec![
                            set("neg", bin("==", index(var("cs"), var("i")), s("-"))),
                            set("i", bin("+", var("i"), int(1))),
                        ],
                        None,
                    ),
                ], None),
                decl("d", s("")),
                decl("ints", int(0)),
                decl("seen", boolean(false)),
                while_(bin("<", var("i"), var("n")), vec![
                    if_(
                        call("__sh_isdigit", vec![index(var("cs"), var("i"))]),
                        vec![
                            set("d", bin("+", var("d"), index(var("cs"), var("i")))),
                            set("ints", bin("+", var("ints"), int(1))),
                            set("seen", boolean(true)),
                            set("i", bin("+", var("i"), int(1))),
                        ],
                        Some(vec![S::Break { meta: m() }]),
                    ),
                ]),
                if_(
                    bin("and", bin("<", var("i"), var("n")), bin("==", index(var("cs"), var("i")), s("."))),
                    vec![
                        set("i", bin("+", var("i"), int(1))),
                        while_(bin("<", var("i"), var("n")), vec![
                            if_(
                                call("__sh_isdigit", vec![index(var("cs"), var("i"))]),
                                vec![
                                    set("d", bin("+", var("d"), index(var("cs"), var("i")))),
                                    set("seen", boolean(true)),
                                    set("i", bin("+", var("i"), int(1))),
                                ],
                                Some(vec![S::Break { meta: m() }]),
                            ),
                        ]),
                    ],
                    None,
                ),
                // `p` is the number of digits before the decimal point; the
                // exponent is added to it.
                decl("p", var("ints")),
                if_(bin("<", var("i"), var("n")), vec![
                    if_(
                        bin(
                            "or",
                            bin("==", index(var("cs"), var("i")), s("e")),
                            bin("==", index(var("cs"), var("i")), s("E")),
                        ),
                        vec![
                            set("i", bin("+", var("i"), int(1))),
                            decl("esign", int(1)),
                            if_(bin("<", var("i"), var("n")), vec![
                                if_(
                                    bin("==", index(var("cs"), var("i")), s("-")),
                                    vec![
                                        set("esign", int(-1)),
                                        set("i", bin("+", var("i"), int(1))),
                                    ],
                                    None,
                                ),
                                if_(
                                    bin("==", index(var("cs"), var("i")), s("+")),
                                    vec![set("i", bin("+", var("i"), int(1)))],
                                    None,
                                ),
                            ], None),
                            decl("ex", int(0)),
                            decl("any", boolean(false)),
                            while_(bin("<", var("i"), var("n")), vec![
                                if_(
                                    call("__sh_isdigit", vec![index(var("cs"), var("i"))]),
                                    vec![
                                        set(
                                            "ex",
                                            bin(
                                                "+",
                                                bin("*", var("ex"), int(10)),
                                                bin(
                                                    "-",
                                                    cap("conv.ord", vec![index(var("cs"), var("i"))]),
                                                    int(48),
                                                ),
                                            ),
                                        ),
                                        set("any", boolean(true)),
                                        set("i", bin("+", var("i"), int(1))),
                                    ],
                                    Some(vec![S::Break { meta: m() }]),
                                ),
                            ]),
                            if_(
                                not(var("any")),
                                vec![throw(concat(vec![
                                    s("unsupported: a float conversion of `"),
                                    var("text"),
                                    s("` (an empty exponent) at "),
                                    var("at"),
                                ]))],
                                None,
                            ),
                            set("p", bin("+", var("p"), bin("*", var("esign"), var("ex")))),
                        ],
                        None,
                    ),
                ], None),
                while_(bin("<", var("i"), var("n")), vec![
                    if_(
                        call("__sh_isws", vec![index(var("cs"), var("i"))]),
                        vec![set("i", bin("+", var("i"), int(1)))],
                        Some(vec![S::Break { meta: m() }]),
                    ),
                ]),
                if_(
                    bin("or", not(var("seen")), bin("<", var("i"), var("n"))),
                    vec![throw(concat(vec![
                        s("unsupported: a float conversion of `"),
                        var("text"),
                        s("` (not a number) at "),
                        var("at"),
                    ]))],
                    None,
                ),
                // Leading zeros carry no significance: drop them and lower the
                // exponent by one for each.
                decl("k", int(0)),
                while_(bin("<", var("k"), len(chars(var("d")))), vec![
                    if_(
                        bin("==", index(chars(var("d")), var("k")), s("0")),
                        vec![
                            set("k", bin("+", var("k"), int(1))),
                            set("p", bin("-", var("p"), int(1))),
                        ],
                        Some(vec![S::Break { meta: m() }]),
                    ),
                ]),
                set(
                    "d",
                    call(
                        "__sh_substr",
                        vec![
                            var("d"),
                            var("k"),
                            bin("-", len(chars(var("d"))), var("k")),
                            boolean(true),
                        ],
                    ),
                ),
                if_(
                    bin("==", var("d"), s("")),
                    vec![set("d", s("0")), set("p", int(1))],
                    None,
                ),
                ret(E::ObjectLiteral {
                    properties: vec![
                        ("neg".to_string(), var("neg")),
                        ("d".to_string(), var("d")),
                        ("p".to_string(), var("p")),
                    ],
                    meta: m(),
                }),
            ],
        ),
        // __sh_finc(s): a decimal digit string plus one, carrying as in grade
        // school ("999" → "1000", "" → "1").
        "__sh_finc" => func(
            &["s"],
            vec![
                decl("cs", chars(var("s"))),
                decl("n", len(var("cs"))),
                if_(bin("==", var("n"), int(0)), vec![ret(s("1"))], None),
                decl("i", bin("-", var("n"), int(1))),
                while_(bin(">=", var("i"), int(0)), vec![
                    if_(
                        bin("==", index(var("cs"), var("i")), s("9")),
                        vec![set("i", bin("-", var("i"), int(1)))],
                        Some(vec![S::Break { meta: m() }]),
                    ),
                ]),
                if_(bin("<", var("i"), int(0)), vec![
                    ret(concat(vec![
                        s("1"),
                        call("__sh_repeat", vec![s("0"), var("n")]),
                    ])),
                ], None),
                ret(concat(vec![
                    call("__sh_substr", vec![var("s"), int(0), var("i"), boolean(true)]),
                    cap(
                        "conv.chr",
                        vec![bin(
                            "+",
                            cap("conv.ord", vec![index(var("cs"), var("i"))]),
                            int(1),
                        )],
                    ),
                    call(
                        "__sh_repeat",
                        vec![s("0"), bin("-", bin("-", var("n"), var("i")), int(1))],
                    ),
                ])),
            ],
        ),
        // __sh_fround(d, keep): `d` rounded to `keep` digits, half to even like
        // C's printf ("2.5" → "2", "3.5" → "4"). The result can be one digit
        // longer when the carry runs off the end ("99" → "100").
        "__sh_fround" => func(
            &["d", "keep"],
            vec![
                decl("n", len(chars(var("d")))),
                // The digit after the kept ones must exist to round on.
                decl("s", var("d")),
                if_(bin("<", var("n"), bin("+", var("keep"), int(1))), vec![set(
                    "s",
                    bin(
                        "+",
                        var("d"),
                        call(
                            "__sh_repeat",
                            vec![s("0"), bin("-", bin("+", var("keep"), int(1)), var("n"))],
                        ),
                    ),
                )], None),
                decl("kept", call("__sh_substr", vec![var("s"), int(0), var("keep"), boolean(true)])),
                decl(
                    "rd",
                    bin(
                        "-",
                        cap("conv.ord", vec![index(chars(var("s")), var("keep"))]),
                        int(48),
                    ),
                ),
                decl("tailnz", boolean(false)),
                decl("k", bin("+", var("keep"), int(1))),
                while_(bin("<", var("k"), len(chars(var("s")))), vec![
                    if_(
                        bin("!=", index(chars(var("s")), var("k")), s("0")),
                        vec![set("tailnz", boolean(true))],
                        None,
                    ),
                    set("k", bin("+", var("k"), int(1))),
                ]),
                decl("up", boolean(false)),
                if_(bin(">", var("rd"), int(5)), vec![set("up", boolean(true))], Some(vec![
                    if_(bin("==", var("rd"), int(5)), vec![
                        if_(var("tailnz"), vec![set("up", boolean(true))], Some(vec![
                            // Exactly half: keep the last digit even.
                            decl("last", int(0)),
                            if_(
                                bin(">", var("keep"), int(0)),
                                vec![set(
                                    "last",
                                    bin(
                                        "-",
                                        cap(
                                            "conv.ord",
                                            vec![index(chars(var("kept")), bin("-", var("keep"), int(1)))],
                                        ),
                                        int(48),
                                    ),
                                )],
                                None,
                            ),
                            if_(
                                bin("==", bin("%", var("last"), int(2)), int(1)),
                                vec![set("up", boolean(true))],
                                None,
                            ),
                        ])),
                    ], None),
                ])),
                if_(var("up"), vec![ret(call("__sh_finc", vec![var("kept")]))], None),
                ret(var("kept")),
            ],
        ),
        // __sh_fztrim(s): drop trailing fractional zeros and a bare trailing
        // point, as `%g` does ("100.000" → "100", "1.500000" → "1.5"). A string
        // with no point is left alone, so "100" stays "100".
        "__sh_fztrim" => func(
            &["s"],
            vec![
                decl("cs", chars(var("s"))),
                decl("dot", boolean(false)),
                for_("c", var("cs"), vec![
                    if_(bin("==", var("c"), s(".")), vec![set("dot", boolean(true))], None),
                ]),
                if_(not(var("dot")), vec![ret(var("s"))], None),
                decl("k", len(var("cs"))),
                while_(bin(">", var("k"), int(0)), vec![
                    if_(
                        bin("==", index(var("cs"), bin("-", var("k"), int(1))), s("0")),
                        vec![set("k", bin("-", var("k"), int(1)))],
                        Some(vec![S::Break { meta: m() }]),
                    ),
                ]),
                if_(
                    bin("and", bin(">", var("k"), int(0)), bin("==", index(var("cs"), bin("-", var("k"), int(1))), s("."))),
                    vec![set("k", bin("-", var("k"), int(1)))],
                    None,
                ),
                ret(call("__sh_substr", vec![var("s"), int(0), var("k"), boolean(true)])),
            ],
        ),
        // __sh_ffix(d, p, prec): the `%f` body (no sign) for 0.d × 10^p.
        "__sh_ffix" => func(
            &["d", "p", "prec"],
            vec![
                decl("keep", bin("+", var("p"), var("prec"))),
                if_(bin("<", var("keep"), int(0)), vec![
                    // Below half of the last place printed: zero.
                    if_(bin("==", var("prec"), int(0)), vec![ret(s("0"))], None),
                    ret(bin("+", s("0."), call("__sh_repeat", vec![s("0"), var("prec")]))),
                ], None),
                decl("s", call("__sh_fround", vec![var("d"), var("keep")])),
                decl("m", len(chars(var("s")))),
                if_(bin("<", var("m"), bin("+", var("prec"), int(1))), vec![
                    set(
                        "s",
                        bin(
                            "+",
                            call(
                                "__sh_repeat",
                                vec![s("0"), bin("-", bin("+", var("prec"), int(1)), var("m"))],
                            ),
                            var("s"),
                        ),
                    ),
                    set("m", bin("+", var("prec"), int(1))),
                ], None),
                decl(
                    "ip",
                    call(
                        "__sh_substr",
                        vec![var("s"), int(0), bin("-", var("m"), var("prec")), boolean(true)],
                    ),
                ),
                if_(bin("==", var("prec"), int(0)), vec![ret(var("ip"))], None),
                ret(bin(
                    "+",
                    bin("+", var("ip"), s(".")),
                    call(
                        "__sh_substr",
                        vec![
                            var("s"),
                            bin("-", var("m"), var("prec")),
                            var("prec"),
                            boolean(true),
                        ],
                    ),
                )),
            ],
        ),
        // __sh_fexp(d, p, prec, upper, trim): the `%e` body (no sign) for
        // 0.d × 10^p — one digit, the point, `prec` digits, then `e±NN` (at
        // least two exponent digits, `E` for `%E`). `trim` drops trailing
        // mantissa zeros for `%g`.
        "__sh_fexp" => func(
            &["d", "p", "prec", "upper", "trim"],
            vec![
                decl("keep", bin("+", var("prec"), int(1))),
                decl("s", call("__sh_fround", vec![var("d"), var("keep")])),
                decl("e", bin("-", var("p"), int(1))),
                // A carry that grows the mantissa moves the point: 9.99 → 1.00e+01.
                if_(bin(">", len(chars(var("s"))), var("keep")), vec![
                    set(
                        "s",
                        call("__sh_substr", vec![var("s"), int(0), var("keep"), boolean(true)]),
                    ),
                    set("e", bin("+", var("e"), int(1))),
                ], None),
                decl("m", len(chars(var("s")))),
                if_(bin("<", var("m"), var("keep")), vec![
                    set(
                        "s",
                        bin("+", var("s"), call("__sh_repeat", vec![s("0"), bin("-", var("keep"), var("m"))])),
                    ),
                ], None),
                decl("mant", call("__sh_substr", vec![var("s"), int(0), int(1), boolean(true)])),
                if_(bin(">", var("prec"), int(0)), vec![
                    set(
                        "mant",
                        bin(
                            "+",
                            bin("+", var("mant"), s(".")),
                            call(
                                "__sh_substr",
                                vec![var("s"), int(1), var("prec"), boolean(true)],
                            ),
                        ),
                    ),
                ], None),
                if_(var("trim"), vec![set("mant", call("__sh_fztrim", vec![var("mant")]))], None),
                decl("ex", call(
                    "__sh_base",
                    vec![
                        tern(bin("<", var("e"), int(0)), bin("-", int(0), var("e")), var("e")),
                        int(10),
                        boolean(false),
                    ],
                )),
                if_(bin("<", len(chars(var("ex"))), int(2)), vec![
                    set(
                        "ex",
                        bin(
                            "+",
                            call("__sh_repeat", vec![s("0"), bin("-", int(2), len(chars(var("ex"))))]),
                            var("ex"),
                        ),
                    ),
                ], None),
                ret(concat(vec![
                    var("mant"),
                    tern(var("upper"), s("E"), s("e")),
                    tern(bin("<", var("e"), int(0)), s("-"), s("+")),
                    var("ex"),
                ])),
            ],
        ),
        // __sh_float(v, conv, left, zero, plus, space, w, prec, has_prec, at):
        // one float conversion, sign and width padding included.
        "__sh_float" => func(
            &["v", "conv", "left", "zero", "plus", "space", "w", "prec", "has_prec", "at"],
            vec![
                decl("f", call("__sh_fparse", vec![var("v"), var("at")])),
                decl("d", index(var("f"), s("d"))),
                decl("p", index(var("f"), s("p"))),
                decl("pr", tern(var("has_prec"), var("prec"), int(6))),
                decl("upper", bin("or", bin("==", var("conv"), s("E")), bin("==", var("conv"), s("G")))),
                decl("body", s("")),
                if_(
                    bin(
                        "or",
                        bin("or", bin("==", var("conv"), s("f")), bin("==", var("conv"), s("e"))),
                        bin("==", var("conv"), s("E")),
                    ),
                    vec![if_(
                        bin("==", var("conv"), s("f")),
                        vec![set("body", call("__sh_ffix", vec![var("d"), var("p"), var("pr")]))],
                        Some(vec![set(
                            "body",
                            call(
                                "__sh_fexp",
                                vec![var("d"), var("p"), var("pr"), var("upper"), boolean(false)],
                            ),
                        )]),
                    )],
                    Some(vec![
                        // %g / %G: `prec` significant digits, no trailing
                        // zeros; exponent form when the decimal exponent is
                        // below -4 or not less than the precision.
                        decl("x", bin("-", var("p"), int(1))),
                        decl("gp", var("pr")),
                        if_(bin("==", var("gp"), int(0)), vec![set("gp", int(1))], None),
                        if_(
                            bin("or", bin("<", var("x"), int(-4)), bin(">=", var("x"), var("gp"))),
                            vec![set(
                                "body",
                                call(
                                    "__sh_fexp",
                                    vec![
                                        var("d"),
                                        var("p"),
                                        bin("-", var("gp"), int(1)),
                                        var("upper"),
                                        boolean(true),
                                    ],
                                ),
                            )],
                            Some(vec![set(
                                "body",
                                call(
                                    "__sh_fztrim",
                                    vec![call(
                                        "__sh_ffix",
                                        vec![
                                            var("d"),
                                            var("p"),
                                            bin("-", bin("-", var("gp"), int(1)), var("x")),
                                        ],
                                    )],
                                ),
                            )]),
                        ),
                    ]),
                ),
                decl("sign", s("")),
                if_(
                    index(var("f"), s("neg")),
                    vec![set("sign", s("-"))],
                    Some(vec![
                        if_(
                            var("plus"),
                            vec![set("sign", s("+"))],
                            Some(vec![if_(var("space"), vec![set("sign", s(" "))], None)]),
                        ),
                    ]),
                ),
                decl("s", bin("+", var("sign"), var("body"))),
                decl("pad", bin("-", var("w"), len(chars(var("s"))))),
                if_(bin(">", var("pad"), int(0)), vec![
                    if_(
                        var("left"),
                        vec![set(
                            "s",
                            bin("+", var("s"), call("__sh_repeat", vec![s(" "), var("pad")])),
                        )],
                        Some(vec![
                            if_(
                                // Zeros go after the sign (`%08.2f` → `-0003.50`).
                                var("zero"),
                                vec![set(
                                    "s",
                                    bin(
                                        "+",
                                        bin(
                                            "+",
                                            var("sign"),
                                            call("__sh_repeat", vec![s("0"), var("pad")]),
                                        ),
                                        var("body"),
                                    ),
                                )],
                                Some(vec![set(
                                    "s",
                                    bin(
                                        "+",
                                        call("__sh_repeat", vec![s(" "), var("pad")]),
                                        var("s"),
                                    ),
                                )]),
                            ),
                        ]),
                    ),
                ], None),
                ret(var("s")),
            ],
        ),
        // __sh_fmt(v, conv, left, zero, plus, space, width, prec, has_prec, at):
        // one printf conversion.
        "__sh_fmt" => {
            func(
                &["v", "conv", "left", "zero", "plus", "space", "w", "prec", "has_prec", "at"],
                vec![
                    decl("s", s("")),
                    if_(
                        bin("==", var("conv"), s("s")),
                        vec![
                            set("s", call("__sh_str", vec![var("v")])),
                            if_(
                                var("has_prec"),
                                vec![set("s", call("__sh_substr", vec![var("s"), int(0), var("prec"), boolean(true)]))],
                                None,
                            ),
                        ],
                        Some(vec![
                            if_(
                                bin("==", var("conv"), s("c")),
                                vec![
                                    decl("cc", chars(call("__sh_str", vec![var("v")]))),
                                    if_(
                                        bin("==", len(var("cc")), int(0)),
                                        vec![set("s", cap("conv.chr", vec![int(0)]))],
                                        Some(vec![set("s", index(var("cc"), int(0)))]),
                                    ),
                                ],
                                Some(vec![
                                    // d, i, u, x, X, o
                                    decl("n", call("__sh_number", vec![var("v"), var("at")])),
                                    if_(
                                        bin(
                                            "and",
                                            bin("<", var("n"), int(0)),
                                            bin(
                                                "or",
                                                bin("or", bin("==", var("conv"), s("x")), bin("==", var("conv"), s("X")) ),
                                                bin("or", bin("==", var("conv"), s("o")), bin("==", var("conv"), s("u"))),
                                            ),
                                        ),
                                        vec![throw(concat(vec![
                                            s("unsupported: a negative value for `%"), var("conv"), s("` at "), var("at"),
                                        ]))],
                                        None,
                                    ),
                                    decl("neg", bin("<", var("n"), int(0))),
                                    decl(
                                        "mag",
                                        tern(bin("<", var("n"), int(0)), bin("-", int(0), var("n")), var("n")),
                                    ),
                                    decl("digits", s("")),
                                    if_(
                                        bin("==", var("conv"), s("x")),
                                        vec![set("digits", call("__sh_base", vec![var("mag"), int(16), boolean(false)]))],
                                        Some(vec![if_(
                                            bin("==", var("conv"), s("X")),
                                            vec![set("digits", call("__sh_base", vec![var("mag"), int(16), boolean(true)]))],
                                            Some(vec![if_(
                                                bin("==", var("conv"), s("o")),
                                                vec![set("digits", call("__sh_base", vec![var("mag"), int(8), boolean(false)]))],
                                                Some(vec![set("digits", bin("+", s(""), var("mag")))]),
                                            )]),
                                        )]),
                                    ),
                                    // A precision sets the minimum number of digits
                                    // (and `%.0d` of 0 prints nothing).
                                    if_(
                                        var("has_prec"),
                                        vec![
                                            if_(
                                                bin("and", bin("==", var("prec"), int(0)), bin("==", var("n"), int(0))),
                                                vec![set("digits", s(""))],
                                                Some(vec![while_(
                                                    bin("<", len(chars(var("digits"))), var("prec")),
                                                    vec![set("digits", bin("+", s("0"), var("digits")))],
                                                )]),
                                            ),
                                        ],
                                        None,
                                    ),
                                    // Sign, then width padding (zeros after the sign
                                    // unless a precision was given).
                                    decl("sign", s("")),
                                    if_(var("neg"), vec![set("sign", s("-"))], Some(vec![
                                        if_(var("plus"), vec![set("sign", s("+"))], None),
                                    ])),
                                    set("s", bin("+", bin("+", var("sign"), var("digits")), s(""))),
                                    if_(
                                        bin("<", len(chars(var("s"))), var("w")),
                                        vec![
                                            decl("pad", bin("-", var("w"), len(chars(var("s"))))),
                                            if_(
                                                var("left"),
                                                vec![while_(bin(">", var("pad"), int(0)), vec![
                                                    set("s", bin("+", var("s"), s(" "))),
                                                    set("pad", bin("-", var("pad"), int(1))),
                                                ])],
                                                Some(vec![if_(
                                                    bin("and", var("zero"), not(var("has_prec"))),
                                                    vec![while_(bin(">", var("pad"), int(0)), vec![
                                                        set("digits", bin("+", s("0"), var("digits"))),
                                                        set("pad", bin("-", var("pad"), int(1))),
                                                    ]), set("s", bin("+", var("sign"), var("digits")))],
                                                    Some(vec![while_(bin(">", var("pad"), int(0)), vec![
                                                        set("s", bin("+", s(" "), var("s"))),
                                                        set("pad", bin("-", var("pad"), int(1))),
                                                    ])]),
                                                )]),
                                            ),
                                        ],
                                        None,
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    // %s: width padding with spaces only.
                    if_(
                        bin("==", var("conv"), s("s")),
                        vec![
                            decl("pad", bin("-", var("w"), len(chars(var("s"))))),
                            if_(
                                var("left"),
                                vec![while_(bin(">", var("pad"), int(0)), vec![
                                    set("s", bin("+", var("s"), s(" "))),
                                    set("pad", bin("-", var("pad"), int(1))),
                                ])],
                                Some(vec![while_(bin(">", var("pad"), int(0)), vec![
                                    set("s", bin("+", s(" "), var("s"))),
                                    set("pad", bin("-", var("pad"), int(1))),
                                ])]),
                            ),
                        ],
                        None,
                    ),
                    ret(var("s")),
                ],
            )
        }
        // __sh_strcmp(a, b): -1/0/1 by Unicode scalar value (the byte order
        // bash and zsh use for `[[ a < b ]]` under a UTF-8 locale).
        "__sh_strcmp" => func(
            &["a", "b"],
            vec![
                decl("ca", chars(var("a"))),
                decl("cb", chars(var("b"))),
                decl("i", int(0)),
                while_(
                    bin("and", bin("<", var("i"), len(var("ca"))), bin("<", var("i"), len(var("cb")))),
                    vec![
                        if_(
                            bin("!=", index(var("ca"), var("i")), index(var("cb"), var("i"))),
                            vec![ret(bin(
                                "-",
                                cap("conv.ord", vec![index(var("ca"), var("i"))]),
                                cap("conv.ord", vec![index(var("cb"), var("i"))]),
                            ))],
                            None,
                        ),
                        set("i", bin("+", var("i"), int(1))),
                    ],
                ),
                ret(bin("-", len(var("ca")), len(var("cb")))),
            ],
        ),
        // __sh_pow(b, e, at): integer `**`. bash rejects a negative exponent;
        // an overflow is reported by the VM's checked multiply.
        "__sh_pow" => func(
            &["b", "e", "at"],
            vec![
                if_(
                    bin("<", var("e"), int(0)),
                    vec![throw(concat(vec![s("unsupported: negative exponent `"), var("e"), s("` in `**` at "), var("at")]))],
                    None,
                ),
                decl("r", int(1)),
                decl("base", var("b")),
                decl("n", var("e")),
                while_(
                    bin(">", var("n"), int(0)),
                    vec![
                        if_(bin("==", bin("%", var("n"), int(2)), int(1)), vec![set("r", bin("*", var("r"), var("base")))], None),
                        set("n", bin("/", var("n"), int(2))),
                        if_(bin(">", var("n"), int(0)), vec![set("base", bin("*", var("base"), var("base")))], None),
                    ],
                ),
                ret(var("r")),
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
                            // A glob character stays in the field: `fields`
                            // expands the resulting words against the filesystem.
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
        other => unreachable!("unknown shell helper {other}"),
    }
}
