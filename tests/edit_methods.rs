use std::fs;
use tabbify_codeservice::methods::edit::rename_symbol;
use tabbify_codeservice::methods::edit::{edit, insert_after_symbol, replace_symbol_body};
use tabbify_codeservice::state::{AppState, CodeRoots};
use tabbify_workspace_contract::error::CodeErrorCode;
use tabbify_workspace_contract::rpc::{
    EditReq, InsertSymbolReq, Position, RenameSymbolReq, Replacement, ReplaceSymbolBodyReq,
};

fn fixture() -> (tempfile::TempDir, AppState, std::path::PathBuf) {
    let td = tempfile::tempdir().unwrap();
    let projects = td.path().join("projects");
    fs::create_dir_all(projects.join("demo/src")).unwrap();
    fs::write(projects.join("demo/Cargo.toml"), "[package]\nname=\"demo\"\n").unwrap();
    let f = projects.join("demo/src/lib.rs");
    fs::write(&f, "pub fn greet() {\n    println!(\"hi\");\n}\n").unwrap();
    let roots = CodeRoots { projects, knowledge: td.path().join("knowledge") };
    fs::create_dir_all(&roots.knowledge).unwrap();
    (td, AppState::new(roots, "default".into()), f)
}

#[test]
fn edit_replaces_string_and_reports_changed_file() {
    let (_td, st, f) = fixture();
    let resp = edit(&st, EditReq {
        repo: "demo".into(),
        path: "src/lib.rs".into(),
        replacements: vec![Replacement {
            old_string: Some("hi".into()),
            range: None,
            new_string: "hello".into(),
        }],
    })
    .unwrap();
    assert_eq!(resp.changed_files, vec!["src/lib.rs".to_string()]);
    assert!(fs::read_to_string(&f).unwrap().contains("hello"));
}

#[test]
fn edit_creates_missing_file_on_pure_insert() {
    // Свежий репозиторий: агент кладёт ПЕРВЫЙ файл через чистую вставку
    // (old_string/range отсутствуют). Файла нет → edit создаёт его вместе с
    // недостающими родительскими каталогами.
    let (td, st, _f) = fixture();
    let resp = edit(&st, EditReq {
        repo: "demo".into(),
        path: "public/index.html".into(),
        replacements: vec![Replacement {
            old_string: None,
            range: None,
            new_string: "<!doctype html>\n<h1>hi</h1>\n".into(),
        }],
    })
    .unwrap();
    assert_eq!(resp.changed_files, vec!["public/index.html".to_string()]);
    let written =
        fs::read_to_string(td.path().join("projects/demo/public/index.html")).unwrap();
    assert_eq!(written, "<!doctype html>\n<h1>hi</h1>\n");
}

#[test]
fn edit_creates_missing_file_when_old_string_empty() {
    // Пустой `old_string` тоже «нечего матчить» → трактуется как создание.
    let (td, st, _f) = fixture();
    edit(&st, EditReq {
        repo: "demo".into(),
        path: "Dockerfile".into(),
        replacements: vec![Replacement {
            old_string: Some(String::new()),
            range: None,
            new_string: "FROM scratch\n".into(),
        }],
    })
    .unwrap();
    let written = fs::read_to_string(td.path().join("projects/demo/Dockerfile")).unwrap();
    assert_eq!(written, "FROM scratch\n");
}

#[test]
fn edit_missing_file_with_old_string_is_not_found() {
    // Файла нет, но замена что-то ЖДЁТ найти (`old_string`) → матчить нечего,
    // остаётся `no such file` (создание запрещено — это не чистая вставка).
    let (_td, st, _f) = fixture();
    let err = edit(&st, EditReq {
        repo: "demo".into(),
        path: "missing.rs".into(),
        replacements: vec![Replacement {
            old_string: Some("foo".into()),
            range: None,
            new_string: "bar".into(),
        }],
    })
    .unwrap_err();
    assert_eq!(err.code, CodeErrorCode::NotFound);
}

#[test]
fn edit_missing_old_string_is_not_found() {
    let (_td, st, _f) = fixture();
    let err = edit(&st, EditReq {
        repo: "demo".into(),
        path: "src/lib.rs".into(),
        replacements: vec![Replacement {
            old_string: Some("ZZZ".into()),
            range: None,
            new_string: "x".into(),
        }],
    })
    .unwrap_err();
    assert_eq!(err.code, CodeErrorCode::NotFound);
}

#[test]
fn replace_symbol_body_swaps_function() {
    let (_td, st, f) = fixture();
    let resp = replace_symbol_body(&st, ReplaceSymbolBodyReq {
        repo: "demo".into(),
        symbol: "greet".into(),
        new_body: "pub fn greet() {\n    println!(\"new\");\n}".into(),
    })
    .unwrap();
    assert_eq!(resp.changed_files, vec!["src/lib.rs".to_string()]);
    assert!(fs::read_to_string(&f).unwrap().contains("new"));
}

#[test]
fn insert_after_symbol_appends_content() {
    let (_td, st, f) = fixture();
    insert_after_symbol(&st, InsertSymbolReq {
        repo: "demo".into(),
        symbol: "greet".into(),
        content: "\npub fn bye() {}\n".into(),
    })
    .unwrap();
    assert!(fs::read_to_string(&f).unwrap().contains("bye"));
}

#[test]
fn edit_traversal_repo_is_forbidden() {
    // The single-file edit path confines `repo` via paths::resolve/safe_segment.
    let (_td, st, _f) = fixture();
    let err = edit(&st, EditReq {
        repo: "../../etc".into(),
        path: "passwd".into(),
        replacements: vec![Replacement {
            old_string: Some("root".into()),
            range: None,
            new_string: "x".into(),
        }],
    })
    .unwrap_err();
    assert_eq!(err.code, CodeErrorCode::Forbidden);
}

#[test]
fn symbol_write_traversal_repo_is_forbidden() {
    // The symbol-WRITE path (locate_symbol → repo_root) must reject a traversal
    // `repo` BEFORE any fs walk. This is the gap the review flagged: previously
    // these paths built `projects.join(req.repo)` with no confinement.
    let (_td, st, _f) = fixture();
    let err = replace_symbol_body(&st, ReplaceSymbolBodyReq {
        repo: "../../etc".into(),
        symbol: "greet".into(),
        new_body: "x".into(),
    })
    .unwrap_err();
    assert_eq!(err.code, CodeErrorCode::Forbidden);

    let err2 = insert_after_symbol(&st, InsertSymbolReq {
        repo: "../../etc".into(),
        symbol: "greet".into(),
        content: "x".into(),
    })
    .unwrap_err();
    assert_eq!(err2.code, CodeErrorCode::Forbidden);
}

#[test]
fn rename_symbol_traversal_repo_is_forbidden() {
    let (_td, st, _f) = fixture();
    let err = rename_symbol(&st, RenameSymbolReq {
        repo: "../../etc".into(),
        path: "passwd".into(),
        position: Position { line: 0, character: 0 },
        new_name: "x".into(),
    })
    .unwrap_err();
    assert_eq!(err.code, CodeErrorCode::Forbidden);
}
