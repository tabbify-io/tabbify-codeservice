//! Path confinement: every workspace path is resolved under
//! `PROJECTS_DIR ∪ KNOWLEDGE_DIR`. A lexical normalization rejects any `..`
//! component or absolute segment BEFORE touching the filesystem, so traversal
//! and absolute-escape can never reach `std::fs`. We normalize lexically
//! (not via `canonicalize`) so confinement holds even for not-yet-created files.

use std::path::{Component, Path, PathBuf};

use tabbify_workspace_contract::error::{CodeError, CodeErrorCode};
use tabbify_workspace_contract::{KNOWLEDGE_DIR, PROJECTS_DIR};

/// Which confinement root a request targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Root {
    /// `~/projects/<repo>` — `repo` is required and non-empty.
    Projects,
    /// `~/knowledge` — flat KB root; `repo` is ignored (pass `""`).
    Knowledge,
}

/// Resolve `(root, repo, rel)` to an absolute path confined under the root.
///
/// - `Root::Projects` requires a non-empty `repo`; the path is
///   `PROJECTS_DIR/<repo>/<rel>`.
/// - `Root::Knowledge` ignores `repo`; the path is `KNOWLEDGE_DIR/<rel>`.
///
/// Any `..`, root, or prefix component in `repo`/`rel` → `Forbidden`.
pub fn resolve(root: Root, repo: &str, rel: &str) -> Result<PathBuf, CodeError> {
    let base = match root {
        Root::Projects => {
            if repo.is_empty() {
                return Err(CodeError::new(CodeErrorCode::Invalid, "repo is required"));
            }
            PathBuf::from(PROJECTS_DIR).join(safe_segment(repo)?)
        }
        Root::Knowledge => PathBuf::from(KNOWLEDGE_DIR),
    };
    let mut out = base.clone();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(seg) => out.push(seg),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CodeError::new(
                    CodeErrorCode::Forbidden,
                    "path escapes the workspace root",
                ));
            }
        }
    }
    // Defense in depth: the assembled path must still start with `base`.
    if !out.starts_with(&base) {
        return Err(CodeError::new(
            CodeErrorCode::Forbidden,
            "path escapes the workspace root",
        ));
    }
    Ok(out)
}

/// A single path segment (a repo name) with no separators or traversal. PUBLIC
/// so every fs-touching path (read, search, symbols, edit, git) enforces repo
/// confinement through THIS one function — never an ad-hoc re-implementation.
pub fn safe_segment(seg: &str) -> Result<&str, CodeError> {
    if seg.is_empty() {
        return Err(CodeError::new(CodeErrorCode::Invalid, "repo is required"));
    }
    let mut comps = Path::new(seg).components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(s)), None) if s == seg => Ok(seg),
        _ => Err(CodeError::new(CodeErrorCode::Forbidden, "invalid repo name")),
    }
}

/// Resolve a confined repo ROOT (`<projects_root>/<repo>`) for the multi-file
/// walks (find_symbol, locate_symbol). `repo` is validated through
/// [`safe_segment`]; `projects_root` is the live (possibly test) root so the
/// confinement check and the I/O target never diverge. Use this anywhere a path
/// is built from a caller-supplied `repo` WITHOUT a `rel` component.
pub fn repo_root(projects_root: &Path, repo: &str) -> Result<PathBuf, CodeError> {
    Ok(projects_root.join(safe_segment(repo)?))
}
