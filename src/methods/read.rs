//! READ/NAV methods that need no LSP: workspace_status, read, list_dir,
//! find_file, code_search, kb_list.

use tabbify_workspace_contract::rpc::{
    IndexStatus, LangIndex, RepoIndex, WorkspaceStatusReq, WorkspaceStatusResp,
};

use crate::repo::discover_repos;
use crate::state::AppState;

/// `workspace_status{}` → repos + per-(repo,lang) index status.
///
/// In v1 the LSP is not yet wired, so every detected language reports
/// `Indexing` (rust) — `Ready` arrives once Task 8 lands the LSP signal.
pub fn workspace_status(state: &AppState, _req: WorkspaceStatusReq) -> WorkspaceStatusResp {
    let repos = discover_repos(&state.roots.projects)
        .into_iter()
        .map(|r| RepoIndex {
            repo: r.name,
            languages: r
                .languages
                .into_iter()
                .map(|lang| LangIndex { lang, status: IndexStatus::Indexing })
                .collect(),
        })
        .collect();
    WorkspaceStatusResp { repos }
}
