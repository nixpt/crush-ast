use crush_walker_core::FeatureReport;
use zshrs_parse::parser::*;

const DANGEROUS_COMMANDS: &[&str] = &[
    "eval", "exec", "source", ".", "alias", "unset", "trap", "kill",
];

pub fn analyze_program(program: &ZshProgram) -> FeatureReport {
    let mut r = FeatureReport::default();
    r.lang = "zsh".to_string();
    for list in &program.lists {
        analyze_sublist(&list.sublist, &mut r);
    }
    r.estimated_complexity = program.lists.len();
    r
}

fn analyze_sublist(sublist: &ZshSublist, r: &mut FeatureReport) {
    analyze_pipe(&sublist.pipe, r);
    if let Some((_op, next)) = &sublist.next {
        analyze_sublist(next, r);
    }
}

fn analyze_pipe(pipe: &ZshPipe, r: &mut FeatureReport) {
    analyze_command(&pipe.cmd, r);
    if let Some(next) = &pipe.next {
        analyze_pipe(next, r);
    }
}

fn analyze_command(cmd: &ZshCommand, r: &mut FeatureReport) {
    match cmd {
        ZshCommand::Simple(simple) => {
            if let Some(name) = simple.words.first() {
                if DANGEROUS_COMMANDS.contains(&name.as_str()) {
                    r.dangerous_imports.push(name.clone());
                    r.uses_unsafe = true;
                }
            }
            if !simple.assigns.is_empty() {
                r.has_top_level_side_effects = true;
            }
            r.estimated_complexity += 1;
        }
        ZshCommand::FuncDef(func) => {
            r.uses_functions = true;
            analyze_program(&func.body);
        }
        ZshCommand::Subsh(prog) | ZshCommand::Cursh(prog) => {
            analyze_program(prog);
        }
        ZshCommand::If(if_cmd) => {
            analyze_program(&if_cmd.cond);
            analyze_program(&if_cmd.then);
            for (elif_cond, elif_body) in &if_cmd.elif {
                analyze_program(elif_cond);
                analyze_program(elif_body);
            }
            if let Some(else_) = &if_cmd.else_ {
                analyze_program(else_);
            }
        }
        ZshCommand::While(w) | ZshCommand::Until(w) => {
            analyze_program(&w.cond);
            analyze_program(&w.body);
        }
        ZshCommand::For(f) => {
            analyze_program(&f.body);
        }
        ZshCommand::Case(case) => {
            for arm in &case.arms {
                analyze_program(&arm.body);
            }
        }
        ZshCommand::Repeat(rep) => {
            analyze_program(&rep.body);
        }
        ZshCommand::Try(try_cmd) => {
            analyze_program(&try_cmd.try_block);
            analyze_program(&try_cmd.always);
        }
        ZshCommand::Time(body) => {
            if let Some(sublist) = body {
                analyze_sublist(sublist, r);
            }
        }
        ZshCommand::Cond(_) | ZshCommand::Arith(_) => {
            r.estimated_complexity += 1;
        }
        ZshCommand::Redirected(cmd_inner, _) => {
            analyze_command(cmd_inner, r);
        }
    }
}
