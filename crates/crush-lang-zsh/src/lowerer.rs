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
use crush_walker_core::shell::{self, AndOr, Assign, Cmd, Dialect, List, Pipeline, Redir, RedirKind, Word};
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
    AndOr { first: Pipeline { negated: false, stages: vec![cmd] }, rest: vec![] }
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
    if coproc {
        return Pipeline { negated, stages: vec![unsupported("`coproc`", line)] };
    }
    let mut stages = Vec::new();
    let mut cur = Some(p);
    while let Some(stage) = cur {
        // `a |& b` merges stderr into the pipe; there is no stderr stream here.
        if stage.merge_stderr && stage.next.is_some() {
            return Pipeline { negated, stages: vec![unsupported("`|&`", line)] };
        }
        stages.push(command(&stage.cmd, line));
        cur = stage.next.as_deref();
    }
    Pipeline { negated, stages }
}

/// One redirection (`<`, `>`, `>>`, `<<`) as the shared shell IR wants it.
fn redir(r: &ZshRedir, line: usize) -> Result<Redir, Cmd> {
    if r.varid.is_some() {
        return Err(unsupported("`{var}>file`", line));
    }
    let kind = match r.rtype {
        RedirType::Write | RedirType::Writenow => RedirKind::Out,
        RedirType::Append | RedirType::Appendnow => RedirKind::Append,
        RedirType::Read => RedirKind::In,
        RedirType::Heredoc | RedirType::HeredocDash => RedirKind::HereDoc,
        RedirType::MergeIn
        | RedirType::MergeOut
        | RedirType::ErrWrite
        | RedirType::ErrWritenow
        | RedirType::ErrAppend
        | RedirType::ErrAppendnow => {
            return Err(unsupported(
                "a redirection that duplicates a file descriptor (`2>&1`)",
                line,
            ))
        }
        other => {
            return Err(unsupported(
                &format!("`{other:?}` (only `<`, `>`, `>>` and here-documents)"),
                line,
            ))
        }
    };
    // -1 is "no fd written", so the default for the kind applies.
    let want = if matches!(kind, RedirKind::In | RedirKind::HereDoc) { 0 } else { 1 };
    if r.fd != want && r.fd != -1 {
        return Err(unsupported(&format!("a redirection of file descriptor {}", r.fd), line));
    }
    if kind == RedirKind::HereDoc {
        let doc = r
            .heredoc
            .as_ref()
            .ok_or_else(|| unsupported("a here-document with no body", line))?;
        let body = if r.rtype == RedirType::HeredocDash {
            doc.content.lines().map(|l| l.trim_start_matches('\t')).collect::<Vec<_>>().join("\n")
        } else {
            doc.content.clone()
        };
        return Ok(Redir {
            kind,
            word: Word { raw: body, line },
            // `<<'EOT'` is literal; `<<EOT` expands `$…`.
            expand: !doc.quoted,
            line,
        });
    }
    Ok(Redir { kind, word: Word { raw: source_text(&r.name), line }, expand: false, line })
}

/// zshrs-parse's `[[ … ]]` tree → the shared shell IR's conditional expression.
fn cond(c: &ZshCond, line: usize) -> Result<shell::Cond, Cmd> {
    Ok(match c {
        ZshCond::Not(x) => shell::Cond::Not(Box::new(cond(x, line)?)),
        ZshCond::And(a, b) => shell::Cond::And(Box::new(cond(a, line)?), Box::new(cond(b, line)?)),
        ZshCond::Or(a, b) => shell::Cond::Or(Box::new(cond(a, line)?), Box::new(cond(b, line)?)),
        ZshCond::Regex(..) => return Err(unsupported("`=~` in `[[ … ]]`", line)),
        // Operators arrive tokenized (`-gt` is DASH + `gt`), so decode them the
        // same way word text is decoded.
        ZshCond::Unary(op, w) => {
            let op = source_text(op);
            match op.as_str() {
                "-z" => shell::Cond::Unary("-z", word(w, line)),
                "-n" => shell::Cond::Unary("-n", word(w, line)),
                // The three the capability layer can answer (`fs.exists`,
                // `fs.is_dir`). `-f` is "exists and not a directory" there.
                "-e" => shell::Cond::Unary("-e", word(w, line)),
                "-f" => shell::Cond::Unary("-f", word(w, line)),
                "-d" => shell::Cond::Unary("-d", word(w, line)),
                other => return Err(unsupported(&format!("`{other}` in `[[ … ]]` (file and shell-option tests)"), line)),
            }
        }
        ZshCond::Binary(l, op, r) => {
            let op = match source_text(op).as_str() {
                "=" | "==" => "==",
                "!=" => "!=",
                "<" => "<",
                ">" => ">",
                "-eq" => "-eq",
                "-ne" => "-ne",
                "-lt" => "-lt",
                "-le" => "-le",
                "-gt" => "-gt",
                "-ge" => "-ge",
                other => return Err(unsupported(&format!("`{other}` in `[[ … ]]` (regex and file tests)"), line)),
            };
            shell::Cond::Binary(op, word(l, line), word(r, line))
        }
    })
}

fn command(c: &ZshCommand, line: usize) -> Cmd {
    match c {
        ZshCommand::Simple(s) => {
            let mut redirs = Vec::new();
            for r in &s.redirs {
                match redir(r, line) {
                    Ok(x) => redirs.push(x),
                    Err(c) => return c,
                }
            }
            let mut assigns = Vec::new();
            for a in &s.assigns {
                // `a[1]=x` arrives with the subscript still in the name, and
                // the brackets as lexer markers.
                let decoded = source_text(&a.name);
                let (name, index) = match decoded.find('[') {
                    Some(i) if decoded.ends_with(']') && i < decoded.len() - 1 => (
                        decoded[..i].to_string(),
                        Some(Word { raw: decoded[i + 1..decoded.len() - 1].to_string(), line }),
                    ),
                    _ => (decoded, None),
                };
                let (array, value) = match &a.value {
                    ZshAssignValue::Scalar(v) => (None, word(v, line)),
                    ZshAssignValue::Array(items) => (
                        Some(items.iter().map(|v| word(v, line)).collect()),
                        Word { raw: String::new(), line },
                    ),
                };
                assigns.push(Assign { name, index, array, value, append: a.append, line });
            }
            let simple = Cmd::Simple { assigns, words: s.words.iter().map(|w| word(w, line)).collect(), line };
            if redirs.is_empty() {
                simple
            } else {
                Cmd::Redirected { inner: Box::new(simple), redirs, line }
            }
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
        ZshCommand::Cond(c) => match cond(c, line) {
            Ok(cond) => Cmd::ExtendedTest { cond, line },
            Err(unsupported) => unsupported,
        },
        ZshCommand::Arith(e) => Cmd::Arith { expr: untokenize(e), line },
        ZshCommand::Try(_) => unsupported("zsh `{ … } always { … }`", line),
        ZshCommand::Redirected(inner, rs) => {
            let mut redirs = Vec::new();
            for r in rs {
                match redir(r, line) {
                    Ok(x) => redirs.push(x),
                    Err(c) => return c,
                }
            }
            let inner = command(inner, line);
            Cmd::Redirected { inner: Box::new(inner), redirs, line }
        }
    }
}
