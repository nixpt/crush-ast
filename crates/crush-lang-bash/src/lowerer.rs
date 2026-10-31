//! brush-parser AST → the shared shell IR → CAST.
//!
//! All semantics live in `crush_walker_core::shell` (shared with the zsh
//! walker). This file only maps brush's node types onto that IR, turning the
//! nodes the IR has no form for (pipelines, redirections, subshells, `[[ ]]`,
//! …) into `Cmd::Unsupported`, which lowering reports as
//! `unsupported: <construct> at line N` (CRUSH-202).

use brush_parser::ast::{
    self, AndOr as BAndOr, AndOrList, CaseItemPostAction, Command, CommandPrefixOrSuffixItem, CompoundCommand,
    CompoundList, SeparatorOperator, SourceLocation,
};
use crush_cast::Program;
use crush_walker_core::LowerCtx;
use crush_walker_core::shell::{self, AndOr, Assign, Cmd, Dialect, List, Pipeline, Redir, RedirKind, Word};

/// Lower a brush-parser Program to a CAST Program.
pub fn lower_program(program: ast::Program, _ctx: &LowerCtx) -> anyhow::Result<Program> {
    let mut script = List::new();
    for complete in &program.complete_commands {
        script.extend(list(complete));
    }
    shell::lower_script(Dialect::Bash, &script)
}

fn line_of<T: SourceLocation>(node: &T) -> usize {
    node.location().map(|l| l.start.line).unwrap_or(0)
}

fn word(w: &ast::Word) -> Word {
    Word { raw: w.value.clone(), line: line_of(w) }
}

fn unsupported(what: &str, line: usize) -> Cmd {
    Cmd::Unsupported { what: what.to_string(), line }
}

fn list(cl: &CompoundList) -> List {
    cl.0.iter()
        .map(|item| {
            if matches!(item.1, SeparatorOperator::Async) {
                let line = line_of(&item.0);
                AndOr {
                    first: Pipeline {
                        negated: false,
                        stages: vec![unsupported("a background command (`&`)", line)],
                    },
                    rest: vec![],
                }
            } else {
                and_or(&item.0)
            }
        })
        .collect()
}

fn and_or(a: &AndOrList) -> AndOr {
    AndOr {
        first: pipeline(&a.first),
        rest: a
            .additional
            .iter()
            .map(|x| match x {
                BAndOr::And(p) => (true, pipeline(p)),
                BAndOr::Or(p) => (false, pipeline(p)),
            })
            .collect(),
    }
}

fn pipeline(p: &ast::Pipeline) -> Pipeline {
    let line = line_of(p);
    if p.timed.is_some() {
        return Pipeline { negated: p.bang, stages: vec![unsupported("`time`", line)] };
    }
    Pipeline { negated: p.bang, stages: p.seq.iter().map(|c| command(c, line)).collect() }
}

/// One redirection (`<`, `>`, `>>`, `<<`) as the shared shell IR wants it.
fn redir(r: &ast::IoRedirect, line: usize) -> Result<Redir, Cmd> {
    use ast::{IoFileRedirectKind as K, IoFileRedirectTarget as T, IoRedirect as R};
    let fd_ok = |fd: &Option<ast::IoFd>, want: i32| match fd {
        None => Ok(()),
        Some(n) if *n == want => Ok(()),
        Some(n) => Err(unsupported(&format!("a redirection of file descriptor {n}"), line)),
    };
    match r {
        R::File(fd, kind, target) => {
            let (rkind, want) = match kind {
                K::Read => (RedirKind::In, 0),
                K::Write | K::Clobber => (RedirKind::Out, 1),
                K::Append => (RedirKind::Append, 1),
                K::DuplicateInput | K::DuplicateOutput => {
                    return Err(unsupported(
                        "a redirection that duplicates a file descriptor (`2>&1`)",
                        line,
                    ))
                }
                other => {
                    return Err(unsupported(
                        &format!("`{other}` (only `<`, `>`, `>>` and here-documents)"),
                        line,
                    ))
                }
            };
            fd_ok(fd, want)?;
            match target {
                T::Filename(w) => Ok(Redir { kind: rkind, word: word(w), expand: false, line }),
                _ => Err(unsupported("a redirection onto a file descriptor or process substitution", line)),
            }
        }
        R::HereDocument(fd, doc) => {
            fd_ok(fd, 0)?;
            let body = if doc.remove_tabs {
                doc.doc.value.lines().map(|l| l.trim_start_matches('\t')).collect::<Vec<_>>().join("\n")
            } else {
                doc.doc.value.clone()
            };
            Ok(Redir {
                kind: RedirKind::HereDoc,
                word: Word { raw: body, line },
                // `<<'EOT'` is literal; `<<EOT` expands `$…`.
                expand: doc.requires_expansion,
                line,
            })
        }
        R::HereString(..) => Err(unsupported("a here-string `<<<`", line)),
        R::OutputAndError(..) => Err(unsupported("`&>` (stdout and stderr together)", line)),
    }
}

fn command(c: &Command, line: usize) -> Cmd {
    match c {
        Command::Simple(sc) => simple(sc, line),
        Command::Compound(cc, redirs) => {
            let inner = compound(cc, line);
            match redirs {
                None => inner,
                Some(rs) => {
                    let mut out = Vec::new();
                    for r in &rs.0 {
                        match redir(r, line) {
                            Ok(x) => out.push(x),
                            Err(c) => return c,
                        }
                    }
                    Cmd::Redirected { inner: Box::new(inner), redirs: out, line }
                }
            }
        }
        Command::Function(fd) => {
            if fd.body.1.is_some() {
                return unsupported("a redirection on a function body", line);
            }
            let body = match &fd.body.0 {
                CompoundCommand::BraceGroup(g) => list(&g.list),
                other => vec![AndOr {
                    first: Pipeline { negated: false, stages: vec![compound(other, line)] },
                    rest: vec![],
                }],
            };
            Cmd::FuncDef { name: fd.fname.value.clone(), body, line }
        }
        Command::ExtendedTest(t, _) => match cond(&t.expr, line) {
            Ok(cond) => Cmd::ExtendedTest { cond, line },
            Err(unsupported) => unsupported,
        },
    }
}

/// brush's `[[ … ]]` tree → the shared shell IR's conditional expression.
fn cond(e: &ast::ExtendedTestExpr, line: usize) -> Result<shell::Cond, Cmd> {
    use ast::{BinaryPredicate as BP, ExtendedTestExpr as X, UnaryPredicate as UP};
    Ok(match e {
        X::And(a, b) => shell::Cond::And(Box::new(cond(a, line)?), Box::new(cond(b, line)?)),
        X::Or(a, b) => shell::Cond::Or(Box::new(cond(a, line)?), Box::new(cond(b, line)?)),
        X::Not(x) => shell::Cond::Not(Box::new(cond(x, line)?)),
        X::Parenthesized(x) => cond(x, line)?,
        X::UnaryTest(p, w) => match p {
            UP::StringHasZeroLength => shell::Cond::Unary("-z", word(w)),
            UP::StringHasNonZeroLength => shell::Cond::Unary("-n", word(w)),
            other => return Err(unsupported(&format!("`{other}` in `[[ … ]]` (file and shell-option tests)"), line)),
        },
        X::BinaryTest(p, l, r) => {
            let op = match p {
                BP::StringExactlyMatchesPattern | BP::StringExactlyMatchesString => "==",
                BP::StringDoesNotExactlyMatchPattern | BP::StringDoesNotExactlyMatchString => "!=",
                BP::LeftSortsBeforeRight => "<",
                BP::LeftSortsAfterRight => ">",
                BP::ArithmeticEqualTo => "-eq",
                BP::ArithmeticNotEqualTo => "-ne",
                BP::ArithmeticLessThan => "-lt",
                BP::ArithmeticLessThanOrEqualTo => "-le",
                BP::ArithmeticGreaterThan => "-gt",
                BP::ArithmeticGreaterThanOrEqualTo => "-ge",
                other => return Err(unsupported(&format!("`{other}` in `[[ … ]]` (regex and file tests)"), line)),
            };
            shell::Cond::Binary(op, word(l), word(r))
        }
    })
}

fn simple(sc: &ast::SimpleCommand, line: usize) -> Cmd {
    let line = sc.word_or_name.as_ref().map(line_of).filter(|l| *l > 0).unwrap_or(line);
    let mut assigns = Vec::new();
    let mut words = Vec::new();
    let mut redirs = Vec::new();
    for item in sc.prefix.iter().flat_map(|p| p.0.iter()) {
        match item {
            CommandPrefixOrSuffixItem::AssignmentWord(a, _) => match assignment(a) {
                Ok(a) => assigns.push(a),
                Err(c) => return c,
            },
            CommandPrefixOrSuffixItem::Word(w) => words.push(word(w)),
            CommandPrefixOrSuffixItem::IoRedirect(r) => match redir(r, line) {
                Ok(r) => redirs.push(r),
                Err(c) => return c,
            },
            CommandPrefixOrSuffixItem::ProcessSubstitution(..) => return unsupported("process substitution", line),
        }
    }
    if let Some(w) = &sc.word_or_name {
        words.push(word(w));
    }
    for item in sc.suffix.iter().flat_map(|s| s.0.iter()) {
        match item {
            // `local x=1`, `export x=1`: the builtin parses `name=value` itself.
            CommandPrefixOrSuffixItem::AssignmentWord(_, w) | CommandPrefixOrSuffixItem::Word(w) => words.push(word(w)),
            CommandPrefixOrSuffixItem::IoRedirect(r) => match redir(r, line) {
                Ok(r) => redirs.push(r),
                Err(c) => return c,
            },
            CommandPrefixOrSuffixItem::ProcessSubstitution(..) => return unsupported("process substitution", line),
        }
    }
    let simple = Cmd::Simple { assigns, words, line };
    if redirs.is_empty() {
        simple
    } else {
        Cmd::Redirected { inner: Box::new(simple), redirs, line }
    }
}

fn assignment(a: &ast::Assignment) -> Result<Assign, Cmd> {
    let line = line_of(a);
    let (name, index) = match &a.name {
        ast::AssignmentName::VariableName(n) => (n.clone(), None),
        ast::AssignmentName::ArrayElementName(n, sub) => (n.clone(), Some(Word { raw: sub.clone(), line })),
    };
    match &a.value {
        ast::AssignmentValue::Scalar(w) => Ok(Assign { name, index, array: None, value: word(w), append: a.append, line }),
        ast::AssignmentValue::Array(items) => {
            if let Some((i, _)) = items.iter().find(|(i, _)| i.is_some()) {
                let at = i.as_ref().map(|w| w.value.clone()).unwrap_or_default();
                return Err(unsupported(&format!("an indexed array literal `{name}=([{at}]=…)`"), line));
            }
            Ok(Assign {
                name,
                index,
                array: Some(items.iter().map(|(_, w)| word(w)).collect()),
                value: Word { raw: String::new(), line },
                append: a.append,
                line,
            })
        }
    }
}

fn compound(cc: &CompoundCommand, line: usize) -> Cmd {
    let line = match line_of(cc) {
        0 => line,
        l => l,
    };
    match cc {
        CompoundCommand::Arithmetic(a) => Cmd::Arith { expr: a.expr.value.clone(), line },
        CompoundCommand::ArithmeticForClause(f) => Cmd::ArithFor {
            init: f.initializer.as_ref().map(|e| e.value.clone()),
            cond: f.condition.as_ref().map(|e| e.value.clone()),
            step: f.updater.as_ref().map(|e| e.value.clone()),
            body: list(&f.body.list),
            line,
        },
        CompoundCommand::BraceGroup(g) => Cmd::Group(list(&g.list)),
        CompoundCommand::Subshell(_) => unsupported("a subshell `( … )`", line),
        CompoundCommand::ForClause(f) => Cmd::For {
            var: f.variable_name.clone(),
            words: f.values.as_ref().map(|ws| ws.iter().map(word).collect()),
            body: list(&f.body.list),
            line,
        },
        CompoundCommand::CaseClause(c) => {
            let mut arms = Vec::new();
            for item in &c.cases {
                if !matches!(item.post_action, CaseItemPostAction::ExitCase) {
                    return unsupported("`;&` / `;;&` in `case`", line);
                }
                let body = item.cmd.as_ref().map(list).unwrap_or_default();
                arms.push((item.patterns.iter().map(word).collect(), body));
            }
            Cmd::Case { word: word(&c.value), arms, line }
        }
        CompoundCommand::IfClause(i) => {
            let mut branches = vec![(list(&i.condition), list(&i.then))];
            let mut else_body = None;
            for e in i.elses.iter().flatten() {
                match &e.condition {
                    Some(cond) => branches.push((list(cond), list(&e.body))),
                    None => else_body = Some(list(&e.body)),
                }
            }
            Cmd::If { branches, else_body, line }
        }
        CompoundCommand::WhileClause(w) => Cmd::Loop { cond: list(&w.0), body: list(&w.1.list), until: false, line },
        CompoundCommand::UntilClause(w) => Cmd::Loop { cond: list(&w.0), body: list(&w.1.list), until: true, line },
        CompoundCommand::Coprocess(_) => unsupported("`coproc`", line),
    }
}
