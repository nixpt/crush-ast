//! Raw-socket capabilities (`tcp.*`, crush-net) through the real `crush-run`
//! binary, over real loopback sockets (CRUSH-318). Nothing here leaves
//! 127.0.0.1, and every port is OS-assigned.
//!
//! What this pins:
//!   * `--tcp` lets a Crush program be an echo *client*; it does not let it listen;
//!   * `--tcp-listen` lets a Crush program accept a connection; it does not let it connect out;
//!   * with neither, and with `--net` (HTTP) alone, both programs are refused
//!     *before running*, with the flag named.
#![cfg(feature = "tcp")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn crush_run(args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_crush-run"));
    c.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}

fn write_prog(dir: &tempfile::TempDir, name: &str, src: &str) -> String {
    let p = dir.path().join(name);
    std::fs::write(&p, src).unwrap();
    p.to_string_lossy().into_owned()
}

fn client_prog(port: u16) -> String {
    format!(
        r#"fn main() {{
    let c = tcp.connect("tcp://127.0.0.1:{port}")
    tcp.send(c, "hello crush")
    let reply = tcp.recv(c)
    io.print(reply)
    tcp.close(c)
}}
"#
    )
}

fn server_prog(port: u16) -> String {
    format!(
        r#"fn main() {{
    let l = tcp.listen("tcp://127.0.0.1:{port}")
    let c = tcp.accept(l)
    let msg = tcp.recv(c)
    tcp.send(c, "got:" + msg)
    tcp.close(c)
    io.print("served")
}}
"#
    )
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// Spawn a one-shot echo server on an ephemeral port; returns (port, join handle).
fn echo_server() -> (u16, std::thread::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let h = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut buf = [0u8; 256];
        let n = s.read(&mut buf).unwrap();
        s.write_all(&buf[..n]).unwrap();
        buf[..n].to_vec()
    });
    (port, h)
}

#[test]
fn tcp_grant_runs_an_echo_client_against_a_real_socket() {
    let (port, server) = echo_server();
    let dir = tempfile::tempdir().unwrap();
    let prog = write_prog(&dir, "client.crush", &client_prog(port));
    let out = crush_run(&["run", "--tcp", &prog]).output().unwrap();
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout(&out),
        stderr(&out)
    );
    assert!(
        stdout(&out).contains("hello crush"),
        "stdout={}",
        stdout(&out)
    );
    assert_eq!(server.join().unwrap(), b"hello crush");
}

#[test]
fn tcp_listen_grant_accepts_one_connection_from_a_real_client() {
    let port = free_port();
    let dir = tempfile::tempdir().unwrap();
    let prog = write_prog(&dir, "server.crush", &server_prog(port));
    let child = crush_run(&["run", "--tcp-listen", &prog]).spawn().unwrap();

    // The program binds a moment after it starts; retry the connect.
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut conn = loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(c) => break c,
            Err(e) if Instant::now() < deadline => {
                let _ = e;
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => panic!("never connected to the Crush server: {e}"),
        }
    };
    conn.set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    conn.write_all(b"ping from rust").unwrap();
    let mut reply = String::new();
    conn.read_to_string(&mut reply).unwrap();
    assert_eq!(reply, "got:ping from rust");

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "stderr={}", stderr(&out));
    assert!(stdout(&out).contains("served"), "stdout={}", stdout(&out));
}

/// Refused up front (nothing runs), naming the flag that would grant it.
fn assert_refused(args: &[&str], prog: &str, flag: &str, cap: &str) {
    let mut a = vec!["run"];
    a.extend_from_slice(args);
    a.push(prog);
    let out = crush_run(&a).output().unwrap();
    assert!(
        !out.status.success(),
        "expected refusal with {args:?}; stdout={}",
        stdout(&out)
    );
    let err = stderr(&out);
    assert!(
        err.contains(flag) && err.contains(cap),
        "args={args:?} stderr={err}"
    );
    assert!(
        stdout(&out).is_empty(),
        "program must not have run: {}",
        stdout(&out)
    );
}

#[test]
fn programs_are_refused_without_their_grant_naming_the_flag() {
    let dir = tempfile::tempdir().unwrap();
    // Unused port: if the check were skipped these would fail differently (and print).
    let client = write_prog(&dir, "client.crush", &client_prog(free_port()));
    let server = write_prog(&dir, "server.crush", &server_prog(free_port()));

    // No grant at all.
    assert_refused(&[], &client, "--tcp", "tcp.connect");
    assert_refused(&[], &server, "--tcp-listen", "tcp.listen");
    // The client grant is not the server grant ...
    assert_refused(&["--tcp"], &server, "--tcp-listen", "tcp.listen");
    // ... and the server grant does not allow dialing out.
    assert_refused(&["--tcp-listen"], &client, "--tcp", "tcp.connect");
    // The HTTP grant, and the other I/O grants, grant neither.
    assert_refused(
        &["--net", "--fs", "--process", "--env"],
        &client,
        "--tcp",
        "tcp.connect",
    );
    assert_refused(
        &["--net", "--fs", "--process", "--env"],
        &server,
        "--tcp-listen",
        "tcp.listen",
    );
}

#[test]
fn catalog_puts_sockets_under_their_own_grants_not_net() {
    use crush_lang_sdk::effects::{TCP_IO_GRANT, catalog};
    let cat = catalog();
    let grant = |n: &str| {
        cat.iter()
            .find(|c| c.name == n)
            .unwrap_or_else(|| panic!("{n} missing"))
            .grant
    };
    assert_eq!(grant("tcp.connect"), "--tcp");
    assert_eq!(grant("tcp.tls_wrap"), "--tcp");
    assert_eq!(grant("tcp.listen"), "--tcp-listen");
    assert_eq!(grant("tcp.accept"), "--tcp-listen");
    for n in ["tcp.send", "tcp.recv", "tcp.close", "tcp.ping"] {
        assert_eq!(grant(n), TCP_IO_GRANT);
    }
    // Nothing the `--net` (HTTP) grant registers is a socket cap, and vice versa.
    assert!(
        cat.iter()
            .filter(|c| c.grant == "--net")
            .all(|c| c.name.starts_with("net.http"))
    );
    assert!(
        cat.iter()
            .filter(|c| c.name.starts_with("tcp."))
            .all(|c| !c.is_ambient())
    );
}

#[test]
fn builder_grants_are_independent() {
    use crush_lang_sdk::HostCapsBuilder;
    let has = |b: HostCapsBuilder, n: &str| b.build().get(n).is_some();
    // Client only.
    assert!(has(HostCapsBuilder::new().tcp(true), "tcp.connect"));
    assert!(!has(HostCapsBuilder::new().tcp(true), "tcp.listen"));
    assert!(!has(HostCapsBuilder::new().tcp(true), "tcp.accept"));
    // Server only.
    assert!(has(HostCapsBuilder::new().tcp_listen(true), "tcp.accept"));
    assert!(has(HostCapsBuilder::new().tcp_listen(true), "tcp.recv"));
    assert!(!has(HostCapsBuilder::new().tcp_listen(true), "tcp.connect"));
    // Nothing granted, nothing registered.
    for n in ["tcp.connect", "tcp.listen", "tcp.send", "tcp.ping"] {
        assert!(
            !has(HostCapsBuilder::new(), n),
            "{n} registered without a grant"
        );
    }
    // The HTTP grant does not register any socket cap.
    #[cfg(feature = "net")]
    for n in ["tcp.connect", "tcp.listen", "tcp.send"] {
        assert!(
            !has(HostCapsBuilder::new().net(true), n),
            "--net must not grant {n}"
        );
    }
}

/// A one-shot server that writes `chunks` with `gap` between them, then
/// closes. Returns its port and a handle yielding what the client sent.
fn chunked_server(chunks: Vec<Vec<u8>>, gap: Duration) -> (u16, std::thread::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let h = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_nodelay(true).unwrap();
        for c in chunks {
            s.write_all(&c).unwrap();
            s.flush().unwrap();
            std::thread::sleep(gap);
        }
        let _ = s.shutdown(std::net::Shutdown::Write);
        let mut got = Vec::new();
        s.set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        let _ = s.read_to_end(&mut got);
        got
    });
    (port, h)
}

/// #507: a multibyte character split across two TCP reads is reassembled,
/// not an error, and the bytes are not lost.
#[test]
fn recv_reassembles_a_utf8_character_split_across_reads() {
    let e_acute = "é".as_bytes().to_vec(); // [0xC3, 0xA9]
    let (port, h) = chunked_server(
        vec![vec![b'a', e_acute[0]], vec![e_acute[1], b'b']],
        Duration::from_millis(150),
    );
    let dir = tempfile::tempdir().unwrap();
    let prog = write_prog(
        &dir,
        "split.crush",
        &format!(
            r#"fn main() {{
    let c = tcp.connect("tcp://127.0.0.1:{port}")
    let all = ""
    let n = 0
    let part = tcp.recv(c)
    while part != "" {{
        all = all + part
        n = n + 1
        part = tcp.recv(c)
    }}
    io.print(all)
    tcp.close(c)
}}
"#
        ),
    );
    let out = crush_run(&["run", "--tcp", &prog]).output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "aéb\n");
    h.join().unwrap();
}

/// #507: `tcp.recv(c, {timeout_ms})` fails catchably and loses nothing: the
/// data that arrives later is read by the next call. Also: tcp.send takes
/// a bytes value.
#[test]
fn recv_timeout_is_catchable_and_send_takes_bytes() {
    let (port, h) = chunked_server(vec![vec![], b"late".to_vec()], Duration::from_millis(600));
    let dir = tempfile::tempdir().unwrap();
    let prog = write_prog(
        &dir,
        "timeout.crush",
        &format!(
            r#"fn main() {{
    let c = tcp.connect("tcp://127.0.0.1:{port}")
    tcp.send(c, bytes.from_string("hi"))
    try {{ tcp.recv(c, {{ timeout_ms: 150 }}) }} catch e {{ io.print("caught: " + conv.to_str(e)) }}
    io.print(tcp.recv(c, {{ timeout_ms: 5000 }}))
    tcp.close(c)
}}
"#
        ),
    );
    let out = crush_run(&["run", "--tcp", &prog]).output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("tcp.recv: timed out after 150 ms"), "{text}");
    assert!(text.ends_with("late\n"), "{text}");
    assert_eq!(h.join().unwrap(), b"hi");
}

/// #507: `tcp.accept(l, {timeout_ms})` gives up catchably when no client
/// comes (`CapTimedOut`, #464). Bad opts are refused with the key named, and
/// a refusal is fatal even inside try/catch (ruling 5).
#[test]
fn accept_timeout_is_catchable_and_opts_are_checked() {
    let port = free_port();
    let dir = tempfile::tempdir().unwrap();
    let prog = write_prog(
        &dir,
        "accept.crush",
        &format!(
            r#"fn main() {{
    let l = tcp.listen("tcp://127.0.0.1:{port}")
    try {{ tcp.accept(l, {{ timeout_ms: 150 }}) }} catch e {{ io.print("caught: " + conv.to_str(e)) }}
    try {{ tcp.accept(l, {{ timeout: 150 }}) }} catch e {{ io.print("bad: " + conv.to_str(e)) }}
    io.print("done")
}}
"#
        ),
    );
    let start = Instant::now();
    let out = crush_run(&["run", "--tcp-listen", &prog]).output().unwrap();
    assert!(!out.status.success(), "the bad opts key must end the run");
    let text = stdout(&out);
    assert!(text.starts_with("caught: "), "{text}");
    assert!(
        text.contains("tcp.accept: timed out after 150 ms"),
        "{text}"
    );
    assert!(!text.contains("bad: "), "{text}");
    assert!(!text.contains("done"), "{text}");
    assert!(
        stderr(&out).contains("unknown opts key 'timeout'"),
        "{}",
        stderr(&out)
    );
    assert!(start.elapsed() < Duration::from_secs(5));
}
