//! zshrs-parse AST → the shared shell IR → CAST.
//!
//! All semantics live in `crush_walker_core::shell` (shared with the bash
//! walker); this file maps zshrs-parse's nodes onto that IR. Constructs the IR
//! has no form for become `Cmd::Unsupported`, reported as
//! `unsupported: <construct> at line N` (CRUSH-202).
//!
//! zshrs-parse hands words over in zsh's lexer form: quote characters are
//! replaced by marker tokens (Snull `'`, Dnull `"`, Bnull `\`) and the
//! other special characters by their own tokens. [`source_text`] turns that
//! back into the word as written, quotes included, so the shared word parser
//! sees the same text the bash walker gets from brush. (The old lowerer used
//! `untokenize`, which drops the quote markers — which is why zsh expanded
//! `$x` inside single quotes.)

use crush_cast::Program;
use crush_walker_core::LowerCtx;
use crush_walker_core::shell::{self, AndOr, Assign, Cmd, Dialect, List, Pipeline, Word};
use zshrs_parse::lexer::untokenize;
use zshrs_parse::parser::*;

pub fn lower_program(program: &ZshProgram, _ctx: &LowerCtx) -> anyhow::Result<Program> {
    shell::lower_script(Dialect::Zsh, &list(program, 0))
}

/// A tokenized zsh word back as source text.
fn source_text(w: &str) -> String {
    let mut out = String::new();
    for c in w.chars() {
        match c {
            '\u{9d}' => out.push('\''),
            '\u{9e}' => out.push('"'),
            '\u{9f}' => out.push('\\'),
            c if ('\u{83}'..='\u{a1}').contains(&c) => out.push_str(&untokenize(&c.to_string())),
            c => out.push(c),
        }
    }
    out
}

fn word(w: &str, line: usize) -> Word {
    Word { raw: source_text(w), line }
}

fn unsupported(what: &str, line: usize) -> Cmd {
    Cmd::Unsupported { what: what.to_string(), line }
}

fn single(cmd: Cmd) -> AndOr {
    AndOr { first: Pipeline { negated: false, cmd }, rest: vec![] }
}

fn list(p: &ZshProgram, outer_line: usize) -> List {
    p.lists
        .iter()
        .map(|l| {
            if l.flags.async_ || l.flags.disown {
                let line = (l.sublist.pipe.lineno as usize).max(outer_line);
                single(unsupported("a background command (`&`)", line))
            } else {
                sublist(&l.sublist)
            }
        })
        .collect()
}

fn sublist(s: &ZshSublist) -> AndOr {
    let mut out = AndOr { first: pipe(&s.pipe, s.flags.not, s.flags.coproc), rest: vec![] };
    let mut next = &s.next;
    while let Some((op, nxt)) = next {
        out.rest.push((matches!(op, SublistOp::And), pipe(&nxt.pipe, nxt.flags.not, nxt.flags.coproc)));
        next = &nxt.next;
    }
    out
}

fn pipe(p: &ZshPipe, negated: bool, coproc: bool) -> Pipeline {
    let line = p.lineno as usize;
    let cmd = if coproc {
        unsupported("`coproc`", line)
    } else if p.next.is_some() {
        unsupported("a pipeline (`|`)", line)
    } else {
        command(&p.cmd, line)
    };
    Pipeline { negated, cmd }
}

fn command(c: &ZshCommand, line: usize) -> Cmd {
    match c {
        ZshCommand::Simple(s) => {
            if !s.redirs.is_empty() {
                return unsupported("a redirection", line);
            }
            let mut assigns = Vec::new();
            for a in &s.assigns {
                match &a.value {
                    ZshAssignValue::Scalar(v) => {
                        assigns.push(Assign { name: a.name.clone(), value: word(v, line), append: a.append, line })
                    }
                    ZshAssignValue::Array(_) => return unsupported(&format!("array assignment `{}=(…)`", a.name), line),
                }
            }
            Cmd::Simple { assigns, words: s.words.iter().map(|w| word(w, line)).collect(), line }
        }
        ZshCommand::Subsh(_) => unsupported("a subshell `( … )`", line),
        ZshCommand::Cursh(p) => Cmd::Group(list(p, line)),
        ZshCommand::For(f) => {
            if f.is_select {
                return unsupported("`select`", line);
            }
            let body = list(&f.body, line);
            match &f.list {
                ForList::Words(ws) => {
                    Cmd::For { var: f.var.clone(), words: Some(ws.iter().map(|w| word(w, line)).collect()), body, line }
                }
                ForList::Positional => Cmd::For { var: f.var.clone(), words: None, body, line },
                ForList::CStyle { init, cond, step } => {
                    let part = |s: &String| Some(untokenize(s)).filter(|t| !t.trim().is_empty());
                    Cmd::ArithFor { init: part(init), cond: part(cond), step: part(step), body, line }
                }
            }
        }
        ZshCommand::Case(c) => {
            let mut arms = Vec::new();
            for arm in &c.arms {
                if !matches!(arm.terminator, CaseTerm::Break) {
                    return unsupported("`;&` / `;|` in `case`", line);
                }
                arms.push((arm.patterns.iter().map(|p| word(p, line)).collect(), list(&arm.body, line)));
            }
            Cmd::Case { word: word(&c.word, line), arms, line }
        }
        ZshCommand::If(i) => {
            let mut branches = vec![(list(&i.cond, line), list(&i.then, line))];
            for (c, t) in &i.elif {
                branches.push((list(c, line), list(t, line)));
            }
            Cmd::If { branches, else_body: i.else_.as_ref().map(|e| list(e, line)), line }
        }
        ZshCommand::While(w) | ZshCommand::Until(w) => Cmd::Loop {
            cond: list(&w.cond, line),
            body: list(&w.body, line),
            until: matches!(c, ZshCommand::Until(_)) || w.until,
            line,
        },
        ZshCommand::Repeat(_) => unsupported("zsh `repeat`", line),
        ZshCommand::FuncDef(f) => {
            if f.auto_call_args.is_some() {
                return unsupported("an anonymous function", line);
            }
            match f.names.as_slice() {
                [name] => Cmd::FuncDef { name: name.clone(), body: list(&f.body, line), line },
                _ => unsupported("a function with several names", line),
            }
        }
        ZshCommand::Time(_) => unsupported("`time`", line),
        ZshCommand::Cond(_) => unsupported("`[[ … ]]` (use `[ … ]`)", line),
        ZshCommand::Arith(e) => Cmd::Arith { expr: untokenize(e), line },
        ZshCommand::Try(_) => unsupported("zsh `{ … } always { … }`", line),
        ZshCommand::Redirected(..) => unsupported("a redirection", line),
    }
}
