//! READ/NAV methods that need no LSP: workspace_status, read, list_dir,
//! find_file, code_search, kb_list.

use tabbify_workspace_contract::error::{CodeError, CodeErrorCode};
use tabbify_workspace_contract::rpc::{
    DirEntry, IndexStatus, KbListReq, KbListResp, LangIndex, ListDirReq, ListDirResp, Range,
    ReadReq, ReadResp, RepoIndex, WorkspaceStatusReq, WorkspaceStatusResp,
};
use walkdir::WalkDir;

use crate::paths::{resolve, Root};
use crate::repo::discover_repos;
use crate::state::AppState;

/// `workspace_status{}` → repos + per-(repo,lang) index status.
///
/// Reports `Ready` once the LSP has signalled its first finished index (the
/// global `state.ready` flag mirrors `LspClient::ready`); until then every
/// detected language reports `Indexing` and symbol queries route to the
/// tree-sitter fallback. This flag is node's cold→warm snapshot trigger.
pub fn workspace_status(state: &AppState, _req: WorkspaceStatusReq) -> WorkspaceStatusResp {
    let status = if state.ready.load(std::sync::atomic::Ordering::SeqCst) {
        IndexStatus::Ready
    } else {
        IndexStatus::Indexing
    };
    let repos = discover_repos(&state.roots.projects)
        .into_iter()
        .map(|r| RepoIndex {
            repo: r.name,
            languages: r
                .languages
                .into_iter()
                .map(|lang| LangIndex { lang, status })
                .collect(),
        })
        .collect();
    WorkspaceStatusResp { repos }
}

/// Map an `io::Error` to the wire taxonomy: missing → not_found, else internal.
fn io_err(e: std::io::Error) -> CodeError {
    if e.kind() == std::io::ErrorKind::NotFound {
        CodeError::new(CodeErrorCode::NotFound, "no such path")
    } else {
        CodeError::new(CodeErrorCode::Internal, format!("io: {e}"))
    }
}

/// `read{repo,path,range?}` → raw content, optionally a `[start,end)` line slice.
pub fn read_file(state: &AppState, req: ReadReq) -> Result<ReadResp, CodeError> {
    // `resolve` performs the lexical confinement check against the contract
    // constant; we then re-base onto `state.roots.projects` so tests can point
    // at a temp dir (in prod `roots.projects == PROJECTS_DIR`, a no-op).
    let abs = resolve(Root::Projects, &req.repo, &req.path)?;
    let abs = state.roots.projects.join(
        abs.strip_prefix(tabbify_workspace_contract::PROJECTS_DIR)
            .unwrap_or(&abs),
    );
    let content = std::fs::read_to_string(&abs).map_err(io_err)?;
    let sliced = match req.range {
        None => content,
        Some(r) => slice_lines(&content, r),
    };
    Ok(ReadResp { content: sliced })
}

/// Slice `[start.line, end.line)` (0-based, half-open) by whole lines — a
/// token-cheap approximation of a character range, sufficient for `read`.
fn slice_lines(content: &str, r: Range) -> String {
    let start = r.start.line as usize;
    let end = r.end.line as usize;
    content
        .lines()
        .enumerate()
        .filter(|(i, _)| *i >= start && *i < end)
        .map(|(_, l)| l)
        .collect::<Vec<_>>()
        .join("\n")
}

/// `list_dir{repo,path?}` → one directory level (path defaults to repo root).
pub fn list_dir(state: &AppState, req: ListDirReq) -> Result<ListDirResp, CodeError> {
    let rel = req.path.unwrap_or_default();
    let abs = resolve(Root::Projects, &req.repo, &rel)?;
    let abs = state.roots.projects.join(
        abs.strip_prefix(tabbify_workspace_contract::PROJECTS_DIR)
            .unwrap_or(&abs),
    );
    let rd = std::fs::read_dir(&abs).map_err(io_err)?;
    let mut entries: Vec<DirEntry> = rd
        .flatten()
        .map(|e| DirEntry {
            name: e.file_name().to_string_lossy().into_owned(),
            is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ListDirResp { entries })
}

/// `kb_list{}` → markdown files relative to `KNOWLEDGE_DIR`, recursive.
pub fn kb_list(state: &AppState, _req: KbListReq) -> Result<KbListResp, CodeError> {
    let root = &state.roots.knowledge;
    let mut files = Vec::new();
    for entry in WalkDir::new(root).into_iter().flatten() {
        if entry.file_type().is_file()
            && entry.path().extension().map(|x| x == "md").unwrap_or(false)
        {
            if let Ok(rel) = entry.path().strip_prefix(root) {
                files.push(rel.to_string_lossy().into_owned());
            }
        }
    }
    files.sort();
    Ok(KbListResp { files })
}
