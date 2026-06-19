//! READ-method integration over a temp fixture. We override the projects root
//! via `CodeRoots` so the tests do not touch the real `/home/agent`.
use std::fs;
use tabbify_codeservice::repo::discover_repos;
use tabbify_codeservice::state::CodeRoots;

fn fixture() -> (tempfile::TempDir, CodeRoots) {
    let td = tempfile::tempdir().unwrap();
    let projects = td.path().join("projects");
    let knowledge = td.path().join("knowledge");
    fs::create_dir_all(projects.join("demo/src")).unwrap();
    fs::write(projects.join("demo/Cargo.toml"), "[package]\nname=\"demo\"\n").unwrap();
    fs::write(projects.join("demo/src/lib.rs"), "pub fn hi() {}\n").unwrap();
    fs::create_dir_all(&knowledge).unwrap();
    fs::write(knowledge.join("notes.md"), "# notes\n").unwrap();
    let roots = CodeRoots { projects: projects.clone(), knowledge };
    (td, roots)
}

#[test]
fn discover_finds_demo_repo_and_rust_lang() {
    let (_td, roots) = fixture();
    let repos = discover_repos(&roots.projects);
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].name, "demo");
    assert!(repos[0].languages.iter().any(|l| l == "rust"));
}

use tabbify_codeservice::methods::read::{kb_list, list_dir, read_file};
use tabbify_codeservice::state::AppState;
use tabbify_workspace_contract::error::CodeErrorCode;
use tabbify_workspace_contract::rpc::{KbListReq, ListDirReq, ReadReq};

fn state(roots: CodeRoots) -> AppState {
    AppState::new(roots, "default".to_owned())
}

#[test]
fn read_returns_file_content() {
    let (_td, roots) = fixture();
    let st = state(roots);
    let resp = read_file(&st, ReadReq {
        repo: "demo".into(),
        path: "src/lib.rs".into(),
        range: None,
    }).unwrap();
    assert_eq!(resp.content, "pub fn hi() {}\n");
}

#[test]
fn read_rejects_escape_with_forbidden() {
    let (_td, roots) = fixture();
    let st = state(roots);
    let err = read_file(&st, ReadReq {
        repo: "demo".into(),
        path: "../../etc/passwd".into(),
        range: None,
    }).unwrap_err();
    assert_eq!(err.code, CodeErrorCode::Forbidden);
}

#[test]
fn read_missing_file_is_not_found() {
    let (_td, roots) = fixture();
    let st = state(roots);
    let err = read_file(&st, ReadReq {
        repo: "demo".into(),
        path: "src/nope.rs".into(),
        range: None,
    }).unwrap_err();
    assert_eq!(err.code, CodeErrorCode::NotFound);
}

#[test]
fn list_dir_lists_repo_root() {
    let (_td, roots) = fixture();
    let st = state(roots);
    let resp = list_dir(&st, ListDirReq { repo: "demo".into(), path: None }).unwrap();
    let names: Vec<_> = resp.entries.iter().map(|e| e.name.clone()).collect();
    assert!(names.contains(&"Cargo.toml".to_string()));
    assert!(names.contains(&"src".to_string()));
    assert!(resp.entries.iter().find(|e| e.name == "src").unwrap().is_dir);
}

#[test]
fn kb_list_lists_markdown() {
    let (_td, roots) = fixture();
    let st = state(roots);
    let resp = kb_list(&st, KbListReq {}).unwrap();
    assert_eq!(resp.files, vec!["notes.md".to_string()]);
}
