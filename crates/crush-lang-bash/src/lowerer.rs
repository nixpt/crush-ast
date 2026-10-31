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
use crush_walker_core::shell::{self, AndOr, Assign, Cmd, Dialect, List, Pipeline, Word};

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
                    first: Pipeline { negated: false, cmd: unsupported("a background command (`&`)", line) },
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
    let cmd = if p.timed.is_some() {
        unsupported("`time`", line)
    } else if p.seq.len() != 1 {
        unsupported("a pipeline (`|`)", line)
    } else {
        command(&p.seq[0], line)
    };
    Pipeline { negated: p.bang, cmd }
}

fn command(c: &Command, line: usize) -> Cmd {
    match c {
        Command::Simple(sc) => simple(sc, line),
        Command::Compound(_, Some(_)) => unsupported("a redirection", line),
        Command::Compound(cc, None) => compound(cc, line),
        Command::Function(fd) => {
            if fd.body.1.is_some() {
                return unsupported("a redirection on a function body", line);
            }
            let body = match &fd.body.0 {
                CompoundCommand::BraceGroup(g) => list(&g.list),
                other => vec![AndOr { first: Pipeline { negated: false, cmd: compound(other, line) }, rest: vec![] }],
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
    for item in sc.prefix.iter().flat_map(|p| p.0.iter()) {
        match item {
            CommandPrefixOrSuffixItem::AssignmentWord(a, _) => match assignment(a) {
                Ok(a) => assigns.push(a),
                Err(c) => return c,
            },
            CommandPrefixOrSuffixItem::Word(w) => words.push(word(w)),
            CommandPrefixOrSuffixItem::IoRedirect(_) => return unsupported("a redirection", line),
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
            CommandPrefixOrSuffixItem::IoRedirect(_) => return unsupported("a redirection", line),
            CommandPrefixOrSuffixItem::ProcessSubstitution(..) => return unsupported("process substitution", line),
        }
    }
    Cmd::Simple { assigns, words, line }
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
