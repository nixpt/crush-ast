//! CRUSH-202: zsh walker output vs real zsh, over the shared shell corpus
//! (the cases live with the bash walker's copy of this test).

include!("../../crush-lang-bash/tests/oracle/cases.rs");

#[test]
fn zsh_corpus_matches_zsh_or_fails_loudly() {
    run_corpus(&Oracle {
        dialect: "zsh",
        shell: "/usr/bin/zsh",
        shell_args: &["-f"],
        to_cast: crush_lang_zsh::zsh_to_cast,
    });
}
