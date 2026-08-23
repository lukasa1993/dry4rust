use proc_macro2::{TokenStream, TokenTree};
use rustc_lexer::{LiteralKind, TokenKind};
use serde::Serialize;
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;
use syn::{Attribute, Item};
use thiserror::Error;
use walkdir::{DirEntry, WalkDir};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Rust parse error in {path}: {source}")]
    Parse { path: PathBuf, source: syn::Error },
    #[error("invalid argument: {0}")]
    Argument(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub value: String,
    pub line: usize,
    pub index: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Location {
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(skip)]
    pub start_token: usize,
    #[serde(skip)]
    pub end_token: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Duplicate {
    pub token_count: usize,
    pub locations: Vec<Location>,
}

fn ignored(entry: &DirEntry) -> bool {
    matches!(
        entry.file_name().to_str(),
        Some(".git" | "target" | "vendor" | "node_modules" | ".venv" | "venv" | "build" | "dist")
    )
}

fn is_test_path(path: &Path, root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    relative
        .components()
        .any(|part| part.as_os_str() == "tests")
        || relative
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_test.rs"))
}

fn is_auxiliary_path(path: &Path, root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    if relative.file_name().and_then(|name| name.to_str()) == Some("build.rs") {
        return true;
    }
    relative.components().any(|part| {
        matches!(
            part.as_os_str().to_str(),
            Some("examples" | "benches" | "fuzz")
        )
    })
}

pub fn discover_files(root: &Path, include_tests: bool, filters: &[String]) -> Vec<PathBuf> {
    let mut files: Vec<_> = WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| !ignored(entry))
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("rs"))
        .filter(|path| !is_auxiliary_path(path, root))
        .filter(|path| include_tests || !is_test_path(path, root))
        .filter(|path| {
            if filters.is_empty() {
                true
            } else {
                let relative = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
                filters.iter().any(|filter| relative.contains(filter))
            }
        })
        .collect();
    files.sort();
    files
}

#[derive(Clone, Copy)]
struct CfgPossibility {
    can_be_true_without_test: bool,
    can_be_false_without_test: bool,
}

impl CfgPossibility {
    const UNKNOWN: Self = Self {
        can_be_true_without_test: true,
        can_be_false_without_test: true,
    };
}

fn split_cfg_arguments(stream: TokenStream) -> Vec<TokenStream> {
    let mut output = Vec::new();
    let mut current = TokenStream::new();
    for token in stream {
        if matches!(&token, TokenTree::Punct(value) if value.as_char() == ',') {
            if !current.is_empty() {
                output.push(current);
                current = TokenStream::new();
            }
        } else {
            current.extend(std::iter::once(token));
        }
    }
    if !current.is_empty() {
        output.push(current);
    }
    output
}

fn cfg_possibility(stream: TokenStream) -> CfgPossibility {
    let tokens: Vec<_> = stream.into_iter().collect();
    if tokens.len() == 1 {
        return match &tokens[0] {
            TokenTree::Ident(value) if value == "test" => CfgPossibility {
                can_be_true_without_test: false,
                can_be_false_without_test: true,
            },
            TokenTree::Group(group) => cfg_possibility(group.stream()),
            _ => CfgPossibility::UNKNOWN,
        };
    }
    if tokens.len() != 2 {
        return CfgPossibility::UNKNOWN;
    }
    let (TokenTree::Ident(operation), TokenTree::Group(group)) = (&tokens[0], &tokens[1]) else {
        return CfgPossibility::UNKNOWN;
    };
    let possibilities: Vec<_> = split_cfg_arguments(group.stream())
        .into_iter()
        .map(cfg_possibility)
        .collect();
    match operation.to_string().as_str() {
        "all" => CfgPossibility {
            can_be_true_without_test: possibilities
                .iter()
                .all(|value| value.can_be_true_without_test),
            can_be_false_without_test: possibilities
                .iter()
                .any(|value| value.can_be_false_without_test),
        },
        "any" => CfgPossibility {
            can_be_true_without_test: possibilities
                .iter()
                .any(|value| value.can_be_true_without_test),
            can_be_false_without_test: possibilities
                .iter()
                .all(|value| value.can_be_false_without_test),
        },
        "not" if possibilities.len() == 1 => CfgPossibility {
            can_be_true_without_test: possibilities[0].can_be_false_without_test,
            can_be_false_without_test: possibilities[0].can_be_true_without_test,
        },
        _ => CfgPossibility::UNKNOWN,
    }
}

fn attrs_are_test_only(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attribute| {
        if attribute.path().is_ident("test") {
            return true;
        }
        match &attribute.meta {
            syn::Meta::List(list) if list.path.is_ident("cfg") => {
                !cfg_possibility(list.tokens.clone()).can_be_true_without_test
            }
            _ => false,
        }
    })
}

fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(value) => &value.attrs,
        Item::Enum(value) => &value.attrs,
        Item::ExternCrate(value) => &value.attrs,
        Item::Fn(value) => &value.attrs,
        Item::ForeignMod(value) => &value.attrs,
        Item::Impl(value) => &value.attrs,
        Item::Macro(value) => &value.attrs,
        Item::Mod(value) => &value.attrs,
        Item::Static(value) => &value.attrs,
        Item::Struct(value) => &value.attrs,
        Item::Trait(value) => &value.attrs,
        Item::TraitAlias(value) => &value.attrs,
        Item::Type(value) => &value.attrs,
        Item::Union(value) => &value.attrs,
        Item::Use(value) => &value.attrs,
        _ => &[],
    }
}

fn item_range(attrs: &[Attribute], item: &impl Spanned) -> Range<usize> {
    let item_range = item.span().byte_range();
    let start = attrs
        .first()
        .map(|attribute| attribute.span().byte_range().start)
        .unwrap_or(item_range.start);
    start..item_range.end
}

fn collect_test_only_ranges(items: &[Item], output: &mut Vec<Range<usize>>) {
    for item in items {
        let attrs = item_attrs(item);
        if attrs_are_test_only(attrs) {
            output.push(item_range(attrs, item));
            continue;
        }
        match item {
            Item::Mod(module) => {
                if let Some((_, nested)) = &module.content {
                    collect_test_only_ranges(nested, output);
                }
            }
            Item::Impl(implementation) => {
                for member in &implementation.items {
                    let attrs = match member {
                        syn::ImplItem::Const(value) => &value.attrs[..],
                        syn::ImplItem::Fn(value) => &value.attrs[..],
                        syn::ImplItem::Type(value) => &value.attrs[..],
                        syn::ImplItem::Macro(value) => &value.attrs[..],
                        _ => &[],
                    };
                    if attrs_are_test_only(attrs) {
                        output.push(item_range(attrs, member));
                    }
                }
            }
            Item::Trait(trait_item) => {
                for member in &trait_item.items {
                    let attrs = match member {
                        syn::TraitItem::Const(value) => &value.attrs[..],
                        syn::TraitItem::Fn(value) => &value.attrs[..],
                        syn::TraitItem::Type(value) => &value.attrs[..],
                        syn::TraitItem::Macro(value) => &value.attrs[..],
                        _ => &[],
                    };
                    if attrs_are_test_only(attrs) {
                        output.push(item_range(attrs, member));
                    }
                }
            }
            _ => {}
        }
    }
}

fn is_keyword(text: &str) -> bool {
    matches!(
        text,
        "_" | "as"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "gen"
            | "macro"
            | "macro_rules"
            | "override"
            | "priv"
            | "raw"
            | "safe"
            | "typeof"
            | "union"
            | "unsized"
            | "virtual"
            | "yield"
            | "try"
    )
}

fn normalize_literal(kind: LiteralKind) -> &'static str {
    match kind {
        LiteralKind::Int { .. } | LiteralKind::Float { .. } => "NUM",
        LiteralKind::Char { .. }
        | LiteralKind::Byte { .. }
        | LiteralKind::Str { .. }
        | LiteralKind::ByteStr { .. }
        | LiteralKind::RawStr { .. }
        | LiteralKind::RawByteStr { .. } => "STR",
    }
}

fn normalized_tokens_with_tests(path: &Path, include_tests: bool) -> Result<Vec<Token>, Error> {
    let source = fs::read_to_string(path)?;
    let syntax = syn::parse_file(&source).map_err(|source_error| Error::Parse {
        path: path.to_path_buf(),
        source: source_error,
    })?;
    let mut excluded_ranges = Vec::new();
    if !include_tests {
        collect_test_only_ranges(&syntax.items, &mut excluded_ranges);
    }
    let shebang = rustc_lexer::strip_shebang(&source).unwrap_or(0);
    let mut offset = shebang;
    let mut line = 1 + source[..shebang]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    let mut output = Vec::new();
    for raw in rustc_lexer::tokenize(&source[shebang..]) {
        let end = offset + raw.len;
        let text = &source[offset..end];
        let start_line = line;
        line += text.bytes().filter(|byte| *byte == b'\n').count();
        let token_range = offset..end;
        offset = end;
        if excluded_ranges
            .iter()
            .any(|excluded| excluded.start <= token_range.start && token_range.end <= excluded.end)
        {
            continue;
        }
        let normalized = match raw.kind {
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. } => None,
            TokenKind::Ident => Some(if is_keyword(text) {
                text.to_string()
            } else {
                "ID".into()
            }),
            TokenKind::RawIdent => Some("ID".into()),
            TokenKind::Literal { kind, .. } => Some(normalize_literal(kind).into()),
            TokenKind::Lifetime { .. } => Some("LIFETIME".into()),
            _ => Some(text.to_string()),
        };
        if let Some(value) = normalized.filter(|value| !value.trim().is_empty()) {
            output.push(Token {
                value,
                line: start_line,
                index: output.len(),
            });
        }
    }
    Ok(output)
}

pub fn normalized_tokens(path: &Path) -> Result<Vec<Token>, Error> {
    normalized_tokens_with_tests(path, false)
}

fn window_hash(tokens: &[Token], start: usize, size: usize) -> u64 {
    let mut hasher = DefaultHasher::new();
    for token in &tokens[start..start + size] {
        token.value.hash(&mut hasher);
        0_u8.hash(&mut hasher);
    }
    hasher.finish()
}

fn windows_equal(a: &[Token], ai: usize, b: &[Token], bi: usize, size: usize) -> bool {
    a.get(ai..ai + size)
        .zip(b.get(bi..bi + size))
        .is_some_and(|(left, right)| {
            left.iter()
                .zip(right)
                .all(|(first, second)| first.value == second.value)
        })
}

fn extend(a: &[Token], ai: usize, b: &[Token], bi: usize, minimum: usize) -> (usize, usize, usize) {
    let mut left = 0;
    while ai > left && bi > left && a[ai - left - 1].value == b[bi - left - 1].value {
        left += 1;
    }
    let mut right = minimum;
    while ai + right < a.len() && bi + right < b.len() && a[ai + right].value == b[bi + right].value
    {
        right += 1;
    }
    (ai - left, bi - left, right + left)
}

fn location(file: &str, tokens: &[Token], start: usize, size: usize) -> Location {
    Location {
        file: file.to_string(),
        start_line: tokens[start].line,
        end_line: tokens[start + size - 1].line,
        start_token: start,
        end_token: start + size,
    }
}

fn overlaps(a: &Location, b: &Location) -> bool {
    a.file == b.file && !(a.end_token <= b.start_token || b.end_token <= a.start_token)
}

fn contained(candidate: &Duplicate, existing: &Duplicate) -> bool {
    if candidate.locations.len() < 2 || existing.locations.len() < 2 {
        return false;
    }
    let a = &candidate.locations[0];
    let b = &candidate.locations[1];
    let x = &existing.locations[0];
    let y = &existing.locations[1];
    a.file == x.file
        && b.file == y.file
        && x.start_token <= a.start_token
        && a.end_token <= x.end_token
        && y.start_token <= b.start_token
        && b.end_token <= y.end_token
}

pub fn find_duplicates(
    root: &Path,
    min_tokens: usize,
    max_groups: usize,
    max_occurrences_per_window: usize,
    include_tests: bool,
    filters: &[String],
) -> Result<Vec<Duplicate>, Error> {
    if min_tokens < 4 {
        return Err(Error::Argument("min-tokens must be at least 4".into()));
    }
    if max_groups == 0 || max_occurrences_per_window < 2 {
        return Err(Error::Argument(
            "group and occurrence limits must be positive".into(),
        ));
    }
    let files = discover_files(root, include_tests, filters);
    let mut names = Vec::new();
    let mut token_sets = Vec::new();
    for path in files {
        names.push(
            path.strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/"),
        );
        token_sets.push(normalized_tokens_with_tests(&path, include_tests)?);
    }
    let mut windows: HashMap<u64, Vec<(usize, usize)>> = HashMap::new();
    for (file_index, tokens) in token_sets.iter().enumerate() {
        if tokens.len() < min_tokens {
            continue;
        }
        for start in 0..=tokens.len() - min_tokens {
            let values = windows
                .entry(window_hash(tokens, start, min_tokens))
                .or_default();
            if values.len() < max_occurrences_per_window {
                values.push((file_index, start));
            }
        }
    }
    let mut candidates = Vec::new();
    let mut seen = HashMap::<(String, usize, String, usize, usize), ()>::new();
    for occurrences in windows.values().filter(|values| values.len() > 1) {
        for left in 0..occurrences.len() {
            for right in left + 1..occurrences.len() {
                let (af, ai) = occurrences[left];
                let (bf, bi) = occurrences[right];
                if !windows_equal(&token_sets[af], ai, &token_sets[bf], bi, min_tokens) {
                    continue;
                }
                let (a_start, b_start, size) =
                    extend(&token_sets[af], ai, &token_sets[bf], bi, min_tokens);
                let mut a = location(&names[af], &token_sets[af], a_start, size);
                let mut b = location(&names[bf], &token_sets[bf], b_start, size);
                if overlaps(&a, &b) {
                    continue;
                }
                if (b.file.as_str(), b.start_token) < (a.file.as_str(), a.start_token) {
                    std::mem::swap(&mut a, &mut b);
                }
                let key = (
                    a.file.clone(),
                    a.start_token,
                    b.file.clone(),
                    b.start_token,
                    size,
                );
                if seen.insert(key, ()).is_none() {
                    candidates.push(Duplicate {
                        token_count: size,
                        locations: vec![a, b],
                    });
                }
            }
        }
    }
    candidates.sort_by(|a, b| {
        b.token_count
            .cmp(&a.token_count)
            .then_with(|| a.locations[0].file.cmp(&b.locations[0].file))
            .then_with(|| a.locations[0].start_token.cmp(&b.locations[0].start_token))
            .then_with(|| a.locations[1].file.cmp(&b.locations[1].file))
            .then_with(|| a.locations[1].start_token.cmp(&b.locations[1].start_token))
    });
    let mut selected: Vec<Duplicate> = Vec::new();
    for candidate in candidates {
        if selected
            .iter()
            .any(|existing| contained(&candidate, existing))
        {
            continue;
        }
        selected.push(candidate);
        if selected.len() >= max_groups {
            break;
        }
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn current_rust_keywords_are_preserved_by_normalization() {
        for keyword in ["_", "gen", "macro_rules", "raw", "safe", "union"] {
            assert!(
                is_keyword(keyword),
                "{keyword} must be treated as Rust syntax"
            );
        }
        assert!(!is_keyword("ordinary_identifier"));
    }

    #[test]
    fn token_normalization_ignores_comments_and_values() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.rs");
        fs::write(
            &path,
            "fn alpha(x: i32) -> i32 { /* name */ let y = 42; x + y + \"value\".len() as i32 }\n",
        )
        .unwrap();
        let tokens = normalized_tokens(&path).unwrap();
        assert!(tokens.iter().any(|token| token.value == "ID"));
        assert!(tokens.iter().any(|token| token.value == "NUM"));
        assert!(tokens.iter().any(|token| token.value == "STR"));
        assert!(!tokens.iter().any(|token| token.value.contains("name")));
    }

    #[test]
    fn inline_test_modules_are_excluded_by_default() {
        let dir = tempdir().unwrap();
        let production = dir.path().join("production.rs");
        let with_tests = dir.path().join("with_tests.rs");
        let body = "fn production(a: i32) -> i32 { let b = a + 1; b * 2 }\n";
        fs::write(&production, body).unwrap();
        fs::write(
            &with_tests,
            format!(
                "{body}#[cfg(test)]\nmod tests {{ #[test] fn repeated_test_logic() {{ let x = 1 + 2; assert_eq!(x * 2, 6); }} }}\n"
            ),
        )
        .unwrap();
        assert_eq!(
            normalized_tokens(&production).unwrap(),
            normalized_tokens(&with_tests).unwrap()
        );
    }

    #[test]
    fn cfg_not_test_code_remains_in_token_stream() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.rs");
        fs::write(
            &path,
            "#[cfg(not(test))]\nfn production() -> bool { true }\n",
        )
        .unwrap();
        let tokens = normalized_tokens(&path).unwrap();
        assert!(tokens.iter().any(|token| token.value == "true"));
    }

    #[test]
    fn discovery_excludes_build_scripts_and_auxiliary_targets() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::create_dir_all(dir.path().join("benches")).unwrap();
        fs::write(dir.path().join("src/lib.rs"), "fn production() {}\n").unwrap();
        fs::write(dir.path().join("build.rs"), "fn main() {}\n").unwrap();
        fs::write(dir.path().join("benches/bench.rs"), "fn main() {}\n").unwrap();
        assert_eq!(
            discover_files(dir.path(), false, &[]),
            vec![dir.path().join("src/lib.rs")]
        );
    }

    #[test]
    fn candidate_windows_must_match_even_when_hash_bucket_matches() {
        let left = vec![
            Token {
                value: "a".into(),
                line: 1,
                index: 0,
            },
            Token {
                value: "b".into(),
                line: 1,
                index: 1,
            },
            Token {
                value: "c".into(),
                line: 1,
                index: 2,
            },
            Token {
                value: "d".into(),
                line: 1,
                index: 3,
            },
        ];
        let right = vec![
            Token {
                value: "a".into(),
                line: 1,
                index: 0,
            },
            Token {
                value: "b".into(),
                line: 1,
                index: 1,
            },
            Token {
                value: "x".into(),
                line: 1,
                index: 2,
            },
            Token {
                value: "d".into(),
                line: 1,
                index: 3,
            },
        ];
        assert!(!windows_equal(&left, 0, &right, 0, 4));
    }

    #[test]
    fn finds_non_overlapping_same_file_duplicate() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.rs");
        fs::write(&path, "fn first(a: i32) -> i32 { let b = a + 1; if b > 2 { b * 2 } else { b - 1 } }\nfn second(x: i32) -> i32 { let y = x + 9; if y > 8 { y * 7 } else { y - 3 } }\n").unwrap();
        let duplicates = find_duplicates(dir.path(), 12, 20, 100, false, &[]).unwrap();
        assert!(!duplicates.is_empty());
        assert!(duplicates
            .iter()
            .any(|group| group.locations[0].file == group.locations[1].file));
    }

    #[test]
    fn rejects_tiny_windows() {
        let dir = tempdir().unwrap();
        assert!(find_duplicates(dir.path(), 3, 10, 10, false, &[]).is_err());
    }
}
