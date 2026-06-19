//! Broker-delegating methods (`push`, `forge_*`) over a REAL spawned broker on a
//! temp unix socket. The broker is the actual `tabbify_broker::server::serve`
//! (same wire it speaks in prod) — NOT a fake. We point the codeservice's
//! broker client at the temp socket via `TABBIFY_BROKER_SOCKET` (the configurable
//! path the run scope requires for tests; prod uses the contract constant).
//!
//! This proves: (1) `push` round-trips the TYPED `BrokerRequest::GitOp` and
//! advances a local bare remote; (2) the T5-owned forge arms surface the broker's
//! HONEST not-implemented error verbatim — never a fabricated success.

use std::process::Command;
use std::sync::Mutex;

use tabbify_broker::creds::Creds;
use tabbify_codeservice::methods::git::{forge_list_repos, push};
use tabbify_codeservice::state::{AppState, CodeRoots};
use tabbify_workspace_contract::error::CodeErrorCode;
use tabbify_workspace_contract::rpc::{ForgeListReposReq, PushReq};

/// Serialize the env-var mutation across tests (env is process-global).
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn sh(dir: &std::path::Path, args: &[&str]) {
    assert!(
        Command::new(args[0])
            .args(&args[1..])
            .current_dir(dir)
            .status()
            .unwrap()
            .success(),
        "command failed: {args:?}"
    );
}

/// Build a projects dir with `demo` as a git working copy whose `origin` is a
/// local bare repo, and a Creds with NO cap-URL (so the broker uses `origin`).
fn git_fixture(td: &std::path::Path) -> std::path::PathBuf {
    let bare = td.join("remote.git");
    let projects = td.join("projects");
    let work = projects.join("demo");
    std::fs::create_dir_all(&work).unwrap();
    sh(td, &["git", "init", "--bare", bare.to_str().unwrap()]);
    sh(&work, &["git", "init"]);
    sh(&work, &["git", "config", "user.email", "a@b.c"]);
    sh(&work, &["git", "config", "user.name", "t"]);
    std::fs::write(work.join("f.txt"), "hi").unwrap();
    sh(&work, &["git", "add", "."]);
    sh(&work, &["git", "commit", "-m", "init"]);
    sh(&work, &["git", "remote", "add", "origin", bare.to_str().unwrap()]);
    projects
}

/// The current branch name of `<projects>/demo` (git's default-branch name
/// varies: `master` on old git, `main`/configured on new). We push THIS branch
/// rather than assuming a name.
fn current_branch(projects: &std::path::Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(projects.join("demo"))
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Spawn the real broker on `sock`, serving `projects` with `creds`, and wait
/// for the socket to appear.
fn spawn_broker(
    rt: &tokio::runtime::Runtime,
    sock: std::path::PathBuf,
    creds: Creds,
    projects: std::path::PathBuf,
) {
    let sock2 = sock.clone();
    rt.spawn(async move {
        let _ = tabbify_broker::server::serve(&sock2, creds, projects).await;
    });
    for _ in 0..100 {
        if sock.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("broker socket never appeared");
}

#[test]
fn push_round_trips_through_real_broker_and_advances_remote() {
    let _guard = ENV_LOCK.lock().unwrap();
    let td = tempfile::tempdir().unwrap();
    let projects = git_fixture(td.path());
    let branch = current_branch(&projects);
    let sock = td.path().join("broker.sock");

    let rt = tokio::runtime::Runtime::new().unwrap();
    spawn_broker(&rt, sock.clone(), Creds::default(), projects.clone());

    // SAFETY: env mutation is serialized by ENV_LOCK; single-threaded test body.
    unsafe {
        std::env::set_var("TABBIFY_BROKER_SOCKET", &sock);
    }
    let st = AppState::new(
        CodeRoots { projects, knowledge: td.path().join("knowledge") },
        "default".into(),
    );
    let result = push(&st, PushReq {
        repo: "demo".into(),
        branch: Some(branch),
    });
    // Drop the override BEFORE asserting, so a failed assert never leaks env.
    unsafe {
        std::env::remove_var("TABBIFY_BROKER_SOCKET");
    }
    let resp = result.unwrap();
    assert!(resp.pushed);
}

#[test]
fn push_traversal_repo_is_forbidden_before_broker() {
    let _guard = ENV_LOCK.lock().unwrap();
    let td = tempfile::tempdir().unwrap();
    let st = AppState::new(
        CodeRoots {
            projects: td.path().join("projects"),
            knowledge: td.path().join("knowledge"),
        },
        "default".into(),
    );
    // No broker needed: the confinement gate rejects before any socket call.
    let err = push(&st, PushReq {
        repo: "../../etc".into(),
        branch: None,
    })
    .unwrap_err();
    assert_eq!(err.code, CodeErrorCode::Forbidden);
}

#[test]
fn forge_list_surfaces_brokers_honest_not_implemented() {
    let _guard = ENV_LOCK.lock().unwrap();
    let td = tempfile::tempdir().unwrap();
    let projects = td.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let sock = td.path().join("broker.sock");

    let rt = tokio::runtime::Runtime::new().unwrap();
    spawn_broker(&rt, sock.clone(), Creds::default(), projects.clone());

    unsafe {
        std::env::set_var("TABBIFY_BROKER_SOCKET", &sock);
    }
    let st = AppState::new(
        CodeRoots { projects, knowledge: td.path().join("knowledge") },
        "default".into(),
    );
    // The broker's ForgeList arm is the T5-owned honest not-implemented: it
    // returns an `internal` error. The codeservice MUST surface it verbatim,
    // NOT fabricate an empty repo list.
    let result = forge_list_repos(&st, ForgeListReposReq {});
    unsafe {
        std::env::remove_var("TABBIFY_BROKER_SOCKET");
    }
    let err = result.unwrap_err();
    assert_eq!(err.code, CodeErrorCode::Internal);
    assert!(err.message.contains("not implemented"));
}
