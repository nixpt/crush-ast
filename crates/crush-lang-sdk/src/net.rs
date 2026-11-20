//! Network host capabilities for the CRUSH runtime.
//!
//! Enabled by the `net` cargo feature and the `--net` grant. Every verb —
//! `net.http_get`, `net.http_post`, `net.http_put`, `net.http_delete` and the
//! general `net.http_request` — goes through one [`request`] function over
//! `ureq`, so they share the bounded response size and the wall-clock
//! deadline: a request still running when the VM's `max_wall_time_ms` runs
//! out is abandoned and reported as `CapTimeout`.
//!
//! Every verb takes an optional trailing `opts` map (#508):
//! `{ timeout_ms: N }` bounds that one request. A request that outlives its
//! own `timeout_ms` fails with a `CapFailed` error (`… timed out after N ms`),
//! fatal like every capability failure (ruling 5, #464); the VM's wall budget
//! still applies on top.
//! `net.http_request` returns `{status, reason, headers, body}`; `headers` is
//! a map of lower-cased response header names (repeated headers joined with
//! `, `). `status` and `body` are unchanged from CRUSH-153.
//!
//! One name per capability: exosphere's `http.*` spellings are deliberately
//! not aliased (CRUSH-153).

use crush_vm::host::HostCapError;
use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};
use std::collections::HashMap;
use std::time::Duration;

/// Add network capabilities to an existing [`HostCaps`] registry.
pub fn register(caps: &mut HostCaps, max_response_bytes: usize) {
    caps.register(Box::new(NetHttpGetCap::new(max_response_bytes)));
    caps.register(Box::new(NetHttpPostCap::new(max_response_bytes)));
    for verb in [Verb::Put, Verb::Delete, Verb::Request] {
        caps.register(Box::new(NetHttpCap {
            verb,
            max_response_bytes,
        }));
    }
}

/// Which capability a [`NetHttpCap`] is, and how its arguments map onto a
/// request.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verb {
    /// `net.http_get(url, opts?)` → body
    Get,
    /// `net.http_post(url, body, opts?)` → body
    Post,
    /// `net.http_put(url, body, opts?)` → body
    Put,
    /// `net.http_delete(url, opts?)` → body
    Delete,
    /// `net.http_request(method, url, body, headers, opts?)` →
    /// `{status, reason, headers, body}`
    Request,
}

impl Verb {
    fn name(self) -> &'static str {
        match self {
            Verb::Get => "net.http_get",
            Verb::Post => "net.http_post",
            Verb::Put => "net.http_put",
            Verb::Delete => "net.http_delete",
            Verb::Request => "net.http_request",
        }
    }

    /// Required argument count; one more (the `opts` map) is optional.
    fn argc(self) -> usize {
        match self {
            Verb::Get | Verb::Delete => 1,
            Verb::Post | Verb::Put => 2,
            Verb::Request => 4,
        }
    }

    fn signature(self) -> &'static str {
        match self {
            Verb::Get | Verb::Delete => "url, opts?",
            Verb::Post | Verb::Put => "url, body, opts?",
            Verb::Request => "method, url, body, headers, opts?",
        }
    }
}

/// One HTTP capability. `net.http_get` / `net.http_post` keep their own
/// public types below for compatibility; they wrap this.
struct NetHttpCap {
    verb: Verb,
    max_response_bytes: usize,
}

impl HostCap for NetHttpCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: self.verb.name().to_string(),
            // argc or argc + 1 (trailing `opts`); `request` checks the count.
            argc: None,
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        request(self.verb, &args, self.max_response_bytes, None).map_err(|e| match e {
            HostCapError::Message(m) => m,
            HostCapError::Timeout => format!("{}: timed out", self.verb.name()),
            // `sys.exit` is the only source of `Exit`, and an HTTP request never raises it;
            // the string-returning `call()` has no way to carry a status, so report it.
            HostCapError::Exit(code) => format!("{}: unexpected exit({code})", self.verb.name()),
        })
    }

    fn call_with_deadline(
        &self,
        args: Vec<Value>,
        deadline_ms: u64,
    ) -> Result<Option<Value>, HostCapError> {
        request(self.verb, &args, self.max_response_bytes, Some(deadline_ms))
    }
}

/// The one HTTP implementation behind every `net.*` verb.
fn request(
    verb: Verb,
    args: &[Value],
    max_bytes: usize,
    deadline_ms: Option<u64>,
) -> Result<Option<Value>, HostCapError> {
    let cap = verb.name();
    let text = crate::caps::value_as_text;
    let need = verb.argc();
    if args.len() != need && args.len() != need + 1 {
        return Err(HostCapError::Message(format!(
            "{cap}: expected {need} or {} arguments ({}), got {}",
            need + 1,
            verb.signature(),
            args.len()
        )));
    }
    let timeout_ms = request_timeout(cap, args.get(need))?;
    let (method, url, body, headers) = match verb {
        Verb::Get => ("GET".to_string(), text(&args[0]), None, Vec::new()),
        Verb::Delete => ("DELETE".to_string(), text(&args[0]), None, Vec::new()),
        Verb::Post => (
            "POST".to_string(),
            text(&args[0]),
            Some(text(&args[1])),
            Vec::new(),
        ),
        Verb::Put => (
            "PUT".to_string(),
            text(&args[0]),
            Some(text(&args[1])),
            Vec::new(),
        ),
        Verb::Request => {
            let method = text(&args[0]).to_ascii_uppercase();
            if method.is_empty() || !method.bytes().all(|b| b.is_ascii_alphabetic()) {
                return Err(format!("{cap}: invalid method '{method}'").into());
            }
            let body = match &args[2] {
                Value::Null => None,
                v => Some(text(v)),
            };
            (method, text(&args[1]), body, header_list(cap, &args[3])?)
        }
    };

    // The per-request bound wins only when it is tighter than the VM's wall
    // budget; then a timeout is this request's own failure (`CapFailed`).
    let own_bound = match (timeout_ms, deadline_ms) {
        (Some(t), Some(d)) => t < d,
        (Some(_), None) => true,
        (None, _) => false,
    };
    let limit = match (timeout_ms, deadline_ms) {
        (Some(t), Some(d)) => Some(t.min(d)),
        (t, d) => t.or(d),
    };
    let timed_out = || -> HostCapError {
        match (own_bound, timeout_ms) {
            (true, Some(t)) => {
                HostCapError::Message(format!("{cap} {url}: timed out after {t} ms"))
            }
            _ => HostCapError::Timeout,
        }
    };
    let mut agent = ureq::AgentBuilder::new();
    // An unlimited wall budget (#505) arrives as u64::MAX: no agent timeout
    // then, rather than a Duration ureq would overflow adding to `now`.
    if let Some(ms) = limit.filter(|&ms| crush_vm::wall::instant_after(ms).is_some()) {
        agent = agent.timeout(Duration::from_millis(ms.max(1)));
    }
    let mut req = agent.build().request(&method, &url);
    for (k, v) in &headers {
        req = req.set(k, v);
    }
    let sent = match &body {
        Some(b) => req.send_string(b),
        None => req.call(),
    };
    let response = match sent {
        Ok(r) => r,
        // The general verb reports the status instead of failing on it.
        Err(ureq::Error::Status(_, r)) if verb == Verb::Request => r,
        Err(e) if is_timeout(&e) => return Err(timed_out()),
        Err(e) => return Err(transport_error(cap, &url, e)),
    };
    let status = response.status();
    let reason = response.status_text().to_string();
    let response_headers = response_header_map(&response);
    let body = read_limited(response, max_bytes).map_err(|e| match e {
        ReadError::TimedOut => timed_out(),
        ReadError::Other(m) => HostCapError::Message(format!("{cap} {url}: {m}")),
    })?;
    Ok(Some(match verb {
        Verb::Request => Value::new_map(HashMap::from([
            ("status".to_string(), Value::Int(i64::from(status))),
            ("reason".to_string(), Value::Str(reason)),
            ("headers".to_string(), response_headers),
            ("body".to_string(), Value::Str(body)),
        ])),
        _ => Value::Str(body),
    }))
}

/// The optional trailing `opts` map: null/absent, or `{ timeout_ms: N }`
/// with N a positive integer. Unknown keys are refused, so a typo like
/// `timeout` is not silently ignored.
fn request_timeout(cap: &str, opts: Option<&Value>) -> Result<Option<u64>, HostCapError> {
    let map = match opts {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Map(m)) => m.borrow(),
        Some(other) => {
            return Err(format!("{cap}: opts must be a map or null, got {other}").into());
        }
    };
    let mut timeout = None;
    for (k, v) in map.iter() {
        match (k.as_str(), v) {
            ("timeout_ms", Value::Int(n)) if *n > 0 => timeout = Some(*n as u64),
            ("timeout_ms", Value::Null) => {}
            ("timeout_ms", other) => {
                return Err(format!(
                    "{cap}: opts.timeout_ms must be a positive integer, got {other}"
                )
                .into());
            }
            (key, _) => {
                return Err(format!("{cap}: unknown opts key '{key}' (known: timeout_ms)").into());
            }
        }
    }
    Ok(timeout)
}

/// Response headers as a map: lower-cased names, repeated values joined
/// with `, ` (RFC 9110 §5.3).
fn response_header_map(response: &ureq::Response) -> Value {
    let mut out: HashMap<String, Value> = HashMap::new();
    for name in response.headers_names() {
        let key = name.to_ascii_lowercase();
        if out.contains_key(&key) {
            continue;
        }
        let joined = response.all(&name).join(", ");
        out.insert(key, Value::Str(joined));
    }
    Value::new_map(out)
}

/// `headers` for `net.http_request`: a map of name → value, or null for none.
fn header_list(cap: &str, v: &Value) -> Result<Vec<(String, String)>, HostCapError> {
    match v {
        Value::Null => Ok(Vec::new()),
        Value::Map(m) => {
            let mut list: Vec<_> = m
                .borrow()
                .iter()
                .map(|(k, v)| (k.clone(), crate::caps::value_as_text(v)))
                .collect();
            list.sort();
            Ok(list)
        }
        other => Err(format!("{cap}: headers must be a map or null, got {other}").into()),
    }
}

/// A transport failure, or [`HostCapError::Timeout`] when it was the deadline.
fn transport_error(cap: &str, url: &str, e: ureq::Error) -> HostCapError {
    if is_timeout(&e) {
        return HostCapError::Timeout;
    }
    HostCapError::Message(format!("{cap} {url}: {e}"))
}

fn is_timeout(e: &(dyn std::error::Error + 'static)) -> bool {
    let mut source = Some(e);
    while let Some(err) = source {
        if let Some(io) = err.downcast_ref::<std::io::Error>()
            && matches!(
                io.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            )
        {
            return true;
        }
        source = err.source();
    }
    false
}

pub struct NetHttpGetCap(NetHttpCap);

impl NetHttpGetCap {
    pub fn new(max_response_bytes: usize) -> Self {
        Self(NetHttpCap {
            verb: Verb::Get,
            max_response_bytes,
        })
    }
}

pub struct NetHttpPostCap(NetHttpCap);

impl NetHttpPostCap {
    pub fn new(max_response_bytes: usize) -> Self {
        Self(NetHttpCap {
            verb: Verb::Post,
            max_response_bytes,
        })
    }
}

macro_rules! delegate_http_cap {
    ($ty:ty) => {
        impl HostCap for $ty {
            fn spec(&self) -> HostCapSpec {
                self.0.spec()
            }
            fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
                self.0.call(args)
            }
            fn call_with_deadline(
                &self,
                args: Vec<Value>,
                deadline_ms: u64,
            ) -> Result<Option<Value>, HostCapError> {
                self.0.call_with_deadline(args, deadline_ms)
            }
        }
    };
}

delegate_http_cap!(NetHttpGetCap);
delegate_http_cap!(NetHttpPostCap);

enum ReadError {
    TimedOut,
    Other(String),
}

fn read_limited(response: ureq::Response, max_bytes: usize) -> Result<String, ReadError> {
    use std::io::Read;
    let mut reader = response.into_reader().take(max_bytes as u64 + 1);
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).map_err(|e| {
        if is_timeout(&e) {
            ReadError::TimedOut
        } else {
            ReadError::Other(format!("failed to read response: {e}"))
        }
    })?;
    if buf.len() > max_bytes {
        return Err(ReadError::Other(format!(
            "response exceeded {max_bytes} bytes"
        )));
    }
    String::from_utf8(buf)
        .map_err(|e| ReadError::Other(format!("response is not valid UTF-8: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    #[test]
    fn http_get_spec() {
        let cap = NetHttpGetCap::new(4096);
        assert_eq!(cap.spec().name, "net.http_get");
        // url + optional opts: the cap checks the count itself (#508).
        assert_eq!(cap.spec().argc, None);
        assert!(cap.spec().returns);
    }

    /// What the local test server saw.
    #[derive(Debug)]
    struct Seen {
        request_line: String,
        headers: Vec<String>,
        body: String,
    }

    /// A one-shot HTTP server on 127.0.0.1: answers the first request with
    /// `status` + `reply` (after `delay`) and reports what it received.
    fn serve_once(status: u16, reply: &str, delay: Duration) -> (String, mpsc::Receiver<Seen>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/path", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        let reply = reply.to_string();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let mut headers = Vec::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let line = line.trim_end().to_string();
                if line.is_empty() {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
                headers.push(line);
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let _ = tx.send(Seen {
                request_line: request_line.trim_end().to_string(),
                headers,
                body: String::from_utf8(body).unwrap(),
            });
            std::thread::sleep(delay);
            let mut stream = stream;
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
        });
        (url, rx)
    }

    fn caps() -> HostCaps {
        let mut caps = HostCaps::new();
        register(&mut caps, 4096);
        caps
    }

    fn s(v: &str) -> Value {
        Value::Str(v.to_string())
    }

    #[test]
    fn put_and_delete_send_their_method_and_body() {
        let caps = caps();
        let (url, seen) = serve_once(200, "put-ok", Duration::ZERO);
        let out = caps
            .get("net.http_put")
            .unwrap()
            .call(vec![s(&url), s("payload")]);
        assert_eq!(out, Ok(Some(s("put-ok"))));
        let seen = seen.recv().unwrap();
        assert_eq!(seen.request_line, "PUT /path HTTP/1.1");
        assert_eq!(seen.body, "payload");

        let (url, seen) = serve_once(200, "gone", Duration::ZERO);
        let out = caps.get("net.http_delete").unwrap().call(vec![s(&url)]);
        assert_eq!(out, Ok(Some(s("gone"))));
        assert_eq!(seen.recv().unwrap().request_line, "DELETE /path HTTP/1.1");
    }

    #[test]
    fn request_sends_method_headers_body_and_reports_status() {
        let caps = caps();
        let cap = caps.get("net.http_request").unwrap();
        let (url, seen) = serve_once(404, "nope", Duration::ZERO);
        let headers = Value::new_map(HashMap::from([("X-Test".to_string(), s("yes"))]));
        let out = cap
            .call(vec![s("patch"), s(&url), s("{}"), headers])
            .unwrap();
        let Some(Value::Map(m)) = out else {
            panic!("expected a map, got {out:?}");
        };
        assert_eq!(m.borrow()["status"], Value::Int(404));
        assert_eq!(m.borrow()["body"], s("nope"));
        let seen = seen.recv().unwrap();
        assert_eq!(seen.request_line, "PATCH /path HTTP/1.1");
        assert!(
            seen.headers.iter().any(|h| h == "X-Test: yes"),
            "{:?}",
            seen.headers
        );
        assert_eq!(seen.body, "{}");

        assert!(
            cap.call(vec![s("GE T"), s(&url), Value::Null, Value::Null])
                .is_err()
        );
        assert!(
            cap.call(vec![s("GET"), s(&url), Value::Null, s("x")])
                .is_err()
        );
    }

    #[test]
    fn get_still_fails_on_an_error_status() {
        let (url, _seen) = serve_once(500, "boom", Duration::ZERO);
        let err = caps()
            .get("net.http_get")
            .unwrap()
            .call(vec![s(&url)])
            .unwrap_err();
        assert!(err.starts_with("net.http_get "), "{err}");
    }

    #[test]
    fn every_verb_times_out_at_the_deadline() {
        let caps = caps();
        let cases: [(&str, Vec<Value>); 5] = [
            ("net.http_get", vec![]),
            ("net.http_post", vec![s("b")]),
            ("net.http_put", vec![s("b")]),
            ("net.http_delete", vec![]),
            ("net.http_request", vec![]),
        ];
        for (name, extra) in cases {
            let (url, _seen) = serve_once(200, "late", Duration::from_secs(3));
            let args = if name == "net.http_request" {
                vec![s("GET"), s(&url), Value::Null, Value::Null]
            } else {
                std::iter::once(s(&url)).chain(extra).collect()
            };
            let start = std::time::Instant::now();
            let result = caps.get(name).unwrap().call_with_deadline(args, 150);
            assert!(matches!(result, Err(HostCapError::Timeout)), "{name}");
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "{name} waited out the server"
            );
        }
    }

    #[test]
    fn request_reports_reason_and_response_headers() {
        let caps = caps();
        let (url, _seen) = serve_once(404, "nope", Duration::ZERO);
        let out = caps
            .get("net.http_request")
            .unwrap()
            .call(vec![s("GET"), s(&url), Value::Null, Value::Null])
            .unwrap();
        let Some(Value::Map(m)) = out else {
            panic!("expected a map, got {out:?}");
        };
        let m = m.borrow();
        assert_eq!(m["status"], Value::Int(404));
        assert_eq!(m["reason"], s("X"));
        assert_eq!(m["body"], s("nope"));
        let Value::Map(h) = &m["headers"] else {
            panic!("headers must be a map, got {:?}", m["headers"]);
        };
        // Lower-cased names, whatever the server sent.
        assert_eq!(h.borrow()["content-length"], s("4"));
        assert_eq!(h.borrow()["connection"], s("close"));
    }

    #[test]
    fn timeout_ms_is_a_per_request_cap_failure() {
        let caps = caps();
        let opts = || Value::new_map(HashMap::from([("timeout_ms".to_string(), Value::Int(150))]));
        let cases: [(&str, Vec<Value>); 5] = [
            ("net.http_get", vec![]),
            ("net.http_post", vec![s("b")]),
            ("net.http_put", vec![s("b")]),
            ("net.http_delete", vec![]),
            ("net.http_request", vec![]),
        ];
        for (name, extra) in cases {
            let (url, _seen) = serve_once(200, "late", Duration::from_secs(3));
            let mut args = if name == "net.http_request" {
                vec![s("GET"), s(&url), Value::Null, Value::Null]
            } else {
                std::iter::once(s(&url)).chain(extra).collect()
            };
            args.push(opts());
            let start = std::time::Instant::now();
            // The wall budget (30 s) is far away: the request's own bound fires.
            let result = caps.get(name).unwrap().call_with_deadline(args, 30_000);
            match result {
                Err(HostCapError::Message(m)) => {
                    assert!(m.contains("timed out after 150 ms"), "{name}: {m}")
                }
                _ => panic!("{name}: expected a timeout message (CapFailed)"),
            }
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "{name} waited out the server"
            );
        }
    }

    #[test]
    fn wall_deadline_tighter_than_timeout_ms_stays_a_wall_timeout() {
        let (url, _seen) = serve_once(200, "late", Duration::from_secs(3));
        let opts = Value::new_map(HashMap::from([(
            "timeout_ms".to_string(),
            Value::Int(5_000),
        )]));
        let result = caps()
            .get("net.http_get")
            .unwrap()
            .call_with_deadline(vec![s(&url), opts], 150);
        assert!(matches!(result, Err(HostCapError::Timeout)));
    }

    // An unlimited wall budget (#505) reaches caps as u64::MAX; that must mean
    // "no agent timeout", not a Duration overflow inside ureq.
    #[test]
    fn unlimited_wall_deadline_does_not_overflow() {
        let (url, _seen) = serve_once(200, "ok", Duration::ZERO);
        let out = caps()
            .get("net.http_get")
            .unwrap()
            .call_with_deadline(vec![s(&url)], u64::MAX);
        assert!(matches!(out, Ok(Some(Value::Str(ref b))) if b == "ok"));
    }

    #[test]
    fn opts_and_arity_are_checked() {
        let caps = caps();
        let get = caps.get("net.http_get").unwrap();
        let (url, _seen) = serve_once(200, "ok", Duration::ZERO);
        // Null opts behaves like no opts.
        assert_eq!(get.call(vec![s(&url), Value::Null]), Ok(Some(s("ok"))));
        let bad = |k: &str, v: Value| Value::new_map(HashMap::from([(k.to_string(), v)]));
        let err = get
            .call(vec![
                s("http://127.0.0.1:9/"),
                bad("timeout", Value::Int(5)),
            ])
            .unwrap_err();
        assert!(err.contains("unknown opts key 'timeout'"), "{err}");
        let err = get
            .call(vec![
                s("http://127.0.0.1:9/"),
                bad("timeout_ms", Value::Int(0)),
            ])
            .unwrap_err();
        assert!(err.contains("positive integer"), "{err}");
        let err = get
            .call(vec![s("http://127.0.0.1:9/"), s("x")])
            .unwrap_err();
        assert!(err.contains("opts must be a map or null"), "{err}");
        let err = get.call(vec![]).unwrap_err();
        assert!(
            err.contains("expected 1 or 2 arguments (url, opts?), got 0"),
            "{err}"
        );
        let err = caps
            .get("net.http_request")
            .unwrap()
            .call(vec![s("GET"), s("u"), Value::Null])
            .unwrap_err();
        assert!(err.contains("expected 4 or 5 arguments"), "{err}");
    }

    // Source → VM: a per-request timeout is a fatal CapFailed, even inside
    // try/catch (ruling 5, #464), and the response map carries reason +
    // headers.
    #[test]
    fn timeout_and_headers_through_the_source_pipeline() {
        let caps = crate::HostCapsBuilder::new()
            .net(true)
            .net_max_response_bytes(4096)
            .build();

        let (slow, _s1) = serve_once(200, "late", Duration::from_secs(3));
        let source = format!(
            "fn main() {{\n\
             try {{ net.http_get(\"{slow}\", {{ timeout_ms: 100 }}) }} catch e {{ print(\"caught\") }}\n\
             print(\"after\")\n\
             }}\n"
        );
        let prog = crate::compile::compile_crush_source(&source).expect("compile");
        let err =
            crush_vm::run_with_caps(&prog, &crush_vm::Quotas::default(), Some(&caps)).unwrap_err();
        assert!(
            matches!(&err, crush_vm::VmError::CapFailed { cap, message }
                if cap == "net.http_get" && message.contains("timed out after 100 ms")),
            "{err:?}"
        );

        let (fast, _s2) = serve_once(201, "made", Duration::ZERO);
        let source = format!(
            "fn main() {{\n\
             let r = net.http_request(\"POST\", \"{fast}\", \"x\", null, {{ timeout_ms: 2000 }})\n\
             print(r.status)\n\
             print(r.reason)\n\
             print(r.headers[\"content-length\"])\n\
             print(r.body)\n\
             }}\n"
        );
        let prog = crate::compile::compile_crush_source(&source).expect("compile");
        let result =
            crush_vm::run_with_caps(&prog, &crush_vm::Quotas::default(), Some(&caps)).unwrap();
        assert_eq!(result.output, "201\nX\n4\nmade\n");
    }

    // Source → VM: `net.http_put` from Crush, against a local listener.
    #[test]
    fn http_put_through_the_source_pipeline() {
        let (url, seen) = serve_once(200, "stored", Duration::ZERO);
        let source = format!("io.print(net.http_put(\"{url}\", \"v1\"))\n");
        let prog = crate::compile::compile_crush_source(&source).expect("compile");
        let caps = crate::HostCapsBuilder::new()
            .net(true)
            .net_max_response_bytes(4096)
            .build();
        let result =
            crush_vm::run_with_caps(&prog, &crush_vm::Quotas::default(), Some(&caps)).unwrap();
        assert_eq!(result.output, "stored\n");
        assert_eq!(seen.recv().unwrap().body, "v1");

        let refused = crush_vm::run_with_caps(&prog, &crush_vm::Quotas::default(), None);
        assert!(refused.is_err(), "net.http_put must need the net grant");
    }
}
