//! HTTP router: one `POST /v1/code/<wire>` route per implemented method, plus
//! `GET /v1/healthz`. Handlers deserialize the per-method `Req`, call the method
//! impl, and wrap the `Result<Resp, CodeError>` in a `CodeResponse<Resp>` —
//! ALWAYS HTTP 200; failures ride in the envelope (spec §6 Seam 1).

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use tabbify_workspace_contract::rpc::*;

use crate::methods::{read, search, symbols};
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
        .route("/v1/code/get_symbols_overview", post(h_get_symbols_overview))
        .route("/v1/code/find_symbol", post(h_find_symbol))
        .route("/v1/code/find_references", post(h_find_references))
        .route("/v1/code/read_symbol", post(h_read_symbol))
        .route("/v1/code/edit", post(h_edit))
        .route("/v1/code/replace_symbol_body", post(h_replace_symbol_body))
        .route("/v1/code/insert_before_symbol", post(h_insert_before_symbol))
        .route("/v1/code/insert_after_symbol", post(h_insert_after_symbol))
        .route("/v1/code/rename_symbol", post(h_rename_symbol))
        .route("/v1/code/commit", post(h_commit))
        .route("/v1/code/push", post(h_push))
        .route("/v1/code/forge_create_repo", post(h_forge_create_repo))
        .route("/v1/code/forge_list_repos", post(h_forge_list_repos))
        .route("/v1/code/forge_open_pr", post(h_forge_open_pr))
        .route("/v1/code/forge_file_url", post(h_forge_file_url))
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

async fn h_get_symbols_overview(
    State(st): State<AppState>,
    Json(req): Json<GetSymbolsOverviewReq>,
) -> Json<CodeResponse<GetSymbolsOverviewResp>> {
    envelope(symbols::get_symbols_overview(&st, req))
}

async fn h_find_symbol(
    State(st): State<AppState>,
    Json(req): Json<FindSymbolReq>,
) -> Json<CodeResponse<FindSymbolResp>> {
    envelope(symbols::find_symbol(&st, req))
}

async fn h_find_references(
    State(st): State<AppState>,
    Json(req): Json<FindReferencesReq>,
) -> Json<CodeResponse<FindReferencesResp>> {
    envelope(symbols::find_references(&st, req))
}

async fn h_read_symbol(
    State(st): State<AppState>,
    Json(req): Json<ReadSymbolReq>,
) -> Json<CodeResponse<ReadSymbolResp>> {
    envelope(symbols::read_symbol(&st, req))
}

async fn h_edit(
    State(st): State<AppState>,
    Json(req): Json<EditReq>,
) -> Json<CodeResponse<EditResp>> {
    envelope(crate::methods::edit::edit(&st, req))
}

async fn h_replace_symbol_body(
    State(st): State<AppState>,
    Json(req): Json<ReplaceSymbolBodyReq>,
) -> Json<CodeResponse<EditResp>> {
    envelope(crate::methods::edit::replace_symbol_body(&st, req))
}

async fn h_insert_before_symbol(
    State(st): State<AppState>,
    Json(req): Json<InsertSymbolReq>,
) -> Json<CodeResponse<EditResp>> {
    envelope(crate::methods::edit::insert_before_symbol(&st, req))
}

async fn h_insert_after_symbol(
    State(st): State<AppState>,
    Json(req): Json<InsertSymbolReq>,
) -> Json<CodeResponse<EditResp>> {
    envelope(crate::methods::edit::insert_after_symbol(&st, req))
}

async fn h_rename_symbol(
    State(st): State<AppState>,
    Json(req): Json<RenameSymbolReq>,
) -> Json<CodeResponse<EditResp>> {
    envelope(crate::methods::edit::rename_symbol(&st, req))
}

async fn h_commit(
    State(st): State<AppState>,
    Json(req): Json<CommitReq>,
) -> Json<CodeResponse<CommitResp>> {
    envelope(crate::methods::git::commit(&st, req))
}

async fn h_push(
    State(st): State<AppState>,
    Json(req): Json<PushReq>,
) -> Json<CodeResponse<PushResp>> {
    envelope(crate::methods::git::push(&st, req))
}

async fn h_forge_create_repo(
    State(st): State<AppState>,
    Json(req): Json<ForgeCreateRepoReq>,
) -> Json<CodeResponse<ForgeRepoInfo>> {
    envelope(crate::methods::git::forge_create_repo(&st, req))
}

async fn h_forge_list_repos(
    State(st): State<AppState>,
    Json(req): Json<ForgeListReposReq>,
) -> Json<CodeResponse<ForgeListReposResp>> {
    envelope(crate::methods::git::forge_list_repos(&st, req))
}

async fn h_forge_open_pr(
    State(st): State<AppState>,
    Json(req): Json<ForgeOpenPrReq>,
) -> Json<CodeResponse<ForgePrResp>> {
    envelope(crate::methods::git::forge_open_pr(&st, req))
}

async fn h_forge_file_url(
    State(st): State<AppState>,
    Json(req): Json<ForgeFileUrlReq>,
) -> Json<CodeResponse<ForgeUrlResp>> {
    envelope(crate::methods::git::forge_file_url(&st, req))
}
