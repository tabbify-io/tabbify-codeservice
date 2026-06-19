//! Symbol navigation: get_symbols_overview, find_symbol, find_references,
//! read_symbol. `get_symbols_overview`/`find_symbol`/`read_symbol` answer from
//! tree-sitter (exact for top-level + impl-method symbols; the LSP
//! `documentSymbol`/`workspace/symbol` upgrade is Phase-2 wiring on the same
//! `LspClient::request()` primitive). `find_references` issues a REAL
//! `textDocument/references` request to the warm rust-analyzer (the v1 accept
//! proof) and returns a typed error — NOT a fake empty set — when the LSP is
//! unavailable or the index is not yet `Ready`.

use tabbify_workspace_contract::error::{CodeError, CodeErrorCode};
use tabbify_workspace_contract::rpc::{
    FindReferencesReq, FindReferencesResp, FindSymbolReq, FindSymbolResp, GetSymbolsOverviewReq,
    GetSymbolsOverviewResp, ReadSymbolReq, ReadSymbolResp, Symbol,
};

use crate::paths::{resolve, Root};
use crate::state::AppState;
use crate::treesitter::rust_symbols;

/// Re-base a confined contract path onto the (possibly test) projects root.
fn rebase(state: &AppState, abs: std::path::PathBuf) -> std::path::PathBuf {
    state.roots.projects.join(
        abs.strip_prefix(tabbify_workspace_contract::PROJECTS_DIR)
            .unwrap_or(&abs),
    )
}

/// `get_symbols_overview{repo,path}` → structural map, bodies omitted.
pub fn get_symbols_overview(
    state: &AppState,
    req: GetSymbolsOverviewReq,
) -> Result<GetSymbolsOverviewResp, CodeError> {
    let abs = rebase(state, resolve(Root::Projects, &req.repo, &req.path)?);
    let src = std::fs::read_to_string(&abs)
        .map_err(|_| CodeError::new(CodeErrorCode::NotFound, "no such file"))?;
    // MVP: tree-sitter is the overview backend (LSP documentSymbol is an
    // additive upgrade in Phase 2; the wire shape is identical).
    let symbols = rust_symbols(&src, &req.repo, &req.path);
    Ok(GetSymbolsOverviewResp { symbols })
}

/// `find_symbol{name_path,kind?,include_body?}` → matching symbols across the
/// repo's rust files (tree-sitter walk; LSP `workspace/symbol` in Phase 2).
pub fn find_symbol(state: &AppState, req: FindSymbolReq) -> Result<FindSymbolResp, CodeError> {
    if req.name_path.is_empty() {
        return Err(CodeError::new(CodeErrorCode::Invalid, "empty name_path"));
    }
    let want = req.name_path.rsplit('/').next().unwrap_or(&req.name_path);
    let include_body = req.include_body.unwrap_or(false);
    let mut hits: Vec<Symbol> = Vec::new();
    for repo in crate::repo::discover_repos(&state.roots.projects) {
        let repo_root = state.roots.projects.join(&repo.name);
        for entry in walkdir::WalkDir::new(&repo_root).into_iter().flatten() {
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
                .strip_prefix(&repo_root)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            for mut sym in rust_symbols(&src, &repo.name, &rel) {
                let name_ok = sym.name == want || sym.name_path == req.name_path;
                let kind_ok = req.kind.as_ref().map(|k| &sym.kind == k).unwrap_or(true);
                if name_ok && kind_ok {
                    if include_body {
                        sym.body = Some(body_of(&src, &sym));
                    }
                    hits.push(sym);
                }
            }
        }
    }
    Ok(FindSymbolResp { symbols: hits })
}

/// `read_symbol{repo,path,position}` → the symbol at `position`, body included.
pub fn read_symbol(state: &AppState, req: ReadSymbolReq) -> Result<ReadSymbolResp, CodeError> {
    let abs = rebase(state, resolve(Root::Projects, &req.repo, &req.path)?);
    let src = std::fs::read_to_string(&abs)
        .map_err(|_| CodeError::new(CodeErrorCode::NotFound, "no such file"))?;
    let symbols = rust_symbols(&src, &req.repo, &req.path);
    let sym = symbols
        .into_iter()
        .find(|s| {
            let r = s.location.range;
            req.position.line >= r.start.line && req.position.line <= r.end.line
        })
        .map(|mut s| {
            s.body = Some(body_of(&src, &s));
            s
        })
        .ok_or_else(|| CodeError::new(CodeErrorCode::NotFound, "no symbol at position"))?;
    Ok(ReadSymbolResp { symbol: sym })
}

/// `find_references{repo,path,position}` → a REAL `textDocument/references`
/// request to rust-analyzer (the v1 acceptance proof). We `did_open` the file,
/// issue the request through `LspClient::request()` (id-correlated), and map the
/// LSP `Location[]` reply back to contract `Location`s.
///
/// Taxonomy (NOT a fabricated empty set):
/// - LSP unavailable (no rust-analyzer on PATH) → `Internal "lsp unavailable"`.
/// - Index not yet `Ready` → `Invalid "index still building; retry"` so the
///   agent knows to poll `workspace_status` and retry, rather than mistake a
///   premature empty answer for "no references" (would defeat the proof).
/// - Ready → the EXACT reference set (possibly empty == genuinely no refs).
pub fn find_references(
    state: &AppState,
    req: FindReferencesReq,
) -> Result<FindReferencesResp, CodeError> {
    let abs = rebase(state, resolve(Root::Projects, &req.repo, &req.path)?);
    let src = std::fs::read_to_string(&abs)
        .map_err(|_| CodeError::new(CodeErrorCode::NotFound, "no such file"))?;
    let root = crate::paths::repo_root(&state.roots.projects, &req.repo)?;
    let client = state
        .lsp
        .client(&req.repo, &root)
        .ok_or_else(|| CodeError::new(CodeErrorCode::Internal, "lsp unavailable"))?;
    if !client.is_ready() {
        return Err(CodeError::new(
            CodeErrorCode::Invalid,
            "index still building; poll workspace_status then retry",
        ));
    }
    let file_uri = format!("file://{}", abs.display());
    client.did_open(&file_uri, &src)?;
    let result = client.request(
        "textDocument/references",
        serde_json::json!({
            "textDocument": {"uri": file_uri},
            "position": {"line": req.position.line, "character": req.position.character},
            "context": {"includeDeclaration": true}
        }),
        std::time::Duration::from_secs(15),
    )?;
    let references = parse_lsp_locations(&result, &req.repo, &root);
    Ok(FindReferencesResp { references })
}

/// Map an LSP `Location[]` JSON reply to contract `Location`s, re-relativising
/// each `uri` back to `<repo>/<workspace-relative-path>`. Out-of-repo locations
/// (e.g. std) are dropped (the agent navigates within its own workspace).
fn parse_lsp_locations(
    result: &serde_json::Value,
    repo: &str,
    repo_root: &std::path::Path,
) -> Vec<tabbify_workspace_contract::rpc::Location> {
    use tabbify_workspace_contract::rpc::{Location, Position, Range};
    let Some(arr) = result.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for loc in arr {
        let uri = loc["uri"].as_str().unwrap_or_default();
        let fs_path = uri.strip_prefix("file://").unwrap_or(uri);
        let Ok(rel) = std::path::Path::new(fs_path).strip_prefix(repo_root) else {
            continue; // outside the repo (std/deps) — skip
        };
        let r = &loc["range"];
        out.push(Location {
            repo: repo.to_string(),
            path: rel.to_string_lossy().into_owned(),
            range: Range {
                start: Position {
                    line: r["start"]["line"].as_u64().unwrap_or(0) as u32,
                    character: r["start"]["character"].as_u64().unwrap_or(0) as u32,
                },
                end: Position {
                    line: r["end"]["line"].as_u64().unwrap_or(0) as u32,
                    character: r["end"]["character"].as_u64().unwrap_or(0) as u32,
                },
            },
        });
    }
    out
}

/// Extract a symbol's source body by its tree-sitter line range.
fn body_of(src: &str, sym: &Symbol) -> String {
    let start = sym.location.range.start.line as usize;
    let end = sym.location.range.end.line as usize;
    src.lines()
        .enumerate()
        .filter(|(i, _)| *i >= start && *i <= end)
        .map(|(_, l)| l)
        .collect::<Vec<_>>()
        .join("\n")
}
