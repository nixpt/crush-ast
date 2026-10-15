// JS/TS parity spike baseline probe (2026-10-15, main 5f223a9): 54 programs vs node.
// Seed for the CRUSH-zorro-4 corpus. To re-run: copy to crates/crush-lang-js/tests/ and `cargo test -p crush-lang-js --test <name> -- --nocapture`.
fn run_crush(source: &str) -> String {
    let r = (|| -> anyhow::Result<String> {
        let cast = crush_lang_js::js_to_cast(source, "js")?;
        let casm = crush_frontend::compiler::Compiler::new().compile(cast).map_err(|e| anyhow::anyhow!("CAST→CASM: {e}"))?;
        let vm = crush_lang_sdk::compile::casm_to_vm(&casm)?;
        let quotas = crush_vm::Quotas { max_steps: 10_000_000, ..Default::default() };
        Ok(crush_vm::run_with_caps(&vm, &quotas, None)?.output)
    })();
    r.unwrap_or_else(|e| format!("ERROR: {}", format!("{e:#}").lines().next().unwrap_or("")))
}
fn node(source: &str) -> String {
    let out = std::process::Command::new("node").arg("-e").arg(source).output().unwrap();
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or(""))
}
#[test]
fn probe() {
    let cases: &[(&str, &str)] = &[
        ("vars", "let a = 1; const b = 2; var c = 3; console.log(a + b + c);"),
        ("block-scope", "let x = 1; { let x = 2; console.log(x); } console.log(x);"),
        ("var-hoist", "function f() { if (true) { var v = 5; } return v; } console.log(f());"),
        ("func-decl", "function add(a, b) { return a + b; } console.log(add(2, 3));"),
        ("hoisted-call", "console.log(sq(4)); function sq(n) { return n * n; }"),
        ("arrow", "const dbl = (n) => n * 2; console.log(dbl(21));"),
        ("closure-counter", "function mk() { let c = 0; return () => ++c; } const k = mk(); k(); console.log(k());"),
        ("higher-order", "function ap(f, x) { return f(x); } console.log(ap((n) => n + 1, 1));"),
        ("default-param", "function g(a, b = 10) { return a + b; } console.log(g(1));"),
        ("rest-param", "function s(...xs) { return xs.length; } console.log(s(1, 2, 3));"),
        ("recursion", "function fib(n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); } console.log(fib(15));"),
        ("if-else", "let n = 7; if (n % 2 === 0) { console.log('even'); } else { console.log('odd'); }"),
        ("for", "let t = 0; for (let i = 0; i < 5; i++) { t += i; } console.log(t);"),
        ("for-of", "let t = 0; for (const v of [1, 2, 3]) { t += v; } console.log(t);"),
        ("for-in", "const o = {a: 1, b: 2}; const ks = []; for (const k in o) { ks.push(k); } console.log(ks.join(','));"),
        ("while-break", "let i = 0; while (true) { i++; if (i > 3) break; } console.log(i);"),
        ("do-while", "let i = 0; do { i++; } while (i < 3); console.log(i);"),
        ("switch", "function f(x) { switch (x) { case 1: return 'one'; case 2: return 'two'; default: return 'many'; } } console.log(f(2), f(9));"),
        ("labels", "outer: for (let i = 0; i < 3; i++) { for (let j = 0; j < 3; j++) { if (j == 1) continue outer; if (i == 2) break outer; console.log(i, j); } }"),
        ("try-catch", "try { throw new Error('boom'); } catch (e) { console.log('caught', e.message); }"),
        ("finally", "function f() { try { return 1; } finally { console.log('fin'); } } console.log(f());"),
        ("ternary-nullish", "const u = null; console.log(u ?? 'dflt', 0 || 'or', 1 ? 'y' : 'n');"),
        ("optional-chain", "const o = {a: {b: 5}}; console.log(o?.a?.b, o?.x?.y);"),
        ("strict-eq", "console.log(1 === 1, '1' == 1, null == undefined, NaN === NaN);"),
        ("typeof", "console.log(typeof 1, typeof 'a', typeof {}, typeof undefined, typeof (() => 1));"),
        ("bitwise", "console.log(5 & 3, 5 | 3, 5 ^ 3, ~5, 1 << 4, -16 >> 2, -16 >>> 28);"),
        ("exp-op", "console.log(2 ** 10, (-8) ** (1/3));"),
        ("int-overflow", "console.log(2 ** 53 + 1, 9007199254740993, 2 ** 64);"),
        ("division", "console.log(7 / 2, 1 / 0, -1 / 0, 0 / 0);"),
        ("modulo", "console.log(7 % 3, -7 % 3, 5.5 % 2);"),
        ("string-concat", "console.log('a' + 1 + 2, 1 + 2 + 'a', 'x' + 1.5 * 4);"),
        ("template", "const n = 3; console.log(`n=${n} sq=${n * n} half=${n / 2}`);"),
        ("str-methods", "const s = 'Hello, World'; console.log(s.toUpperCase(), s.length, s.indexOf('World'), s.slice(0, 5), s.includes('lo'));"),
        ("str-methods2", "console.log('a-b-c'.split('-').join('+'), '  x '.trim(), 'ab'.repeat(3), '5'.padStart(3, '0'), 'abc'.charAt(1));"),
        ("str-replace", "console.log('aaa'.replace('a', 'b'), 'aaa'.replaceAll('a', 'b'), 'abc'.substring(1, 3), 'abc'.charCodeAt(0));"),
        ("arr-basic", "const a = [3, 1, 2]; a.push(4); console.log(a.length, a[0], a.indexOf(2), a.includes(9));"),
        ("arr-hof", "const a = [1, 2, 3, 4]; console.log(a.map(x => x * 2).join(','), a.filter(x => x % 2).join(','), a.reduce((s, x) => s + x, 0));"),
        ("arr-sort", "const a = [10, 9, 1, 2]; a.sort(); console.log(a.join(',')); a.sort((x, y) => x - y); console.log(a.join(','));"),
        ("arr-slice-splice", "const a = [1, 2, 3, 4, 5]; console.log(a.slice(1, 3).join(','), a.splice(1, 2).join(','), a.join(','));"),
        ("arr-print", "console.log([1, 'a', true, null], [1.5 * 4]);"),
        ("obj-literal", "const o = {name: 'x', n: 2}; o.n += 1; console.log(o.name, o.n, Object.keys(o).join(','));"),
        ("obj-print", "console.log({a: 1, b: 'two'});"),
        ("spread", "const a = [1, 2]; const b = [...a, 3]; const o = {...{x: 1}, y: 2}; console.log(b.length, o.x + o.y);"),
        ("destructure", "const [p, q] = [1, 2]; const {r, s: t} = {r: 3, s: 4}; console.log(p + q + r + t);"),
        ("class", "class P { constructor(x) { this.x = x; } get2() { return this.x * 2; } } console.log(new P(4).get2());"),
        ("class-extends", "class A { hi() { return 'A'; } } class B extends A { hi() { return 'B' + super.hi(); } } console.log(new B().hi());"),
        ("math", "console.log(Math.floor(2.7), Math.ceil(2.1), Math.round(2.5), Math.max(1, 5, 3), Math.abs(-2), Math.sqrt(16), Math.PI);"),
        ("number-conv", "console.log(parseInt('42px'), parseFloat('3.5'), Number('7'), String(12), (255).toString(16), (3.14159).toFixed(2));"),
        ("json", "console.log(JSON.stringify({a: [1, 2], b: 'x'}), JSON.parse('{\"k\": 5}').k);"),
        ("map-set", "const m = new Map(); m.set('a', 1); const s = new Set([1, 1, 2]); console.log(m.get('a'), m.size, s.size, s.has(2));"),
        ("undefined", "let u; console.log(u, [1][5], ({}).x);"),
        ("async", "async function f() { return 5; } f().then(v => console.log(v));"),
        ("generator", "function* g() { yield 1; yield 2; } console.log([...g()].join(','));"),
        ("console-multi", "console.log('a', 1, true, null, undefined);"),
    ];
    let (mut ok, mut loud, mut wrong) = (vec![], vec![], vec![]);
    for (name, c) in cases {
        let (got, want) = (run_crush(c), node(c));
        if got == want { ok.push(*name) }
        else if got.starts_with("ERROR:") { loud.push(format!("{name}: {}", got.chars().take(110).collect::<String>())) }
        else { wrong.push(format!("{name}: crush={:?} node={:?}", got.trim_end(), want.trim_end())) }
    }
    println!("MATCH ({}): {}", ok.len(), ok.join(" "));
    println!("LOUD ({}):", loud.len()); for l in &loud { println!("  {l}"); }
    println!("SILENTLY WRONG ({}):", wrong.len()); for w in &wrong { println!("  {w}"); }
}
