//! Write methods. Every write is computed in memory then applied atomically
//! (temp file + rename per file), so a failure never leaves a half-written tree.
//! Symbol targeting uses the tree-sitter symbol map (Task 7); the agent edits
//! STRUCTURE, not blind text.

use std::io::Write as _;

use tabbify_workspace_contract::error::{CodeError, CodeErrorCode};
use tabbify_workspace_contract::rpc::{
    EditReq, EditResp, InsertSymbolReq, RenameSymbolReq, Replacement, ReplaceSymbolBodyReq,
};

use crate::paths::{repo_root, resolve, Root};
use crate::state::AppState;
use crate::treesitter::rust_symbols;

fn rebase(state: &AppState, abs: std::path::PathBuf) -> std::path::PathBuf {
    state.roots.projects.join(
        abs.strip_prefix(tabbify_workspace_contract::PROJECTS_DIR)
            .unwrap_or(&abs),
    )
}

/// Write `content` to `abs` atomically (temp + rename in the same dir).
fn atomic_write(abs: &std::path::Path, content: &str) -> Result<(), CodeError> {
    let dir = abs
        .parent()
        .ok_or_else(|| CodeError::new(CodeErrorCode::Internal, "target has no parent dir"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| CodeError::new(CodeErrorCode::Internal, format!("tmp: {e}")))?;
    tmp.write_all(content.as_bytes())
        .map_err(|e| CodeError::new(CodeErrorCode::Internal, format!("write: {e}")))?;
    tmp.persist(abs)
        .map_err(|e| CodeError::new(CodeErrorCode::Internal, format!("persist: {e}")))?;
    Ok(())
}

/// `edit{repo,path,replacements[]}` — string/range replacements in one file.
pub fn edit(state: &AppState, req: EditReq) -> Result<EditResp, CodeError> {
    let abs = rebase(state, resolve(Root::Projects, &req.repo, &req.path)?);
    let mut content = std::fs::read_to_string(&abs)
        .map_err(|_| CodeError::new(CodeErrorCode::NotFound, "no such file"))?;
    for r in &req.replacements {
        content = apply_replacement(&content, r)?;
    }
    atomic_write(&abs, &content)?;
    Ok(EditResp { changed_files: vec![req.path] })
}

/// Apply one replacement (exactly one of `old_string` / `range` must be set).
fn apply_replacement(content: &str, r: &Replacement) -> Result<String, CodeError> {
    match (&r.old_string, &r.range) {
        (Some(old), None) => {
            if !content.contains(old.as_str()) {
                return Err(CodeError::new(CodeErrorCode::NotFound, "old_string not found"));
            }
            Ok(content.replacen(old.as_str(), &r.new_string, 1))
        }
        (None, Some(range)) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = range.start.line as usize;
            let end = range.end.line as usize;
            if start > lines.len() || end > lines.len() || start > end {
                return Err(CodeError::new(CodeErrorCode::Invalid, "range out of bounds"));
            }
            let mut out: Vec<String> = Vec::new();
            out.extend(lines[..start].iter().map(|s| s.to_string()));
            out.push(r.new_string.clone());
            out.extend(lines[end..].iter().map(|s| s.to_string()));
            Ok(out.join("\n"))
        }
        _ => Err(CodeError::new(
            CodeErrorCode::Invalid,
            "exactly one of old_string / range must be set",
        )),
    }
}

/// Locate a symbol by `name_path` in a repo's rust files. Returns the CONFINED
/// repo root (validated through `paths::repo_root`/`safe_segment`), the
/// workspace-relative path, and the symbol's `[start,end]` line span. Callers
/// reuse the returned `root` rather than re-joining the unsanitized `req.repo`,
/// so a `repo:"../../etc"` is rejected here ONCE and never reaches `std::fs`.
fn locate_symbol(
    state: &AppState,
    repo: &str,
    name_path: &str,
) -> Result<(std::path::PathBuf, String, usize, usize), CodeError> {
    let want = name_path.rsplit('/').next().unwrap_or(name_path);
    let root = repo_root(&state.roots.projects, repo)?;
    for entry in walkdir::WalkDir::new(&root).into_iter().flatten() {
        if !entry.file_type().is_file()
            || entry.path().extension().map(|e| e != "rs").unwrap_or(true)
        {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let rel = entry
            .path()
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        for s in rust_symbols(&src, repo, &rel) {
            if s.name == want || s.name_path == name_path {
                return Ok((
                    root.clone(),
                    rel,
                    s.location.range.start.line as usize,
                    s.location.range.end.line as usize,
                ));
            }
        }
    }
    Err(CodeError::new(CodeErrorCode::NotFound, "symbol not found"))
}

/// `replace_symbol_body{repo,symbol,new_body}` — swap the symbol's line span.
pub fn replace_symbol_body(
    state: &AppState,
    req: ReplaceSymbolBodyReq,
) -> Result<EditResp, CodeError> {
    let (root, rel, start, end) = locate_symbol(state, &req.repo, &req.symbol)?;
    let abs = root.join(&rel);
    let content = std::fs::read_to_string(&abs)
        .map_err(|e| CodeError::new(CodeErrorCode::Internal, format!("read: {e}")))?;
    let lines: Vec<&str> = content.lines().collect();
    let mut out: Vec<String> = Vec::new();
    out.extend(lines[..start].iter().map(|s| s.to_string()));
    out.push(req.new_body);
    out.extend(lines[end + 1..].iter().map(|s| s.to_string()));
    atomic_write(&abs, &out.join("\n"))?;
    Ok(EditResp { changed_files: vec![rel] })
}

/// Shared body for insert-before / insert-after.
fn insert_symbol(state: &AppState, req: InsertSymbolReq, after: bool) -> Result<EditResp, CodeError> {
    let (root, rel, start, end) = locate_symbol(state, &req.repo, &req.symbol)?;
    let abs = root.join(&rel);
    let content = std::fs::read_to_string(&abs)
        .map_err(|e| CodeError::new(CodeErrorCode::Internal, format!("read: {e}")))?;
    let lines: Vec<&str> = content.lines().collect();
    let at = if after { end + 1 } else { start };
    let mut out: Vec<String> = Vec::new();
    out.extend(lines[..at].iter().map(|s| s.to_string()));
    out.push(req.content);
    out.extend(lines[at..].iter().map(|s| s.to_string()));
    atomic_write(&abs, &out.join("\n"))?;
    Ok(EditResp { changed_files: vec![rel] })
}

/// `insert_before_symbol{repo,symbol,content}`.
pub fn insert_before_symbol(state: &AppState, req: InsertSymbolReq) -> Result<EditResp, CodeError> {
    insert_symbol(state, req, false)
}

/// `insert_after_symbol{repo,symbol,content}`.
pub fn insert_after_symbol(state: &AppState, req: InsertSymbolReq) -> Result<EditResp, CodeError> {
    insert_symbol(state, req, true)
}

/// `rename_symbol{repo,path,position,new_name}` — single-file textual rename in
/// the MVP (the LSP cross-file rename is an additive Phase-2 upgrade with the
/// same `EditResp` shape). We rename the symbol whose definition spans
/// `position`, replacing whole-word occurrences in that file.
pub fn rename_symbol(state: &AppState, req: RenameSymbolReq) -> Result<EditResp, CodeError> {
    let abs = rebase(state, resolve(Root::Projects, &req.repo, &req.path)?);
    let content = std::fs::read_to_string(&abs)
        .map_err(|_| CodeError::new(CodeErrorCode::NotFound, "no such file"))?;
    let symbols = rust_symbols(&content, &req.repo, &req.path);
    let target = symbols
        .into_iter()
        .find(|s| {
            let r = s.location.range;
            req.position.line >= r.start.line && req.position.line <= r.end.line
        })
        .ok_or_else(|| CodeError::new(CodeErrorCode::NotFound, "no symbol at position"))?;
    if req.new_name.is_empty() {
        return Err(CodeError::new(CodeErrorCode::Invalid, "empty new_name"));
    }
    let renamed = replace_word(&content, &target.name, &req.new_name);
    atomic_write(&abs, &renamed)?;
    Ok(EditResp { changed_files: vec![req.path] })
}

/// Whole-word replace (avoids matching substrings inside other identifiers).
fn replace_word(content: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < content.len() {
        if content[i..].starts_with(from)
            && !ident_byte(bytes.get(i.wrapping_sub(1)).copied())
            && !ident_byte(bytes.get(i + from.len()).copied())
        {
            out.push_str(to);
            i += from.len();
        } else {
            let ch = content[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn ident_byte(b: Option<u8>) -> bool {
    matches!(b, Some(c) if c == b'_' || c.is_ascii_alphanumeric())
}
