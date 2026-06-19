//! Frame-level test for the LSP wire codec. The real rust-analyzer round-trip
//! is an in-image acceptance test (Task 17); here we pin the `Content-Length`
//! framing both directions, which is the part most likely to silently break.
use std::io::Cursor;
use tabbify_codeservice::lsp::jsonrpc::{encode_frame, FrameReader};

#[test]
fn encode_prefixes_content_length() {
    let bytes = encode_frame(br#"{"jsonrpc":"2.0","id":1,"method":"x"}"#);
    let s = String::from_utf8(bytes).unwrap();
    assert!(s.starts_with("Content-Length: 37\r\n\r\n"), "got: {s:?}");
    assert!(s.ends_with(r#"{"jsonrpc":"2.0","id":1,"method":"x"}"#));
}

#[test]
fn reader_decodes_one_frame() {
    let payload = br#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
    let mut buf = Vec::new();
    buf.extend_from_slice(format!("Content-Length: {}\r\n\r\n", payload.len()).as_bytes());
    buf.extend_from_slice(payload);
    let mut reader = FrameReader::new(Cursor::new(buf));
    let frame = reader.read_frame().unwrap().unwrap();
    assert_eq!(frame, payload);
}
