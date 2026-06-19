//! Client for the privileged broker socket (`BROKER_SOCKET`). Sends one tagged
//! request, reads one `CodeResponse<T>` line. The codeservice never holds a
//! credential — it asks the broker to perform the op and returns the result.
//!
//! The request type is the SHARED `tabbify_broker::BrokerRequest` (§12 S2: T1
//! owns the canonical `#[serde(tag="kind")]` enum in the broker crate). The
//! codeservice path-depends on `tabbify-broker` for THIS one wire type so a
//! divergent rename is a compile error, not a silent prod break.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use serde::de::DeserializeOwned;
use tabbify_workspace_contract::error::{CodeError, CodeErrorCode};
use tabbify_workspace_contract::rpc::CodeResponse;

pub use tabbify_broker::BrokerRequest;

/// The broker socket path. Production is the frozen contract constant
/// `BROKER_SOCKET`; `TABBIFY_BROKER_SOCKET` overrides it so integration tests can
/// point at a temp socket and drive a REAL broker (never a fake). The override is
/// process-local and changes only the connect target — never the wire/envelope.
fn socket_path() -> String {
    std::env::var("TABBIFY_BROKER_SOCKET")
        .unwrap_or_else(|_| tabbify_workspace_contract::BROKER_SOCKET.to_string())
}

/// Send a typed broker request (serializes to `{"kind":…, …payload}`) and decode
/// `CodeResponse<T>`.
pub fn call<T: DeserializeOwned>(request: &BrokerRequest) -> Result<T, CodeError> {
    let mut stream = UnixStream::connect(socket_path())
        .map_err(|e| CodeError::new(CodeErrorCode::Internal, format!("broker connect: {e}")))?;
    let mut payload = serde_json::to_vec(request)
        .map_err(|e| CodeError::new(CodeErrorCode::Internal, format!("broker encode: {e}")))?;
    payload.push(b'\n');
    stream
        .write_all(&payload)
        .map_err(|e| CodeError::new(CodeErrorCode::Internal, format!("broker write: {e}")))?;
    let mut line = String::new();
    BufReader::new(&stream)
        .read_line(&mut line)
        .map_err(|e| CodeError::new(CodeErrorCode::Internal, format!("broker read: {e}")))?;
    let resp: CodeResponse<T> = serde_json::from_str(line.trim())
        .map_err(|e| CodeError::new(CodeErrorCode::Internal, format!("broker decode: {e}")))?;
    resp.into_result()
}
