//! Multi-repo discovery + per-(repo,lang) index status.
//!
//! A repo is any immediate subdirectory of `PROJECTS_DIR` containing a `.git`
//! dir OR a recognised manifest. Language detection is manifest-based (MVP: one
//! LSP — rust). `index_status` starts `Indexing` and flips to `Ready` when the
//! LSP signals its first successful index (Task 8).

use std::path::Path;

use tabbify_workspace_contract::rpc::IndexStatus;

/// One discovered repo and the languages detected in it.
#[derive(Debug, Clone)]
pub struct DiscoveredRepo {
    pub name: String,
    pub languages: Vec<String>,
}

/// Live per-(repo,lang) index status, mutated as the LSP warms up.
#[derive(Debug, Clone)]
pub struct LangIndexState {
    pub lang: String,
    pub status: IndexStatus,
}

/// Scan `projects_root` for repos. Best-effort: unreadable entries are skipped.
pub fn discover_repos(projects_root: &Path) -> Vec<DiscoveredRepo> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(projects_root) else {
        return out;
    };
    for entry in rd.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let langs = detect_languages(&entry.path());
        out.push(DiscoveredRepo { name, languages: langs });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Manifest-based language detection (MVP: rust only — the dogfood toolchain).
fn detect_languages(repo: &Path) -> Vec<String> {
    let mut langs = Vec::new();
    if repo.join("Cargo.toml").is_file() {
        langs.push("rust".to_owned());
    }
    langs
}
