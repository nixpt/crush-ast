//! Broad end-to-end check against real zsh, over the same script the bash
//! walker is checked with (the script lives with the bash crate's tests).

include!("../../crush-lang-bash/tests/oracle/cases.rs");

#[test]
fn zsh_runs_the_kitchen_sink_script_like_zsh() {
    check_script_file(
        &Oracle {
            dialect: "zsh",
            shell: "/usr/bin/zsh",
            shell_args: &["-f"],
            to_cast: crush_lang_zsh::zsh_to_cast,
        },
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../crush-lang-bash/tests/scripts/kitchen_sink.sh"),
    );
}
