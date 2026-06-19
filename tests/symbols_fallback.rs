//! With no LSP ready, `get_symbols_overview` + `find_symbol` answer from
//! tree-sitter. We force the fallback path by leaving `ready=false` (the default
//! in a freshly built AppState).
use std::fs;
use tabbify_codeservice::methods::symbols::{find_references, find_symbol, get_symbols_overview};
use tabbify_codeservice::state::{AppState, CodeRoots};
use tabbify_workspace_contract::error::CodeErrorCode;
use tabbify_workspace_contract::rpc::{
    FindReferencesReq, FindSymbolReq, GetSymbolsOverviewReq, Position,
};

fn fixture() -> (tempfile::TempDir, AppState) {
    let td = tempfile::tempdir().unwrap();
    let projects = td.path().join("projects");
    fs::create_dir_all(projects.join("demo/src")).unwrap();
    fs::write(projects.join("demo/Cargo.toml"), "[package]\nname=\"demo\"\n").unwrap();
    fs::write(
        projects.join("demo/src/lib.rs"),
        "pub fn greet() {}\nstruct Cfg { a: u32 }\n",
    )
    .unwrap();
    let roots = CodeRoots { projects, knowledge: td.path().join("knowledge") };
    fs::create_dir_all(&roots.knowledge).unwrap();
    (td, AppState::new(roots, "default".into()))
}

#[test]
fn overview_lists_symbols_via_treesitter() {
    let (_td, st) = fixture();
    let resp = get_symbols_overview(
        &st,
        GetSymbolsOverviewReq { repo: "demo".into(), path: "src/lib.rs".into() },
    )
    .unwrap();
    let names: Vec<_> = resp.symbols.iter().map(|s| s.name.clone()).collect();
    assert!(names.contains(&"greet".to_string()));
    assert!(names.contains(&"Cfg".to_string()));
    // overview omits bodies (token-efficient).
    assert!(resp.symbols.iter().all(|s| s.body.is_none()));
}

#[test]
fn find_symbol_matches_by_name_path() {
    let (_td, st) = fixture();
    let resp = find_symbol(
        &st,
        FindSymbolReq { name_path: "greet".into(), kind: None, include_body: Some(true) },
    )
    .unwrap();
    assert_eq!(resp.symbols.len(), 1);
    assert_eq!(resp.symbols[0].name, "greet");
    assert!(resp.symbols[0].body.as_deref().unwrap().contains("greet"));
}

#[test]
fn find_references_errors_honestly_when_lsp_not_ready() {
    // find_references issues a REAL LSP request only once the index is Ready.
    // On a fresh box (no rust-analyzer, or index not done) it MUST return a
    // typed error — never a fabricated empty set that reads as "no references".
    let (_td, st) = fixture();
    let err = find_references(
        &st,
        FindReferencesReq {
            repo: "demo".into(),
            path: "src/lib.rs".into(),
            position: Position { line: 0, character: 7 },
        },
    )
    .unwrap_err();
    // Either lsp-unavailable (Internal) or index-still-building (Invalid) —
    // both are honest; neither is a silent empty Ok.
    assert!(matches!(err.code, CodeErrorCode::Internal | CodeErrorCode::Invalid));
}
