#[path = "lib.rs"]
mod legacy;
mod scope;

pub use legacy::{discover_files, normalized_tokens, Duplicate, Error, Location, Token, VERSION};

use rustc_lexer::{LiteralKind, TokenKind};
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::Path;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, ForeignItem, ImplItem, Item, TraitItem};

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

fn impl_item_attrs(item: &ImplItem) -> &[Attribute] {
    match item {
        ImplItem::Const(value) => &value.attrs,
        ImplItem::Fn(value) => &value.attrs,
        ImplItem::Type(value) => &value.attrs,
        ImplItem::Macro(value) => &value.attrs,
        _ => &[],
    }
}

fn trait_item_attrs(item: &TraitItem) -> &[Attribute] {
    match item {
        TraitItem::Const(value) => &value.attrs,
        TraitItem::Fn(value) => &value.attrs,
        TraitItem::Type(value) => &value.attrs,
        TraitItem::Macro(value) => &value.attrs,
        _ => &[],
    }
}

fn foreign_item_attrs(item: &ForeignItem) -> &[Attribute] {
    match item {
        ForeignItem::Fn(value) => &value.attrs,
        ForeignItem::Static(value) => &value.attrs,
        ForeignItem::Type(value) => &value.attrs,
        ForeignItem::Macro(value) => &value.attrs,
        _ => &[],
    }
}

fn expr_attrs(expr: &Expr) -> &[Attribute] {
    match expr {
        Expr::Array(value) => &value.attrs,
        Expr::Assign(value) => &value.attrs,
        Expr::Async(value) => &value.attrs,
        Expr::Await(value) => &value.attrs,
        Expr::Binary(value) => &value.attrs,
        Expr::Block(value) => &value.attrs,
        Expr::Break(value) => &value.attrs,
        Expr::Call(value) => &value.attrs,
        Expr::Cast(value) => &value.attrs,
        Expr::Closure(value) => &value.attrs,
        Expr::Const(value) => &value.attrs,
        Expr::Continue(value) => &value.attrs,
        Expr::Field(value) => &value.attrs,
        Expr::ForLoop(value) => &value.attrs,
        Expr::Group(value) => &value.attrs,
        Expr::If(value) => &value.attrs,
        Expr::Index(value) => &value.attrs,
        Expr::Infer(value) => &value.attrs,
        Expr::Let(value) => &value.attrs,
        Expr::Lit(value) => &value.attrs,
        Expr::Loop(value) => &value.attrs,
        Expr::Macro(value) => &value.attrs,
        Expr::Match(value) => &value.attrs,
        Expr::MethodCall(value) => &value.attrs,
        Expr::Paren(value) => &value.attrs,
        Expr::Path(value) => &value.attrs,
        Expr::Range(value) => &value.attrs,
        Expr::RawAddr(value) => &value.attrs,
        Expr::Reference(value) => &value.attrs,
        Expr::Repeat(value) => &value.attrs,
        Expr::Return(value) => &value.attrs,
        Expr::Struct(value) => &value.attrs,
        Expr::Try(value) => &value.attrs,
        Expr::TryBlock(value) => &value.attrs,
        Expr::Tuple(value) => &value.attrs,
        Expr::Unary(value) => &value.attrs,
        Expr::Unsafe(value) => &value.attrs,
        Expr::While(value) => &value.attrs,
        Expr::Yield(value) => &value.attrs,
        _ => &[],
    }
}

fn range_with_attrs(attrs: &[Attribute], node: &impl Spanned) -> Range<usize> {
    let range = node.span().byte_range();
    let start = attrs
        .first()
        .map(|attribute| attribute.span().byte_range().start)
        .unwrap_or(range.start);
    start..range.end
}

struct InactiveRangeVisitor<'a> {
    cfg: &'a scope::CfgContext,
    ranges: Vec<Range<usize>>,
}

impl InactiveRangeVisitor<'_> {
    fn inactive(&mut self, attrs: &[Attribute], node: &impl Spanned) -> bool {
        if self.cfg.attrs_active(attrs) {
            false
        } else {
            self.ranges.push(range_with_attrs(attrs, node));
            true
        }
    }
}

impl<'ast> Visit<'ast> for InactiveRangeVisitor<'_> {
    fn visit_item(&mut self, node: &'ast Item) {
        if self.inactive(item_attrs(node), node) {
            return;
        }
        visit::visit_item(self, node);
    }

    fn visit_impl_item(&mut self, node: &'ast ImplItem) {
        if self.inactive(impl_item_attrs(node), node) {
            return;
        }
        visit::visit_impl_item(self, node);
    }

    fn visit_trait_item(&mut self, node: &'ast TraitItem) {
        if self.inactive(trait_item_attrs(node), node) {
            return;
        }
        visit::visit_trait_item(self, node);
    }

    fn visit_foreign_item(&mut self, node: &'ast ForeignItem) {
        if self.inactive(foreign_item_attrs(node), node) {
            return;
        }
        visit::visit_foreign_item(self, node);
    }

    fn visit_local(&mut self, node: &'ast syn::Local) {
        if self.inactive(&node.attrs, node) {
            return;
        }
        visit::visit_local(self, node);
    }

    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        if self.inactive(&node.attrs, node) {
            return;
        }
        visit::visit_arm(self, node);
    }

    fn visit_field(&mut self, node: &'ast syn::Field) {
        if self.inactive(&node.attrs, node) {
            return;
        }
        visit::visit_field(self, node);
    }

    fn visit_variant(&mut self, node: &'ast syn::Variant) {
        if self.inactive(&node.attrs, node) {
            return;
        }
        visit::visit_variant(self, node);
    }

    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        if self.inactive(&node.attrs, node) {
            return;
        }
        visit::visit_stmt_macro(self, node);
    }

    fn visit_expr(&mut self, node: &'ast Expr) {
        if self.inactive(expr_attrs(node), node) {
            return;
        }
        visit::visit_expr(self, node);
    }
}

fn inactive_ranges(file: &syn::File, cfg: &scope::CfgContext) -> Vec<Range<usize>> {
    let mut visitor = InactiveRangeVisitor {
        cfg,
        ranges: Vec::new(),
    };
    visitor.visit_file(file);
    visitor.ranges.sort_by_key(|range| (range.start, range.end));
    visitor.ranges
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

fn normalized_active_tokens(active: &scope::ActiveFile) -> Result<Vec<Token>, Error> {
    let source = fs::read_to_string(&active.path)?;
    let syntax = syn::parse_file(&source).map_err(|source_error| Error::Parse {
        path: active.path.clone(),
        source: source_error,
    })?;
    let excluded_ranges = inactive_ranges(&syntax, &active.cfg);
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
    while ai + right < a.len()
        && bi + right < b.len()
        && a[ai + right].value == b[bi + right].value
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
    if !root.join("Cargo.toml").is_file() {
        return legacy::find_duplicates(
            root,
            min_tokens,
            max_groups,
            max_occurrences_per_window,
            include_tests,
            filters,
        );
    }
    let files = scope::discover(root, include_tests, filters).map_err(Error::Argument)?;
    let mut names = Vec::new();
    let mut token_sets = Vec::new();
    for active in files {
        names.push(
            active
                .path
                .strip_prefix(root)
                .unwrap_or(&active.path)
                .to_string_lossy()
                .replace('\\', "/"),
        );
        token_sets.push(normalized_active_tokens(&active)?);
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
    let mut selected = Vec::new();
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
    use tempfile::tempdir;

    fn project(name: &str) -> tempfile::TempDir {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n"),
        )
        .unwrap();
        dir
    }

    #[test]
    fn inactive_inline_cfg_items_do_not_create_duplicates() {
        let dir = project("dry-inline-cfg-fixture");
        fs::write(
            dir.path().join("src/lib.rs"),
            "pub fn active() -> i32 { 1 }\n#[cfg(windows)]\nfn hidden_a(x: i32) -> i32 { let y = x + 1; if y > 2 { y * 2 } else { y - 1 } }\n#[cfg(windows)]\nfn hidden_b(x: i32) -> i32 { let y = x + 1; if y > 2 { y * 2 } else { y - 1 } }\n",
        )
        .unwrap();
        let duplicates = find_duplicates(dir.path(), 10, 20, 100, false, &[]).unwrap();
        if cfg!(not(windows)) {
            assert!(duplicates.is_empty());
        }
    }

    #[test]
    fn cfg_disabled_local_statements_do_not_create_duplicates() {
        let dir = project("dry-local-cfg-fixture");
        fs::write(
            dir.path().join("src/lib.rs"),
            "pub fn first() { #[cfg(windows)] let x = { let a = 1; let b = a + 2; let c = b * 3; c }; }\npub fn second() { #[cfg(windows)] let x = { let a = 1; let b = a + 2; let c = b * 3; c }; }\n",
        )
        .unwrap();
        let duplicates = find_duplicates(dir.path(), 8, 20, 100, false, &[]).unwrap();
        if cfg!(not(windows)) {
            assert!(duplicates.is_empty());
        }
    }

    #[test]
    fn cfg_disabled_match_arms_do_not_create_duplicates() {
        let dir = project("dry-arm-cfg-fixture");
        fs::write(
            dir.path().join("src/lib.rs"),
            "pub fn first(x: i32) -> i32 { match x { #[cfg(windows)] 1 => { let a=1; let b=a+2; b*3 }, _ => 0 } }\npub fn second(x: i32) -> i32 { match x { #[cfg(windows)] 1 => { let a=1; let b=a+2; b*3 }, _ => 0 } }\n",
        )
        .unwrap();
        let duplicates = find_duplicates(dir.path(), 8, 20, 100, false, &[]).unwrap();
        if cfg!(not(windows)) {
            assert!(duplicates.is_empty());
        }
    }

    #[test]
    fn active_static_include_participates_in_duplicate_detection() {
        let dir = project("dry-include-fixture");
        fs::write(dir.path().join("src/lib.rs"), "include!(\"shared_a.rs\"); include!(\"shared_b.rs\");\n").unwrap();
        let body = "pub fn repeated(x: i32) -> i32 { let y = x + 1; if y > 2 { y * 2 } else { y - 1 } }\n";
        fs::write(dir.path().join("src/shared_a.rs"), body).unwrap();
        fs::write(dir.path().join("src/shared_b.rs"), body).unwrap();
        let duplicates = find_duplicates(dir.path(), 10, 20, 100, false, &[]).unwrap();
        assert!(!duplicates.is_empty());
    }
}
