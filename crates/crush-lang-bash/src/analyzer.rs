use brush_parser::ast::{self, AndOr, AndOrList, Command, CompoundCommand, CompoundList, Pipeline};
use crush_walker_core::FeatureReport;

const DANGEROUS_COMMANDS: &[&str] = &[
    "eval", "exec", "source", ".", "alias", "unset", "trap", "kill",
];

pub fn analyze_program(program: &ast::Program) -> FeatureReport {
    let mut r = FeatureReport::default();
    r.lang = "bash".to_string();

    for complete in &program.complete_commands {
        for item in &complete.0 {
            let and_or = &item.0;
            analyze_and_or_list(and_or, &mut r);
        }
    }

    r.estimated_complexity = program.complete_commands.len();
    r
}

fn analyze_and_or_list(and_or: &AndOrList, r: &mut FeatureReport) {
    analyze_pipeline(&and_or.first, r);
    for a in &and_or.additional {
        match a {
            AndOr::And(p) | AndOr::Or(p) => {
                analyze_pipeline(p, r);
            }
        }
    }
}

fn analyze_pipeline(pipeline: &Pipeline, r: &mut FeatureReport) {
    for cmd in &pipeline.seq {
        analyze_command(cmd, r);
    }
}

fn analyze_command(cmd: &Command, r: &mut FeatureReport) {
    match cmd {
        Command::Simple(simple) => {
            let name = simple
                .word_or_name
                .as_ref()
                .map(|w| w.value.as_str())
                .unwrap_or("");

            if DANGEROUS_COMMANDS.contains(&name) {
                r.dangerous_imports.push(name.to_string());
                r.uses_unsafe = true;
            }

            if let Some(prefix) = &simple.prefix {
                for item in &prefix.0 {
                    if let brush_parser::ast::CommandPrefixOrSuffixItem::AssignmentWord(..) = item {
                        r.has_top_level_side_effects = true;
                    }
                }
            }

            r.estimated_complexity += 1;
        }
        Command::Compound(compound, _) => {
            analyze_compound(compound, r);
        }
        Command::Function(func) => {
            r.uses_functions = true;
            let body = &func.body.0;
            if let CompoundCommand::BraceGroup(group) = body {
                for item in &group.list.0 {
                    analyze_and_or_list(&item.0, r);
                }
            }
        }
        Command::ExtendedTest(..) => {
            r.estimated_complexity += 1;
        }
    }
}

fn analyze_compound(compound: &CompoundCommand, r: &mut FeatureReport) {
    match compound {
        CompoundCommand::IfClause(if_cmd) => {
            analyze_compound_list(&if_cmd.condition, r);
            analyze_compound_list(&if_cmd.then, r);
            if let Some(elses) = &if_cmd.elses {
                for else_clause in elses {
                    if let Some(cond) = &else_clause.condition {
                        analyze_compound_list(cond, r);
                    }
                    analyze_compound_list(&else_clause.body, r);
                }
            }
        }
        CompoundCommand::WhileClause(cmd) | CompoundCommand::UntilClause(cmd) => {
            analyze_compound_list(&cmd.0, r);
            analyze_compound_list(&cmd.1.list, r);
        }
        CompoundCommand::ForClause(for_cmd) => {
            analyze_compound_list(&for_cmd.body.list, r);
        }
        CompoundCommand::BraceGroup(group) => {
            analyze_compound_list(&group.list, r);
        }
        CompoundCommand::Subshell(ss) => {
            analyze_compound_list(&ss.list, r);
        }
        _ => {}
    }
}

fn analyze_compound_list(list: &CompoundList, r: &mut FeatureReport) {
    for item in &list.0 {
        analyze_and_or_list(&item.0, r);
    }
}
