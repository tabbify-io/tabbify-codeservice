//! rust-analyzer client: spawns the LSP, performs `initialize`, and provides a
//! real request/response path (`request()` + `did_open()`) with id-correlation.
//! v1 USES this path for `textDocument/references` (the find_references accept
//! proof). `documentSymbol`/`workspace/symbol`/`rename` requests are additive
//! Phase-2 upgrades on the SAME `request()` primitive (the framing + correlation
//! are already exercised by references, so they are wiring, not new machinery).
//! The struct buffers responses by id; `request()` blocks until its id arrives.
//!
//! Readiness: rust-analyzer publishes progress via `$/progress`. We flip the
//! repo's `ready` flag ONLY when the cache-priming/indexing work TOKEN reports
//! `end` (not on every progress-end — see `is_index_done`) — THIS is the
//! snapshot trigger node consumes post-index (spec §3.4 / §12). Until then,
//! symbol queries route to the tree-sitter fallback.

pub mod jsonrpc;

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tabbify_workspace_contract::error::{CodeError, CodeErrorCode};

use jsonrpc::{encode_frame, FrameReader};

/// A live rust-analyzer process scoped to one repo root.
pub struct LspClient {
    child: Mutex<Child>,
    next_id: Mutex<i64>,
    /// Pending request id → response slot, filled by the reader thread.
    pending: Arc<Mutex<std::collections::HashMap<i64, Value>>>,
    /// Flips true on first index-complete progress notification.
    pub ready: Arc<std::sync::atomic::AtomicBool>,
}

impl LspClient {
    /// Spawn rust-analyzer rooted at `repo_root` and run `initialize`.
    pub fn spawn(repo_root: &std::path::Path) -> Result<Arc<Self>, CodeError> {
        let mut child = Command::new("rust-analyzer")
            .current_dir(repo_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| CodeError::new(CodeErrorCode::Internal, format!("spawn ra: {e}")))?;

        let stdout = child.stdout.take().expect("ra stdout");
        let pending: Arc<Mutex<std::collections::HashMap<i64, Value>>> = Default::default();
        let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Reader thread: demux responses (by id) and watch progress for ready.
        let pending_r = pending.clone();
        let ready_r = ready.clone();
        std::thread::spawn(move || {
            let mut reader = FrameReader::new(stdout);
            while let Ok(Some(frame)) = reader.read_frame() {
                let Ok(msg) = serde_json::from_slice::<Value>(&frame) else {
                    continue;
                };
                if let Some(id) = msg.get("id").and_then(|i| i.as_i64()) {
                    pending_r.lock().unwrap().insert(id, msg);
                } else if is_index_done(&msg) {
                    ready_r.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
        });

        let client = Arc::new(Self {
            child: Mutex::new(child),
            next_id: Mutex::new(1),
            pending,
            ready,
        });
        client.initialize(repo_root)?;
        Ok(client)
    }

    /// Send `initialize` + `initialized`. We do not block on a reply here; the
    /// reader thread captures it and ready is driven by progress.
    ///
    /// §4 build-script-RCE mitigation: rust-analyzer runs as the SAME `agent`
    /// uid that is the broker-socket client, so executing an attacker's
    /// `build.rs`/proc-macro would let it drive the broker. We therefore DISABLE
    /// build-script + proc-macro execution in the LSP (`cargo.buildScripts.enable
    /// = false`, `procMacro.enable = false`) via `initializationOptions` — the
    /// spec's "disable build-script/proc-macro execution" option (§4). Symbol
    /// nav + references for the dogfood repo do not need expanded proc-macros;
    /// if a future repo does, the alternative (throwaway-uid LSP with no broker
    /// path) is the documented upgrade, NOT re-enabling code execution.
    fn initialize(&self, repo_root: &std::path::Path) -> Result<(), CodeError> {
        let uri = format!("file://{}", repo_root.display());
        let id = self.alloc_id();
        self.send(&json!({
            "jsonrpc":"2.0","id":id,"method":"initialize",
            "params":{
                "processId":null,
                "rootUri":uri,
                "capabilities":{},
                "initializationOptions":{
                    "cargo": { "buildScripts": { "enable": false } },
                    "procMacro": { "enable": false }
                }
            }
        }))?;
        self.send(&json!({"jsonrpc":"2.0","method":"initialized","params":{}}))?;
        Ok(())
    }

    fn alloc_id(&self) -> i64 {
        let mut g = self.next_id.lock().unwrap();
        let id = *g;
        *g += 1;
        id
    }

    /// Write one JSON-RPC message to rust-analyzer's stdin.
    fn send(&self, msg: &Value) -> Result<(), CodeError> {
        let bytes = encode_frame(serde_json::to_vec(msg).unwrap().as_slice());
        let mut child = self.child.lock().unwrap();
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| CodeError::new(CodeErrorCode::Internal, "ra stdin closed"))?;
        stdin
            .write_all(&bytes)
            .and_then(|_| stdin.flush())
            .map_err(|e| CodeError::new(CodeErrorCode::Internal, format!("ra write: {e}")))
    }

    /// True once rust-analyzer has finished its first index.
    pub fn is_ready(&self) -> bool {
        self.ready.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Notify `textDocument/didOpen` so rust-analyzer has the buffer in memory
    /// before a request references it (cheap; idempotent re-opens are fine).
    pub fn did_open(&self, file_uri: &str, text: &str) -> Result<(), CodeError> {
        self.send(&json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen",
            "params":{"textDocument":{
                "uri": file_uri, "languageId":"rust", "version":1, "text": text
            }}
        }))
    }

    /// Issue ONE JSON-RPC request and BLOCK (bounded) until the reader thread
    /// files the matching id, then return `result` (or a typed error). This is
    /// the real request/response correlation path — `find_references` uses it to
    /// drive `textDocument/references`, the v1 acceptance proof.
    pub fn request(
        &self,
        method: &str,
        params: Value,
        timeout: std::time::Duration,
    ) -> Result<Value, CodeError> {
        let id = self.alloc_id();
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(msg) = self.pending.lock().unwrap().remove(&id) {
                if let Some(err) = msg.get("error") {
                    return Err(CodeError::new(
                        CodeErrorCode::Internal,
                        format!("lsp {method} error: {err}"),
                    ));
                }
                return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
            }
            if std::time::Instant::now() >= deadline {
                return Err(CodeError::new(
                    CodeErrorCode::Internal,
                    format!("lsp {method} timed out"),
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

/// Recognise the indexing/cache-priming `$/progress` `end` notification. We only
/// flip `ready` on the END of rust-analyzer's CACHE-PRIMING/INDEXING work token
/// — NOT on every `$/progress end` (rust-analyzer emits many per workspace-load
/// step, several of which fire long before symbols are queryable). The token
/// title carries "roots scanned"/"Indexing"/"cachePriming"; we match those so a
/// premature `ready` cannot trigger a half-indexed snapshot (review/§12 fix).
fn is_index_done(msg: &Value) -> bool {
    if msg.get("method").and_then(|m| m.as_str()) != Some("$/progress") {
        return false;
    }
    let value = &msg["params"]["value"];
    if value["kind"].as_str() != Some("end") {
        return false;
    }
    // The progress TOKEN identifies the work unit. rust-analyzer uses
    // "rustAnalyzer/cachePriming" and "rustAnalyzer/Indexing"; accept either.
    let token = msg["params"]["token"].as_str().unwrap_or_default();
    token.contains("cachePriming") || token.contains("Indexing") || token.contains("indexing")
}

#[cfg(test)]
mod tests {
    use super::is_index_done;
    use serde_json::json;

    #[test]
    fn ready_only_on_indexing_token_end() {
        // A non-indexing progress-end (e.g. "Roots Scanned" / fetch) must NOT
        // flip ready — that was the premature-snapshot bug (review/§12).
        let other = json!({"method":"$/progress","params":{
            "token":"rustAnalyzer/Roots Scanned","value":{"kind":"end"}}});
        assert!(!is_index_done(&other));

        let priming = json!({"method":"$/progress","params":{
            "token":"rustAnalyzer/cachePriming","value":{"kind":"end"}}});
        assert!(is_index_done(&priming));

        let indexing_begin = json!({"method":"$/progress","params":{
            "token":"rustAnalyzer/Indexing","value":{"kind":"begin"}}});
        assert!(!is_index_done(&indexing_begin)); // begin != end
    }
}
