//! tree-sitter extracts top-level rust symbols WITHOUT an LSP — the fallback
//! that answers `find_symbol`/`get_symbols_overview` while the index is building.
use tabbify_codeservice::treesitter::rust_symbols;

#[test]
fn extracts_fn_and_struct() {
    let src = "pub fn greet() {}\nstruct Cfg { a: u32 }\nimpl Cfg { fn new() -> Self { Cfg{a:0} } }\n";
    let syms = rust_symbols(src, "demo", "src/lib.rs");
    let names: Vec<_> = syms.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"greet"));
    assert!(names.contains(&"Cfg"));
    // The impl method is a child of the impl block (name_path carries it).
    assert!(syms.iter().any(|s| s.name_path == "Cfg/new" || s.name == "new"));
}

#[test]
fn carries_location_repo_and_path() {
    let src = "fn only() {}\n";
    let syms = rust_symbols(src, "myrepo", "a/b.rs");
    assert_eq!(syms[0].location.repo, "myrepo");
    assert_eq!(syms[0].location.path, "a/b.rs");
}
