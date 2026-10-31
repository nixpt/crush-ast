//! Broad end-to-end check: a whole script (functions, arrays, `[[ ]]`, `case`
//! globs, `${…}` operators, `printf` formatting, exit status) must produce
//! exactly what the real bash produces.

include!("oracle/cases.rs");

#[test]
fn bash_runs_the_kitchen_sink_script_like_bash() {
    check_script_file(
        &Oracle {
            dialect: "bash",
            shell: "/usr/bin/bash",
            shell_args: &["--norc", "--noprofile"],
            to_cast: crush_lang_bash::bash_to_cast,
        },
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scripts/kitchen_sink.sh"),
    );
}
