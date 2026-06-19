//! Git + forge methods. `commit` is unprivileged (local). `push` + forge ops
//! delegate to the broker, which holds the credential and returns only results
//! (a `needs_credential` error is relayed verbatim to the agent).

use std::process::Command;

use tabbify_workspace_contract::error::{CodeError, CodeErrorCode};
use tabbify_workspace_contract::rpc::{
    CommitReq, CommitResp, ForgeCreateRepoReq, ForgeFileUrlReq, ForgeListReposReq,
    ForgeListReposResp, ForgeOpenPrReq, ForgePrResp, ForgeProvisionReq, ForgeRepoInfo,
    ForgeUrlResp, GitOp, GitOpReq, GitOpResult, PushReq, PushResp,
};

use crate::broker_client::{self, BrokerRequest};
use crate::paths::repo_root;
use crate::state::AppState;

/// `commit{repo,message,paths?}` — stage + commit locally (no credential). The
/// repo name is confined through the SHARED `paths::repo_root`/`safe_segment`
/// (one confinement implementation across the whole crate — no ad-hoc check).
pub fn commit(state: &AppState, req: CommitReq) -> Result<CommitResp, CodeError> {
    let work = repo_root(&state.roots.projects, &req.repo)?;
    if !work.join(".git").exists() {
        return Err(CodeError::new(CodeErrorCode::NotFound, "not a git repo"));
    }
    let mut add = Command::new("git");
    add.current_dir(&work).arg("add");
    match &req.paths {
        Some(paths) if !paths.is_empty() => {
            add.arg("--");
            for p in paths {
                if p.contains("..") {
                    return Err(CodeError::new(CodeErrorCode::Forbidden, "path escapes repo"));
                }
                add.arg(p);
            }
        }
        _ => {
            add.arg("-A");
        }
    }
    run_ok(&mut add, "git add")?;

    let mut ci = Command::new("git");
    ci.current_dir(&work).args(["commit", "-m", &req.message]);
    run_ok(&mut ci, "git commit")?;

    let sha = Command::new("git")
        .current_dir(&work)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| CodeError::new(CodeErrorCode::Internal, format!("rev-parse: {e}")))?;
    Ok(CommitResp {
        commit_sha: String::from_utf8_lossy(&sha.stdout).trim().to_string(),
    })
}

fn run_ok(cmd: &mut Command, what: &str) -> Result<(), CodeError> {
    let out = cmd
        .output()
        .map_err(|e| CodeError::new(CodeErrorCode::Internal, format!("{what} spawn: {e}")))?;
    if !out.status.success() {
        return Err(CodeError::new(
            CodeErrorCode::Internal,
            format!("{what} failed: {}", String::from_utf8_lossy(&out.stderr)),
        ));
    }
    Ok(())
}

/// `push{repo,branch?}` — delegate to the broker. A `needs_credential` from the
/// broker propagates verbatim (the agent relays it to the human). The request is
/// the TYPED `BrokerRequest::GitOp(GitOpReq{..})` — NOT hand-built JSON — so a
/// future rename of the `ref`/`op` wire names is a compile error on this path
/// (the seam stays compile-checked; review contract_mismatch fixed).
pub fn push(state: &AppState, req: PushReq) -> Result<PushResp, CodeError> {
    // Confine the repo through the SHARED gate (rejects traversal) before we ask
    // the broker to operate on `<projects>/<repo>`.
    repo_root(&state.roots.projects, &req.repo)?;
    let request = BrokerRequest::GitOp(GitOpReq {
        repo: req.repo,
        op: GitOp::Push,
        git_ref: req.branch.clone(),
    });
    let res: GitOpResult = broker_client::call(&request)?;
    Ok(PushResp {
        pushed: true,
        remote_ref: req
            .branch
            .map(|b| format!("origin/{b}"))
            .or_else(|| res.commit_sha.map(|s| s[..s.len().min(12)].to_string())),
    })
}

/// `forge_create_repo{name,private?,description?}` → broker `forge_provision`.
/// Typed `BrokerRequest::ForgeProvision(ForgeProvisionReq{..})` (compile-checked).
pub fn forge_create_repo(
    _state: &AppState,
    req: ForgeCreateRepoReq,
) -> Result<ForgeRepoInfo, CodeError> {
    let request = BrokerRequest::ForgeProvision(ForgeProvisionReq {
        name: req.name,
        private: req.private,
        description: req.description,
    });
    broker_client::call(&request)
}

/// `forge_list_repos{}` → broker `forge_list_repos`. The broker owns the
/// Forgejo listing (T5 adds `ForgeList` to the shared `BrokerRequest` enum). The
/// codeservice forwards the typed request and returns whatever the broker
/// reports — it NEVER fabricates an empty list (that would tell the agent "you
/// have no repos" even when the user has them). Until T5 lands the broker arm,
/// the broker replies `internal "forge_list not implemented"` and that error
/// surfaces here verbatim — an honest not-implemented, not fake success.
pub fn forge_list_repos(
    _state: &AppState,
    _req: ForgeListReposReq,
) -> Result<ForgeListReposResp, CodeError> {
    broker_client::call(&BrokerRequest::ForgeList)
}

/// `forge_open_pr{...}` → broker `forge_open_pr` (T5 adds the broker arm). Typed
/// forward; the broker's reply (success or not-implemented) surfaces verbatim.
pub fn forge_open_pr(_state: &AppState, req: ForgeOpenPrReq) -> Result<ForgePrResp, CodeError> {
    broker_client::call(&BrokerRequest::ForgeOpenPr(req))
}

/// `forge_file_url{repo,path,ref?}` → broker `forge_file_url` (T5 adds the arm).
pub fn forge_file_url(_state: &AppState, req: ForgeFileUrlReq) -> Result<ForgeUrlResp, CodeError> {
    broker_client::call(&BrokerRequest::ForgeFileUrl(req))
}
