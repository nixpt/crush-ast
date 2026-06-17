//! Network host capabilities for the CRUSH runtime.
//!
//! Enabled by the `net` cargo feature. Provides simple HTTP GET/POST via
//! `ureq` with bounded response sizes.

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

/// Add network capabilities to an existing [`HostCaps`] registry.
pub fn register(caps: &mut HostCaps, max_response_bytes: usize) {
    caps.register(Box::new(NetHttpGetCap::new(max_response_bytes)));
    caps.register(Box::new(NetHttpPostCap::new(max_response_bytes)));
}

pub struct NetHttpGetCap {
    max_response_bytes: usize,
}

impl NetHttpGetCap {
    pub fn new(max_response_bytes: usize) -> Self {
        Self { max_response_bytes }
    }
}

impl HostCap for NetHttpGetCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "net.http_get".to_string(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let url = crate::caps::value_as_text(&args[0]);
        let response = ureq::get(&url)
            .call()
            .map_err(|e| format!("net.http_get {url}: {e}"))?;
        let body = read_limited(response, self.max_response_bytes)?;
        Ok(Some(Value::Str(body)))
    }
}

pub struct NetHttpPostCap {
    max_response_bytes: usize,
}

impl NetHttpPostCap {
    pub fn new(max_response_bytes: usize) -> Self {
        Self { max_response_bytes }
    }
}

impl HostCap for NetHttpPostCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "net.http_post".to_string(),
            argc: Some(2),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let url = crate::caps::value_as_text(&args[0]);
        let body = crate::caps::value_as_text(&args[1]);
        let response = ureq::post(&url)
            .send_string(&body)
            .map_err(|e| format!("net.http_post {url}: {e}"))?;
        let body = read_limited(response, self.max_response_bytes)?;
        Ok(Some(Value::Str(body)))
    }
}

fn read_limited(response: ureq::Response, max_bytes: usize) -> Result<String, String> {
    use std::io::Read;
    let mut reader = response.into_reader().take(max_bytes as u64 + 1);
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .map_err(|e| format!("failed to read response: {e}"))?;
    if buf.len() > max_bytes {
        return Err(format!("response exceeded {max_bytes} bytes"));
    }
    String::from_utf8(buf).map_err(|e| format!("response is not valid UTF-8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_get_spec() {
        let cap = NetHttpGetCap::new(4096);
        assert_eq!(cap.spec().name, "net.http_get");
        assert_eq!(cap.spec().argc, Some(1));
        assert!(cap.spec().returns);
    }
}
