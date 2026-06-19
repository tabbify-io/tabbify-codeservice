//! LSP base protocol framing: `Content-Length: N\r\n\r\n<json>`. Synchronous
//! `std::io` codec (the LSP client owns a dedicated thread per rust-analyzer
//! process, bridged to async via a channel in `mod.rs`).

use std::io::{BufRead, BufReader, Read};

/// Encode a JSON payload as one LSP frame (header + body).
pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 32);
    out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", payload.len()).as_bytes());
    out.extend_from_slice(payload);
    out
}

/// Reads length-prefixed LSP frames from any `Read`.
pub struct FrameReader<R: Read> {
    inner: BufReader<R>,
}

impl<R: Read> FrameReader<R> {
    pub fn new(r: R) -> Self {
        Self { inner: BufReader::new(r) }
    }

    /// Read one frame's JSON body. `Ok(None)` at clean EOF.
    pub fn read_frame(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let n = self.inner.read_line(&mut line)?;
            if n == 0 {
                return Ok(None); // EOF
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break; // end of headers
            }
            if let Some(v) = trimmed.strip_prefix("Content-Length:") {
                content_length = v.trim().parse().ok();
            }
        }
        let len = content_length.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "missing Content-Length")
        })?;
        let mut body = vec![0u8; len];
        self.inner.read_exact(&mut body)?;
        Ok(Some(body))
    }
}
