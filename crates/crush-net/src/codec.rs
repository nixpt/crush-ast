//! TLV (Type-Length-Value) wire format used by `crush-net`.
//!
//! Frame layout (big-endian, total = 6 + payload):
//!
//! | offset | size | field     |
//! |--------|------|-----------|
//! |   0    |  4   | `len`     | u32 payload length
//! |   4    |  1   | `typ`     | frame type (1 = MeshRequest)
//! |   5    |  1   | `flags`   | reserved
//! |   6    |  `len` | `payload` | opaque bytes
//!
//! `try_decode_frame` returns `Ok(None)` for partial frames so callers can
//! buffer until the next chunk. MAX_FRAME_SIZE caps allocations so a hostile
//! peer cannot trick the decoder into multi-GiB allocation on a 0xFFFFFFFF
//! length field.

use std::io;

pub const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024; // 16 MiB

pub const FRAME_TYPE_MESH_REQUEST: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub typ: u8,
    pub flags: u8,
    pub payload: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("codec: payload too large: {got} > {max}")]
    TooLarge { got: usize, max: usize },
    #[error("codec: frame truncated")]
    Truncated,
    #[error("codec: unexpected frame type {0}")]
    UnexpectedType(u8),
    #[error("mesh-proto: {0}")]
    Mesh(String),
    #[error("uri: {0}")]
    Uri(String),
}

pub fn encode_frame(frame: &Frame, buf: &mut Vec<u8>) -> Result<(), NetError> {
    if frame.payload.len() > MAX_FRAME_SIZE {
        return Err(NetError::TooLarge {
            got: frame.payload.len(),
            max: MAX_FRAME_SIZE,
        });
    }
    let len = frame.payload.len() as u32;
    buf.reserve(4 + 2 + frame.payload.len());
    buf.extend_from_slice(&len.to_be_bytes());
    buf.push(frame.typ);
    buf.push(frame.flags);
    buf.extend_from_slice(&frame.payload);
    Ok(())
}

pub fn try_decode_frame(buf: &[u8]) -> Result<Option<(Frame, usize)>, NetError> {
    if buf.len() < 6 {
        return Ok(None);
    }
    let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(NetError::TooLarge {
            got: len,
            max: MAX_FRAME_SIZE,
        });
    }
    let total = 4 + 2 + len;
    if buf.len() < total {
        return Ok(None);
    }
    let typ = buf[4];
    let flags = buf[5];
    let payload = buf[6..total].to_vec();
    Ok(Some((Frame { typ, flags, payload }, total)))
}

pub fn encode_request(req: &mesh_proto::MeshRequest) -> Result<Vec<u8>, NetError> {
    let payload = serde_json::to_vec(req).map_err(|e| NetError::Mesh(e.to_string()))?;
    let frame = Frame {
        typ: FRAME_TYPE_MESH_REQUEST,
        flags: 0,
        payload,
    };
    let mut buf = Vec::new();
    encode_frame(&frame, &mut buf)?;
    Ok(buf)
}

pub fn decode_request(buf: &[u8]) -> Result<(mesh_proto::MeshRequest, usize), NetError> {
    let (frame, n) = try_decode_frame(buf)?.ok_or(NetError::Truncated)?;
    if frame.typ != FRAME_TYPE_MESH_REQUEST {
        return Err(NetError::UnexpectedType(frame.typ));
    }
    let req: mesh_proto::MeshRequest =
        serde_json::from_slice(&frame.payload).map_err(|e| NetError::Mesh(e.to_string()))?;
    Ok((req, n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_proto::MeshRequest;
    use serde_json::json;

    #[test]
    fn round_trip_empty_payload() {
        let frame = Frame { typ: 0, flags: 0, payload: vec![] };
        let mut buf = Vec::new();
        encode_frame(&frame, &mut buf).unwrap();
        assert_eq!(buf.len(), 6);
        let (decoded, n) = try_decode_frame(&buf).unwrap().unwrap();
        assert_eq!(n, 6);
        assert_eq!(decoded, frame);
    }

    #[test]
    fn partial_buffer_returns_none() {
        let frame = Frame {
            typ: 7,
            flags: 1,
            payload: vec![1, 2, 3, 4, 5],
        };
        let mut buf = Vec::new();
        encode_frame(&frame, &mut buf).unwrap();
        assert!(try_decode_frame(&buf[..3]).unwrap().is_none());
        assert!(try_decode_frame(&buf[..8]).unwrap().is_none());
        let (decoded, n) = try_decode_frame(&buf).unwrap().unwrap();
        assert_eq!(n, buf.len());
        assert_eq!(decoded, frame);
    }

    #[test]
    fn rejects_oversize_length() {
        let mut bad = vec![0xFF, 0xFF, 0xFF, 0xFF, 0, 0];
        bad.extend_from_slice(&[0u8; 8]);
        let err = try_decode_frame(&bad).unwrap_err();
        match err {
            NetError::TooLarge { .. } => {}
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[test]
    fn mesh_request_encode_decode() {
        let original = MeshRequest {
            id: "req-1".to_string(),
            method: "echo".to_string(),
            params: json!({ "hello": "world", "n": 42 }),
            caller_did: "did:key:zTest".to_string(),
            correlation_id: Some("corr-1".to_string()),
        };
        let bytes = encode_request(&original).unwrap();
        let (decoded, n) = decode_request(&bytes).unwrap();
        assert_eq!(n, bytes.len());
        assert_eq!(decoded.id, original.id);
        assert_eq!(decoded.method, original.method);
        assert_eq!(decoded.caller_did, original.caller_did);
        assert_eq!(decoded.correlation_id, original.correlation_id);
        assert_eq!(decoded.params, original.params);
    }
}
