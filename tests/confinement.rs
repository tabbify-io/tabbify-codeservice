use std::path::Path;
use tabbify_codeservice::paths::{repo_root, resolve, safe_segment, Root};
use tabbify_workspace_contract::error::CodeErrorCode;

#[test]
fn rejects_parent_traversal() {
    let err = resolve(Root::Projects, "myrepo", "../../etc/passwd").unwrap_err();
    assert_eq!(err.code, CodeErrorCode::Forbidden);
}

#[test]
fn repo_root_rejects_traversal_repo() {
    // The symbol-write + git paths build a repo root WITHOUT a `rel`; they MUST
    // confine the repo name through the same `safe_segment`/`repo_root` gate.
    let err = repo_root(Path::new("/home/agent/projects"), "../../etc").unwrap_err();
    assert_eq!(err.code, CodeErrorCode::Forbidden);
    assert!(safe_segment("a/b").is_err());
    assert!(safe_segment("..").is_err());
    assert_eq!(safe_segment("").unwrap_err().code, CodeErrorCode::Invalid);
    assert_eq!(safe_segment("demo").unwrap(), "demo");
}

#[test]
fn rejects_absolute_escape() {
    let err = resolve(Root::Projects, "myrepo", "/etc/passwd").unwrap_err();
    assert_eq!(err.code, CodeErrorCode::Forbidden);
}

#[test]
fn accepts_in_repo_relative_path() {
    let p = resolve(Root::Projects, "myrepo", "src/lib.rs").unwrap();
    assert!(p.ends_with("home/agent/projects/myrepo/src/lib.rs"));
}

#[test]
fn knowledge_root_resolves_under_knowledge_dir() {
    let p = resolve(Root::Knowledge, "", "notes/todo.md").unwrap();
    assert!(p.ends_with("home/agent/knowledge/notes/todo.md"));
}

#[test]
fn rejects_empty_repo_for_projects() {
    let err = resolve(Root::Projects, "", "x").unwrap_err();
    assert_eq!(err.code, CodeErrorCode::Invalid);
}
