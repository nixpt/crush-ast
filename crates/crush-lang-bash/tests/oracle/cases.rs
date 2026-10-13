// CRUSH-202: the bash/zsh oracle corpus. Included by
// `crush-lang-bash/tests/shell_oracle.rs` and `crush-lang-zsh/tests/shell_oracle.rs`.
//
// Every case runs through the crush pipeline (walker → CAST → CASM → CVM1)
// and through the real shell. A case either MATCHES (same stdout, same exit
// status) or is expected to be refused LOUDLY: crush fails with an error that
// contains the given text, at compile time or at run time. A crush run that
// succeeds with output different from the shell's is always a failure — that
// is the bug class this corpus exists to keep out.

use std::io::Write as _;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub enum X {
    Match,
    /// Crush must fail with an error containing this text.
    Loud(&'static str),
}

pub struct Case {
    pub name: &'static str,
    pub src: &'static str,
    pub bash: X,
    pub zsh: X,
}

const fn ok(name: &'static str, src: &'static str) -> Case {
    Case { name, src, bash: X::Match, zsh: X::Match }
}
const fn loud(name: &'static str, src: &'static str, needle: &'static str) -> Case {
    Case { name, src, bash: X::Loud(needle), zsh: X::Loud(needle) }
}
const fn per(name: &'static str, src: &'static str, bash: X, zsh: X) -> Case {
    Case { name, src, bash, zsh }
}

pub const CASES: &[Case] = &[
    // ── echo, quoting, word splitting ──
    ok("echo_words", "echo a b\n"),
    ok("echo_quoted_spaces", "echo \"a   b\" c\n"),
    ok("echo_n", "echo -n hi\necho there\n"),
    ok("echo_empty", "echo\n"),
    ok("single_quotes_literal", "x=5\necho '$x is literal'\n"),
    ok("double_quotes_expand", "x=5\necho \"x=$x and \\$x\"\n"),
    ok("braced_var", "x=ab\necho \"${x}c\"\n"),
    ok("quotes_nested", "echo \"it's\" 'say \"hi\"'\n"),
    ok("backslash_escapes", "echo a\\ b \\$x\n"),
    ok("unquoted_concat", "a=1\nb=2\necho $a$b\n"),
    ok("word_splitting", "x=\"a   b\"\necho $x\n"),
    ok("empty_unquoted_dropped", "e=\"\"\necho x $e y\n"),
    ok("append_assign", "s=ab\ns+=cd\necho $s\n"),
    ok("unset_var", "x=1\nunset x\necho \"[$x]\"\n"),
    per("mixed_quote_split", "x=\"1 2\"\necho \"a\"$x\n", X::Loud("mixing quoted text"), X::Match),
    per("glob_from_variable", "x='*'\necho $x\n", X::Loud("unsupported: pathname expansion"), X::Match),
    per("zsh_echo_backslash", "echo 'a\\nb'\n", X::Match, X::Loud("backslash")),
    loud("glob_literal", "echo *\n", "pathname expansion"),
    loud("tilde", "echo ~\n", "tilde expansion"),
    loud("brace_expansion", "echo {a,b}\n", "brace expansion"),
    loud("echo_e", "echo -e \"a\\tb\"\n", "`echo -e`"),
    loud("env_var", "echo $HOME\n", "never assigned"),
    loud("unset_never_assigned", "echo \"[$nope]\"\n", "never assigned"),
    per("zsh_subscript", "x=hello\necho \"$x[1]\"\n", X::Match, X::Loud("zsh subscript")),
    // ── printf ──
    ok("printf_basic", "printf \"%s=%d\\n\" a 5\nprintf \"x%sy\\n\" 1 2\nprintf 'no newline'\necho\n"),
    ok("printf_percent", "printf \"100%%\\n\"\n"),
    loud("printf_float", "printf \"%f\\n\" 1.5\n", "`%f`"),
    // ── test / [ ──
    ok("test_gt_true", "x=5\nif [ $x -gt 3 ]; then echo big; else echo small; fi\n"),
    ok("test_gt_false_4args", "x=15\nif [ $x -gt 20 ]; then echo big; else echo small; fi\n"),
    ok(
        "test_int_ops",
        "a=3\nb=5\n[ $a -eq 3 ] && echo eq\n[ $a -ne $b ] && echo ne\n[ $b -ge 5 ] && echo ge\n[ $a -lt $b ] && echo lt\n[ $a -le 2 ] || echo not-le\ntest $b -gt $a && echo test-gt\n[ 007 -eq 7 ] && echo leading-zero\n",
    ),
    ok(
        "test_strings",
        "s=foo\n[ \"$s\" = foo ] && echo eq\n[ \"$s\" != bar ] && echo ne\n[ -z \"\" ] && echo z\n[ -n \"$s\" ] && echo n\n[ \"$s\" ] && echo nonempty\n[ \"\" ] || echo empty\n[ ] || echo noargs\n",
    ),
    ok(
        "test_not_and_nesting",
        "x=1\n[ ! \"$x\" = 2 ] && echo not2\n! [ \"$x\" = 1 ] || echo is1\n[ \\( \"$x\" \\) ] && echo paren\n[ ! -z \"$x\" ] && echo notz\n[ \"$x\" -a \"\" ] || echo and-false\n[ \"$x\" -o \"\" ] && echo or-true\n",
    ),
    loud("test_non_integer", "x=abc\n[ $x -gt 1 ] && echo yes\necho after\n", "non-integer value"),
    loud("test_file_operator", "[ -f /etc/passwd ] && echo f\n", "test operator `-f`"),
    loud("test_empty_unquoted", "e=\nif [ $e = x ]; then echo y; fi\necho done\n", "words where one is needed"),
    loud("test_5_args", "[ 1 = 1 -a 2 = 2 ] && echo y\n", "more than 4 arguments"),
    loud("double_bracket", "x=1\n[[ $x == 1 ]] && echo y\n", "`[[ … ]]`"),
    // ── arithmetic ──
    ok("arith_basic", "a=7\necho $((a+1)) $(( a * 2 - 3 )) $((a/2)) $((a%3)) $((-a))\n"),
    ok("arith_precedence", "echo $((2+3*4)) $(((2+3)*4)) $((10-2-3)) $((2*3%4)) $((-7/2)) $((-7%3))\n"),
    ok("arith_compare_logic", "a=3\necho $((a>2)) $((a==4)) $((a>1 && a<3)) $((!a)) $((a ? 10 : 20)) $((a<2 || a>2))\n"),
    ok("arith_dollar_vars", "a=4\nb=5\necho $(( $a + $b )) \"$((a*b))\"\n"),
    ok("arith_command_and_increment", "i=0\n(( i++ ))\necho $i\n(( i += 5 ))\necho $i\n(( i > 3 )) && echo big\n(( ++i ))\necho $i\n"),
    ok("arith_zero_status", "i=0\n(( i )) || echo zero\n"),
    ok("arith_unset_is_zero", "x=\necho $((x + 2))\n"),
    ok("length", "s=hello\necho ${#s}\nu=h\u{e9}llo\necho ${#u}\n"),
    loud("arith_octal", "echo $((010))\n", "octal"),
    loud("arith_power", "echo $((2**3))\n", "`**`"),
    loud("arith_assign_in_expansion", "x=1\necho $((x+=1))\n", "assignment"),
    loud("arith_non_integer_var", "a=\"1+2\"\necho $((a*2))\n", "non-integer value"),
    loud("arith_div_zero", "echo $((1/0))\n", "ivision by zero"),
    // ── loops ──
    ok("while_counter", "i=0\nwhile [ $i -lt 3 ]; do echo $i; i=$((i+1)); done\n"),
    ok("until_counter", "i=0\nuntil [ $i -ge 3 ]; do echo $i; i=$((i+1)); done\n"),
    ok("for_words", "for x in a b c; do echo \"item $x\"; done\n"),
    ok("for_split_variable", "list=\"1 2 3\"\nfor n in $list; do echo n$n; done\n"),
    ok("c_for", "for ((i=0; i<3; i++)); do echo $i; done\n"),
    ok("c_for_continue", "for ((i=0; i<5; i++)); do\n  if [ $i -eq 2 ]; then continue; fi\n  echo $i\ndone\n"),
    ok(
        "break_continue",
        "i=0\nwhile true; do\n  i=$((i+1))\n  [ $i -eq 2 ] && continue\n  [ $i -gt 4 ] && break\n  echo $i\ndone\necho end $i\n",
    ),
    ok("nested_loops", "for a in 1 2; do\n  for b in x y; do echo $a$b; done\ndone\n"),
    ok("loop_status", "i=0\nwhile [ $i -lt 2 ]; do i=$((i+1)); done\necho $?\n"),
    // ── functions ──
    ok("func_args", "greet() { echo \"hi $1 and $2 ($#)\"; }\ngreet bob alice\ngreet solo\n"),
    ok("func_status", "check() { [ \"$1\" -gt 10 ]; }\nif check 20; then echo big; fi\ncheck 5 || echo small\n"),
    ok("func_return_n", "f() { return 3; }\nf\necho \"st=$?\"\n"),
    ok("func_globals", "count=0\ninc() { count=$((count+1)); }\ninc\ninc\necho $count\n"),
    ok("func_local", "x=outer\nf() { local x=inner; echo $x; }\nf\necho $x\n"),
    ok(
        "func_recursion",
        "fact() {\n  local n=$1\n  if [ $n -le 1 ]; then result=1; return; fi\n  fact $((n-1))\n  result=$((result*n))\n}\nfact 5\necho $result\n",
    ),
    ok("func_dollar_at", "show() { for a in \"$@\"; do echo \"[$a]\"; done; }\nshow \"a b\" c\n"),
    ok("func_dollar_star", "f() { echo \"$*\"; echo $#; }\nf 1 2 3\n"),
    ok("function_keyword", "function hello { echo \"hello $1\"; }\nhello world\n"),
    loud("func_called_before_definition", "f\nf() { echo x; }\n", "before the line"),
    loud("dynamic_scoping", "f() { local v=1; g; }\ng() { echo $v; }\nv=0\nf\n", "dynamic scoping"),
    // ── exit status ──
    ok("exit_code", "echo before\nexit 3\necho after\n"),
    ok("last_status_is_exit", "echo hi\nfalse\n"),
    ok("status_variable", "false\necho $?\ntrue\necho $?\n"),
    ok("negation", "! false && echo negated\n"),
    ok("and_or_chain", "false && echo no || echo yes\ntrue || echo no2\ntrue && echo yes2\n"),
    ok("if_elif_else", "x=2\nif [ $x -eq 1 ]; then echo one; elif [ $x -eq 2 ]; then echo two; else echo many; fi\n"),
    ok("if_no_branch_status", "if false; then echo x; fi\necho $?\n"),
    // ── case ──
    ok("case_basic", "x=b\ncase $x in a) echo A;; b|c) echo BC;; *) echo other;; esac\ncase zz in a) echo A;; *) echo default;; esac\n"),
    loud("case_glob_pattern", "case abc in a*) echo y;; esac\n", "pathname expansion"),
    // ── not lowered: needs processes, files, stdin, arrays ──
    loud("command_substitution", "v=$(echo hi)\necho $v\n", "command substitution"),
    loud("backticks", "v=`echo hi`\necho $v\n", "backticks"),
    loud("arrays", "arr=(a b c)\necho done\n", "array assignment"),
    loud("external_command", "ls /\n", "command `ls`"),
    loud("pipeline", "echo a | cat\n", "pipeline"),
    loud("redirection", "echo a > /dev/null\n", "redirection"),
    loud("read_builtin", "read x\necho $x\n", "command `read`"),
    loud("subshell", "(echo a)\n", "subshell"),
    loud("background", "echo a &\n", "background"),
];

/// Which walker and real shell a corpus run uses.
pub struct Oracle {
    pub dialect: &'static str,
    pub shell: &'static str,
    pub shell_args: &'static [&'static str],
    pub to_cast: fn(&str) -> anyhow::Result<crush_cast::Program>,
}

fn expected(case: &Case, dialect: &str) -> X {
    if dialect == "zsh" { case.zsh } else { case.bash }
}

/// Run the script through crush, on a thread with a wall-clock limit so a
/// lowering that loops forever fails the test instead of hanging it.
fn run_crush(src: &str, to_cast: fn(&str) -> anyhow::Result<crush_cast::Program>) -> Result<(String, i32), String> {
    let (tx, rx) = mpsc::channel();
    let src = src.to_string();
    std::thread::spawn(move || {
        let r = (|| -> Result<(String, i32), String> {
            let cast = to_cast(&src).map_err(|e| format!("{e:#}"))?;
            // The path `crush-walk-run` takes (walker output does not go through the
            // `.crush` semantic pass, which knows only user-defined functions).
            let casm = crush_frontend::compiler::Compiler::new().compile(cast).map_err(|e| format!("CAST→CASM: {e:#}"))?;
            let vm = crush_lang_sdk::compile::casm_to_vm(&casm).map_err(|e| format!("CASM→CVM1: {e:#}"))?;
            let caps = crush_lang_sdk::HostCapsBuilder::new().build();
            let quotas = crush_vm::Quotas { max_steps: 5_000_000, ..Default::default() };
            let mut out = String::new();
            let r = crush_vm::vm::run_with_caps_streaming(&vm, &quotas, Some(&caps), &mut |s: &str| out.push_str(s));
            match r {
                Ok(_) => Ok((out, 0)),
                Err(crush_vm::VmError::Exit(code)) => Ok((out, code)),
                Err(e) => Err(format!("runtime: {e}")),
            }
        })();
        let _ = tx.send(r);
    });
    rx.recv_timeout(Duration::from_secs(30)).unwrap_or_else(|_| Err("TIMEOUT: crush run did not finish in 30 s".into()))
}

fn run_shell(o: &Oracle, name: &str, src: &str) -> (String, i32) {
    let dir = std::env::temp_dir().join(format!("crush-shell-oracle-{}-{}", o.dialect, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.sh"));
    let cwd = dir.join("cwd");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::File::create(&path).unwrap().write_all(src.as_bytes()).unwrap();
    let mut child = Command::new(o.shell)
        .args(o.shell_args)
        .arg(&path)
        .env("LC_ALL", "C.UTF-8")
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn {}: {e}", o.shell));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("{} hung on case {name}", o.shell);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let out = child.wait_with_output().unwrap();
    (String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code().unwrap_or(-1))
}

fn show(s: &str) -> String {
    let t = s.replace('\n', "⏎").replace('|', "\\|");
    if t.chars().count() > 60 { format!("{}…", t.chars().take(60).collect::<String>()) } else { t }
}

pub fn run_corpus(o: &Oracle) {
    if Command::new(o.shell).arg("-c").arg("exit 0").stdin(Stdio::null()).status().map(|s| !s.success()).unwrap_or(true) {
        eprintln!("NOTICE: {} not available — skipping the {} oracle corpus", o.shell, o.dialect);
        return;
    }
    let mut failures = Vec::new();
    let (mut matched, mut loud_n) = (0, 0);
    eprintln!("| case | {} out (exit) | crush out (exit) | verdict |", o.dialect);
    eprintln!("|---|---|---|---|");
    for case in CASES {
        let (sh_out, sh_code) = run_shell(o, case.name, case.src);
        let crush = run_crush(case.src, o.to_cast);
        let crush_cell = match &crush {
            Ok((out, code)) => format!("`{}` ({code})", show(out)),
            Err(e) => format!("error: {}", show(e)),
        };
        let verdict = match (expected(case, o.dialect), &crush) {
            (X::Match, Ok((out, code))) if *out == sh_out && *code == sh_code => {
                matched += 1;
                "MATCH".to_string()
            }
            (X::Match, Ok(_)) => {
                failures.push(format!("{}: SILENTLY WRONG\n  {}: {sh_out:?} ({sh_code})\n  crush: {crush:?}", case.name, o.dialect));
                "**SILENT-WRONG**".to_string()
            }
            (X::Match, Err(e)) => {
                failures.push(format!("{}: expected a match, crush failed: {e}", case.name));
                "**FAILED**".to_string()
            }
            (X::Loud(needle), Err(e)) if e.contains(needle) => {
                loud_n += 1;
                "UNSUPPORTED (loud)".to_string()
            }
            (X::Loud(needle), Err(e)) => {
                failures.push(format!("{}: loud, but the error lacks {needle:?}: {e}", case.name));
                "**WRONG-ERROR**".to_string()
            }
            (X::Loud(_), Ok((out, code))) => {
                let same = *out == sh_out && *code == sh_code;
                failures.push(format!(
                    "{}: expected a loud refusal, crush ran ({}): {out:?} ({code}) vs {}: {sh_out:?} ({sh_code})",
                    case.name,
                    if same { "and now matches — update the corpus" } else { "SILENTLY WRONG" },
                    o.dialect
                ));
                "**RAN**".to_string()
            }
        };
        eprintln!("| {} | `{}` ({sh_code}) | {crush_cell} | {verdict} |", case.name, show(&sh_out));
    }
    eprintln!("{}: {} cases — {matched} match, {loud_n} loud-unsupported, {} failures", o.dialect, CASES.len(), failures.len());
    assert!(failures.is_empty(), "{} {} corpus failure(s):\n{}", failures.len(), o.dialect, failures.join("\n"));
}
