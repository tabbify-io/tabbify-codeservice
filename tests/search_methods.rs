//! `code_search` (ripgrep) + `find_file` (fd) over the fixture. These shell out
//! to `rg`/`fd`; the tests skip (pass) gracefully if the binary is absent on the
//! dev box — the image bakes both, and the Linux-gate covers the build.
use std::fs;
use std::path::Path;
use tabbify_codeservice::methods::search::{code_search, find_file};
use tabbify_codeservice::state::{AppState, CodeRoots};
use tabbify_workspace_contract::rpc::{CodeSearchReq, FindFileReq};

fn fixture() -> (tempfile::TempDir, AppState) {
    let td = tempfile::tempdir().unwrap();
    let projects = td.path().join("projects");
    fs::create_dir_all(projects.join("demo/src")).unwrap();
    fs::write(projects.join("demo/src/lib.rs"), "pub fn greet() { todo!() }\n").unwrap();
    let roots = CodeRoots { projects, knowledge: td.path().join("knowledge") };
    (td, AppState::new(roots, "default".into()))
}

fn have(bin: &str) -> bool { which::which(bin).is_ok() }

#[test]
fn code_search_finds_greet() {
    if !have("rg") { eprintln!("skip: rg absent"); return; }
    let (_td, st) = fixture();
    let resp = code_search(&st, CodeSearchReq {
        query: "greet".into(), glob: None, max: None,
    }).unwrap();
    assert!(resp.matches.iter().any(|m| m.repo == "demo" && m.text.contains("greet")));
    assert!(resp.matches.iter().all(|m| !Path::new(&m.path).is_absolute()));
}

#[test]
fn find_file_finds_lib_rs() {
    if !have("fd") && !have("fdfind") { eprintln!("skip: fd absent"); return; }
    let (_td, st) = fixture();
    let resp = find_file(&st, FindFileReq { pattern: "lib".into() }).unwrap();
    assert!(resp.paths.iter().any(|p| p.ends_with("lib.rs")));
}
