//! Code-service binary: bind axum on `0.0.0.0:CODE_SERVICE_PORT` (8731) and
//! serve `CodeServiceRpc`. User identity comes from `CODESERVICE_USER_ID`
//! (default `"default"` — single-tenant in v1). The LSP supervisor is wired in
//! Task 8; v1 serves the LSP-less READ spine.
use tabbify_codeservice::router::build_router;
use tabbify_codeservice::state::{AppState, CodeRoots};
use tabbify_workspace_contract::CODE_SERVICE_PORT;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let user_id = std::env::var("CODESERVICE_USER_ID").unwrap_or_else(|_| "default".to_owned());
    let state = AppState::new(CodeRoots::from_contract(), user_id);
    let router = build_router(state);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], CODE_SERVICE_PORT));
    tracing::info!(%addr, "tabbify-codeservice listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}
