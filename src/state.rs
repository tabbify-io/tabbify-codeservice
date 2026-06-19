//! Shared application state.
//!
//! `CodeRoots` decouples the confinement roots from the frozen contract
//! constants so integration tests can point at a temp dir. Production builds
//! `CodeRoots::from_contract()` (= `PROJECTS_DIR` / `KNOWLEDGE_DIR`).

use std::path::PathBuf;
use std::sync::Arc;

use tabbify_workspace_contract::{KNOWLEDGE_DIR, PROJECTS_DIR};

/// Filesystem roots the service is confined to.
#[derive(Debug, Clone)]
pub struct CodeRoots {
    pub projects: PathBuf,
    pub knowledge: PathBuf,
}

impl CodeRoots {
    /// Production roots from the frozen contract constants.
    pub fn from_contract() -> Self {
        Self {
            projects: PathBuf::from(PROJECTS_DIR),
            knowledge: PathBuf::from(KNOWLEDGE_DIR),
        }
    }
}

/// Process-wide shared state behind an `Arc` (axum `State`).
#[derive(Clone)]
pub struct AppState {
    pub roots: Arc<CodeRoots>,
    pub user_id: Arc<String>,
}

impl AppState {
    pub fn new(roots: CodeRoots, user_id: String) -> Self {
        Self { roots: Arc::new(roots), user_id: Arc::new(user_id) }
    }
}
