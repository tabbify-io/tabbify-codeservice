//! HTTP-level test: drive the router in-process via `tower::ServiceExt::oneshot`
//! and assert the wire envelope shape for a READ method.
use std::fs;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tabbify_codeservice::router::build_router;
use tabbify_codeservice::state::{AppState, CodeRoots};
use tower::ServiceExt;

fn app() -> (tempfile::TempDir, axum::Router) {
    let td = tempfile::tempdir().unwrap();
    let projects = td.path().join("projects");
    fs::create_dir_all(projects.join("demo")).unwrap();
    fs::write(projects.join("demo/Cargo.toml"), "[package]\nname=\"demo\"\n").unwrap();
    let roots = CodeRoots { projects, knowledge: td.path().join("knowledge") };
    fs::create_dir_all(&roots.knowledge).unwrap();
    let state = AppState::new(roots, "default".into());
    (td, build_router(state))
}

#[tokio::test]
async fn workspace_status_returns_ok_envelope() {
    let (_td, router) = app();
    let resp = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/code/workspace_status")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["result"]["repos"][0]["repo"], "demo");
}

#[tokio::test]
async fn read_escape_returns_forbidden_envelope() {
    let (_td, router) = app();
    let body = r#"{"repo":"demo","path":"../../etc/passwd"}"#;
    let resp = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/code/read")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    // Transport stays 200; the failure rides in the envelope.
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "forbidden");
}
