//! Subprocess-backed search: ripgrep for `code_search`, fd for `find_file`.
//! Both run with cwd = the projects root, so emitted paths are workspace-
//! relative (`<repo>/<rel>`). We never pass user input as a flag (it is the
//! positional pattern), and ripgrep/fd never follow out of the cwd.

use std::process::Command;

use tabbify_workspace_contract::error::{CodeError, CodeErrorCode};
use tabbify_workspace_contract::rpc::{
    CodeSearchReq, CodeSearchResp, FindFileReq, FindFileResp, SearchMatch,
};

use crate::state::AppState;

fn internal(msg: impl Into<String>) -> CodeError {
    CodeError::new(CodeErrorCode::Internal, msg)
}

/// Resolve the fd binary name (`fd` on most distros, `fdfind` on Debian/Ubuntu).
fn fd_bin() -> Option<&'static str> {
    if which::which("fd").is_ok() {
        Some("fd")
    } else if which::which("fdfind").is_ok() {
        Some("fdfind")
    } else {
        None
    }
}

/// `code_search{query,glob?,max?}` → ripgrep with JSON output, cwd=projects.
pub fn code_search(state: &AppState, req: CodeSearchReq) -> Result<CodeSearchResp, CodeError> {
    if req.query.is_empty() {
        return Err(CodeError::new(CodeErrorCode::Invalid, "empty query"));
    }
    let max = req.max.unwrap_or(200);
    let mut cmd = Command::new("rg");
    cmd.current_dir(&state.roots.projects)
        .arg("--json")
        .arg("--max-count")
        .arg(max.to_string());
    if let Some(g) = &req.glob {
        cmd.arg("--glob").arg(g);
    }
    cmd.arg("--").arg(&req.query);
    let out = cmd.output().map_err(|e| internal(format!("rg spawn: {e}")))?;
    // rg exits 1 on "no matches" — that is a valid empty result, not an error.
    if !out.status.success() && out.status.code() != Some(1) {
        return Err(internal(format!(
            "rg failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let matches = parse_rg_json(&out.stdout, max as usize);
    Ok(CodeSearchResp { matches })
}

/// Parse ripgrep's `--json` stream into `SearchMatch`es. Each `"type":"match"`
/// line carries `data.path.text`, `data.line_number`, and the matched text.
fn parse_rg_json(stdout: &[u8], max: usize) -> Vec<SearchMatch> {
    let mut out = Vec::new();
    for line in stdout.split(|b| *b == b'\n') {
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("match") {
            continue;
        }
        let data = &v["data"];
        let rel = data["path"]["text"].as_str().unwrap_or_default().to_string();
        let (repo, path) = split_repo(&rel);
        let line_no = data["line_number"].as_u64().unwrap_or(0) as u32;
        let text = data["lines"]["text"]
            .as_str()
            .unwrap_or_default()
            .trim_end()
            .to_string();
        let column = data["submatches"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|m| m["start"].as_u64())
            .map(|c| c as u32 + 1);
        out.push(SearchMatch { repo, path, line: line_no, column, text });
        if out.len() >= max {
            break;
        }
    }
    out
}

/// `find_file{pattern}` → fd, cwd=projects; returns workspace-relative paths.
pub fn find_file(state: &AppState, req: FindFileReq) -> Result<FindFileResp, CodeError> {
    let bin = fd_bin().ok_or_else(|| internal("fd not installed"))?;
    let out = Command::new(bin)
        .current_dir(&state.roots.projects)
        .arg("--type")
        .arg("f")
        .arg("--")
        .arg(&req.pattern)
        .output()
        .map_err(|e| internal(format!("fd spawn: {e}")))?;
    if !out.status.success() {
        return Err(internal(format!(
            "fd failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let paths = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim_start_matches("./").to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Ok(FindFileResp { paths })
}

/// Split a `<repo>/<rest>` workspace-relative path into its repo + remainder.
fn split_repo(rel: &str) -> (String, String) {
    match rel.split_once('/') {
        Some((repo, rest)) => (repo.to_string(), rest.to_string()),
        None => (rel.to_string(), String::new()),
    }
}
