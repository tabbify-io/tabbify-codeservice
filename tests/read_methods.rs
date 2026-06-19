//! READ-method integration over a temp fixture. We override the projects root
//! via `CodeRoots` so the tests do not touch the real `/home/agent`.
use std::fs;
use tabbify_codeservice::repo::discover_repos;
use tabbify_codeservice::state::CodeRoots;

fn fixture() -> (tempfile::TempDir, CodeRoots) {
    let td = tempfile::tempdir().unwrap();
    let projects = td.path().join("projects");
    let knowledge = td.path().join("knowledge");
    fs::create_dir_all(projects.join("demo/src")).unwrap();
    fs::write(projects.join("demo/Cargo.toml"), "[package]\nname=\"demo\"\n").unwrap();
    fs::write(projects.join("demo/src/lib.rs"), "pub fn hi() {}\n").unwrap();
    fs::create_dir_all(&knowledge).unwrap();
    fs::write(knowledge.join("notes.md"), "# notes\n").unwrap();
    let roots = CodeRoots { projects: projects.clone(), knowledge };
    (td, roots)
}

#[test]
fn discover_finds_demo_repo_and_rust_lang() {
    let (_td, roots) = fixture();
    let repos = discover_repos(&roots.projects);
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].name, "demo");
    assert!(repos[0].languages.iter().any(|l| l == "rust"));
}
