//! Pure codec roundtrip test. Encodes a `mesh_proto::MeshRequest` into a TLV
//! byte buffer via `crush_net::encode_request`, decodes it back via
//! `crush_net::decode_request`, and asserts structural equality end-to-end.

use mesh_proto::MeshRequest;
use serde_json::json;

#[test]
fn mesh_request_full_tlv_roundtrip_with_correlation_id() {
    let original = MeshRequest {
        id: "req-1".to_string(),
        method: "echo".to_string(),
        params: json!({ "hello": "world", "n": 42 }),
        caller_did: "did:key:zTestRoundtrip".to_string(),
        correlation_id: Some("corr-1".to_string()),
    };

    let bytes = crush_net::encode_request(&original).expect("encode");
    let (decoded, n) = crush_net::decode_request(&bytes).expect("decode");
    assert_eq!(n, bytes.len(), "decoder must consume the entire buffer");
    assert_eq!(decoded.id, original.id);
    assert_eq!(decoded.method, original.method);
    assert_eq!(decoded.caller_did, original.caller_did);
    assert_eq!(decoded.correlation_id, original.correlation_id);
    assert_eq!(decoded.params, original.params);
}

#[test]
fn mesh_request_roundtrip_without_correlation_id() {
    let original = MeshRequest {
        id: "req-2".to_string(),
        method: "ping".to_string(),
        params: json!(null),
        caller_did: "did:key:zAnotherTest".to_string(),
        correlation_id: None,
    };

    let bytes = crush_net::encode_request(&original).expect("encode");
    let (decoded, n) = crush_net::decode_request(&bytes).expect("decode");
    assert_eq!(n, bytes.len());
    assert_eq!(decoded.id, original.id);
    assert_eq!(decoded.method, "ping");
    assert!(decoded.correlation_id.is_none());
    assert_eq!(decoded.params, json!(null));
}

#[test]
fn frame_codec_pure_roundtrip() {
    // Pure-Frame roundtrip via the public `Frame` surface.
    let frame = crush_net::Frame {
        typ: 1,
        flags: 0,
        payload: b"abc".to_vec(),
    };
    let mut buf = Vec::new();
    crush_net::encode_frame(&frame, &mut buf).unwrap();
    let (decoded, consumed) = crush_net::try_decode_frame(&buf).unwrap().unwrap();
    assert_eq!(consumed, buf.len());
    assert_eq!(decoded, frame);
}

#[test]
fn partial_frame_reports_none_not_err() {
    // A truncated buffer (only the header, no full payload yet) MUST report
    // Ok(None), not Err, so callers can buffer until the next chunk arrives.
    let frame = crush_net::Frame {
        typ: 0,
        flags: 1,
        payload: vec![1, 2, 3, 4, 5],
    };
    let mut buf = Vec::new();
    crush_net::encode_frame(&frame, &mut buf).unwrap();
    // Header is 6 bytes; payload starts at offset 6. Truncate below payload end.
    let partial = &buf[..9];
    assert!(matches!(
        crush_net::try_decode_frame(partial),
        Ok(None)
    ));
}
