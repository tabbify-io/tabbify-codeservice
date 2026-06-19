//! `commit` runs unprivileged against a real git working copy (no broker, no
//! network). push/forge are exercised over a real socket in the broker's own
//! tests + the in-image acceptance test (Task 18).
use std::process::Command;
use tabbify_codeservice::methods::git::commit;
use tabbify_codeservice::state::{AppState, CodeRoots};
use tabbify_workspace_contract::error::CodeErrorCode;
use tabbify_workspace_contract::rpc::CommitReq;

fn sh(dir: &std::path::Path, args: &[&str]) {
    assert!(Command::new(args[0])
        .args(&args[1..])
        .current_dir(dir)
        .status()
        .unwrap()
        .success());
}

#[test]
fn commit_creates_a_commit_and_returns_sha() {
    let td = tempfile::tempdir().unwrap();
    let projects = td.path().join("projects");
    let repo = projects.join("demo");
    std::fs::create_dir_all(&repo).unwrap();
    sh(&repo, &["git", "init"]);
    sh(&repo, &["git", "config", "user.email", "a@b.c"]);
    sh(&repo, &["git", "config", "user.name", "t"]);
    std::fs::write(repo.join("f.txt"), "hi").unwrap();
    let st = AppState::new(
        CodeRoots { projects, knowledge: td.path().join("knowledge") },
        "default".into(),
    );
    let resp = commit(&st, CommitReq {
        repo: "demo".into(),
        message: "add f".into(),
        paths: None,
    })
    .unwrap();
    assert_eq!(resp.commit_sha.len(), 40);
}

#[test]
fn commit_traversal_repo_is_forbidden() {
    // The commit path confines `repo` through the SHARED paths::repo_root gate.
    let td = tempfile::tempdir().unwrap();
    let st = AppState::new(
        CodeRoots {
            projects: td.path().join("projects"),
            knowledge: td.path().join("knowledge"),
        },
        "default".into(),
    );
    let err = commit(&st, CommitReq {
        repo: "../../etc".into(),
        message: "x".into(),
        paths: None,
    })
    .unwrap_err();
    assert_eq!(err.code, CodeErrorCode::Forbidden);
}

#[test]
fn commit_non_git_dir_is_not_found() {
    let td = tempfile::tempdir().unwrap();
    let projects = td.path().join("projects");
    std::fs::create_dir_all(projects.join("demo")).unwrap();
    let st = AppState::new(
        CodeRoots { projects, knowledge: td.path().join("knowledge") },
        "default".into(),
    );
    let err = commit(&st, CommitReq {
        repo: "demo".into(),
        message: "x".into(),
        paths: None,
    })
    .unwrap_err();
    assert_eq!(err.code, CodeErrorCode::NotFound);
}
