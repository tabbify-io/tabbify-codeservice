//! Compile-fence: the contract path-dep is wired and the envelope helpers exist.
use tabbify_workspace_contract::error::{CodeError, CodeErrorCode};
use tabbify_workspace_contract::rpc::{CodeResponse, WorkspaceStatusResp};
use tabbify_workspace_contract::{CODE_SERVICE_PORT, PROJECTS_DIR};

#[test]
fn contract_types_link_and_envelope_builds() {
    assert_eq!(CODE_SERVICE_PORT, 8731);
    assert_eq!(PROJECTS_DIR, "/home/agent/projects");

    let ok = CodeResponse::ok(WorkspaceStatusResp { repos: vec![] });
    let j = serde_json::to_string(&ok).unwrap();
    assert_eq!(j, r#"{"ok":true,"result":{"repos":[]}}"#);

    let err: CodeResponse<WorkspaceStatusResp> =
        CodeResponse::err(CodeError::new(CodeErrorCode::NotFound, "x"));
    let je = serde_json::to_string(&err).unwrap();
    assert!(je.contains(r#""ok":false"#) && je.contains(r#""code":"not_found""#));
}
