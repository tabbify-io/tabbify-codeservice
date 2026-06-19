//! HTTP router: one `POST /v1/code/<wire>` route per implemented method, plus
//! `GET /v1/healthz`. Handlers deserialize the per-method `Req`, call the method
//! impl, and wrap the `Result<Resp, CodeError>` in a `CodeResponse<Resp>` —
//! ALWAYS HTTP 200; failures ride in the envelope (spec §6 Seam 1).

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use tabbify_workspace_contract::rpc::*;

use crate::methods::{read, search};
use crate::state::AppState;

/// Wrap any method result into the wire envelope as an axum JSON response.
fn envelope<T: Serialize>(
    r: Result<T, tabbify_workspace_contract::error::CodeError>,
) -> Json<CodeResponse<T>> {
    Json(match r {
        Ok(v) => CodeResponse::ok(v),
        Err(e) => CodeResponse::err(e),
    })
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/healthz", get(|| async { "ok" }))
        .route("/v1/code/workspace_status", post(h_workspace_status))
        .route("/v1/code/read", post(h_read))
        .route("/v1/code/list_dir", post(h_list_dir))
        .route("/v1/code/kb_list", post(h_kb_list))
        .route("/v1/code/code_search", post(h_code_search))
        .route("/v1/code/find_file", post(h_find_file))
        .with_state(state)
}

async fn h_workspace_status(
    State(st): State<AppState>,
    Json(req): Json<WorkspaceStatusReq>,
) -> Json<CodeResponse<WorkspaceStatusResp>> {
    envelope(Ok(read::workspace_status(&st, req)))
}

async fn h_read(
    State(st): State<AppState>,
    Json(req): Json<ReadReq>,
) -> Json<CodeResponse<ReadResp>> {
    envelope(read::read_file(&st, req))
}

async fn h_list_dir(
    State(st): State<AppState>,
    Json(req): Json<ListDirReq>,
) -> Json<CodeResponse<ListDirResp>> {
    envelope(read::list_dir(&st, req))
}

async fn h_kb_list(
    State(st): State<AppState>,
    Json(req): Json<KbListReq>,
) -> Json<CodeResponse<KbListResp>> {
    envelope(read::kb_list(&st, req))
}

async fn h_code_search(
    State(st): State<AppState>,
    Json(req): Json<CodeSearchReq>,
) -> Json<CodeResponse<CodeSearchResp>> {
    envelope(search::code_search(&st, req))
}

async fn h_find_file(
    State(st): State<AppState>,
    Json(req): Json<FindFileReq>,
) -> Json<CodeResponse<FindFileResp>> {
    envelope(search::find_file(&st, req))
}
