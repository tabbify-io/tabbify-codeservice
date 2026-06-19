//! One rust-analyzer per repo, lazily spawned + cached. The manager owns the
//! repo→`LspClient` map and exposes `client(repo_root)`; a spawn failure (no
//! `rust-analyzer` on PATH) returns `None` so callers fall back to tree-sitter.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::LspClient;

/// Lazy per-repo LSP registry.
#[derive(Default)]
pub struct LspManager {
    clients: Mutex<HashMap<String, Arc<LspClient>>>,
}

impl LspManager {
    /// Get-or-spawn the LSP for `repo` rooted at `repo_root`. `None` if
    /// rust-analyzer is unavailable (→ tree-sitter fallback).
    pub fn client(&self, repo: &str, repo_root: &Path) -> Option<Arc<LspClient>> {
        let mut g = self.clients.lock().unwrap();
        if let Some(c) = g.get(repo) {
            return Some(c.clone());
        }
        match LspClient::spawn(repo_root) {
            Ok(c) => {
                g.insert(repo.to_string(), c.clone());
                Some(c)
            }
            Err(_) => None,
        }
    }

    /// True if `repo`'s LSP exists and reports a finished index.
    pub fn is_ready(&self, repo: &str) -> bool {
        self.clients
            .lock()
            .unwrap()
            .get(repo)
            .map(|c| c.is_ready())
            .unwrap_or(false)
    }
}
