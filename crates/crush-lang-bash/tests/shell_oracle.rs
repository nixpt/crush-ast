//! CRUSH-202: bash walker output vs real bash, over the shared shell corpus.

include!("oracle/cases.rs");

#[test]
fn bash_corpus_matches_bash_or_fails_loudly() {
    run_corpus(&Oracle {
        dialect: "bash",
        shell: "/usr/bin/bash",
        shell_args: &["--norc", "--noprofile"],
        to_cast: crush_lang_bash::bash_to_cast,
    });
}
