//! Tabbify code-service: in-FC, unprivileged Serena-grade code intelligence.
//!
//! Implements `tabbify_workspace_contract::rpc::CodeServiceRpc` over HTTP/JSON
//! on :8731. Every path is validated under `~/projects ∪ ~/knowledge` so a
//! traversal escape is `forbidden`, never served.

pub mod methods;
pub mod paths;
pub mod repo;
pub mod router;
pub mod state;
