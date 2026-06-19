//! tree-sitter symbol extraction (rust, MVP). Produces the SAME `Symbol` shape
//! the LSP path returns, so callers are agnostic to which backend answered.
//! Used as the `find_symbol`/`get_symbols_overview` fallback until the LSP
//! reports `IndexStatus::Ready`.

use tabbify_workspace_contract::rpc::{Location, Position, Range, Symbol};
use tree_sitter::{Node, Parser};

/// Extract top-level + impl-method symbols from a rust source string.
pub fn rust_symbols(src: &str, repo: &str, path: &str) -> Vec<Symbol> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_rust::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(src, None) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let root = tree.root_node();
    let bytes = src.as_bytes();
    walk(root, bytes, repo, path, "", &mut out);
    out
}

/// Recurse named definition nodes, building Serena-style `name_path` prefixes.
fn walk(node: Node, src: &[u8], repo: &str, path: &str, prefix: &str, out: &mut Vec<Symbol>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let kind = match child.kind() {
            "function_item" => Some("function"),
            "struct_item" => Some("class"),
            "enum_item" => Some("enum"),
            "trait_item" => Some("interface"),
            "mod_item" => Some("module"),
            "impl_item" => Some("impl"),
            _ => None,
        };
        if let Some(k) = kind {
            // impl blocks contribute their methods under the impl's type name.
            // An `impl_item` has no `name` field — its target type lives in the
            // `type` field — and its methods sit inside the nested
            // `declaration_list`, so we descend into that body with the impl's
            // type name as the `name_path` prefix.
            if child.kind() == "impl_item" {
                let impl_name = impl_type_name(child, src).unwrap_or_default();
                if let Some(body) = child.child_by_field_name("body") {
                    walk(body, src, repo, path, &impl_name, out);
                }
                continue;
            }
            let name = name_of(child, src).unwrap_or_default();
            let name_path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            out.push(Symbol {
                name,
                name_path,
                kind: k.to_string(),
                location: Location {
                    repo: repo.to_string(),
                    path: path.to_string(),
                    range: node_range(child),
                },
                body: None,
                children: Vec::new(),
            });
        }
    }
}

/// The identifier text of a definition node (its `name` field, when present).
fn name_of(node: Node, src: &[u8]) -> Option<String> {
    let name_node = node.child_by_field_name("name")?;
    name_node.utf8_text(src).ok().map(|s| s.to_string())
}

/// The target type name of an `impl_item` (its `type` field — `impl Cfg` →
/// `"Cfg"`). `impl_item` has no `name` field, so [`name_of`] does not apply.
fn impl_type_name(node: Node, src: &[u8]) -> Option<String> {
    let ty = node.child_by_field_name("type")?;
    ty.utf8_text(src).ok().map(|s| s.to_string())
}

/// Convert a tree-sitter node span to the contract's 0-based LSP `Range`.
fn node_range(node: Node) -> Range {
    let s = node.start_position();
    let e = node.end_position();
    Range {
        start: Position { line: s.row as u32, character: s.column as u32 },
        end: Position { line: e.row as u32, character: e.column as u32 },
    }
}
