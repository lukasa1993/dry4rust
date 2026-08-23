use std::fs;

fn replace_once(text: &mut String, old: &str, new: &str, label: &str) {
    let start = text
        .find(old)
        .unwrap_or_else(|| panic!("missing anchor: {}", label));
    assert!(
        text[start + old.len()..].find(old).is_none(),
        "duplicate anchor: {}",
        label
    );
    text.replace_range(start..start + old.len(), new);
}

fn replace_between(text: &mut String, start: &str, end: &str, new: &str, label: &str) {
    let from = text
        .find(start)
        .unwrap_or_else(|| panic!("missing start anchor: {}", label));
    let relative = text[from..]
        .find(end)
        .unwrap_or_else(|| panic!("missing end anchor: {}", label));
    text.replace_range(from..from + relative, new);
}

fn insert_before(text: &mut String, marker: &str, addition: &str, label: &str) {
    let index = text
        .find(marker)
        .unwrap_or_else(|| panic!("missing insertion anchor: {}", label));
    text.insert_str(index, addition);
}

fn main() {
    let mut source = fs::read_to_string("src/scoped_lib.rs").unwrap();

    replace_once(
        &mut source,
        "use rustc_lexer::{LiteralKind, TokenKind};\nuse std::collections::{hash_map::DefaultHasher, HashMap};\nuse std::fs;\nuse std::hash::{Hash, Hasher};\nuse std::ops::Range;\nuse std::path::Path;\nuse syn::spanned::Spanned;\nuse syn::visit::{self, Visit};\nuse syn::{Attribute, Expr, ForeignItem, ImplItem, Item, TraitItem};",
        "use proc_macro2::TokenStream;\nuse rustc_lexer::{LiteralKind, TokenKind};\nuse std::collections::{hash_map::DefaultHasher, HashMap, HashSet, VecDeque};\nuse std::fs;\nuse std::hash::{Hash, Hasher};\nuse std::ops::Range;\nuse std::path::{Path, PathBuf};\nuse syn::parse::{ParseStream, Parser};\nuse syn::spanned::Spanned;\nuse syn::visit::{self, Visit};\nuse syn::{\n    Attribute, Expr, FnArg, ForeignItem, GenericParam, ImplItem, Item, LitStr, Token,\n    TraitItem,\n};",
        "DRY traversal imports",
    );

    let parameter_helpers = r###"fn fn_arg_attrs(argument: &FnArg) -> &[Attribute] {
    match argument {
        FnArg::Receiver(value) => &value.attrs,
        FnArg::Typed(value) => &value.attrs,
    }
}

fn generic_param_attrs(parameter: &GenericParam) -> &[Attribute] {
    match parameter {
        GenericParam::Lifetime(value) => &value.attrs,
        GenericParam::Type(value) => &value.attrs,
        GenericParam::Const(value) => &value.attrs,
    }
}

"###;
    insert_before(
        &mut source,
        "fn expr_attrs(expr: &Expr)",
        parameter_helpers,
        "parameter attribute helpers",
    );

    let parameter_visitors = r###"    fn visit_fn_arg(&mut self, node: &'ast FnArg) {
        if self.inactive(fn_arg_attrs(node), node) {
            return;
        }
        visit::visit_fn_arg(self, node);
    }

    fn visit_generic_param(&mut self, node: &'ast GenericParam) {
        if self.inactive(generic_param_attrs(node), node) {
            return;
        }
        visit::visit_generic_param(self, node);
    }

"###;
    insert_before(
        &mut source,
        "    fn visit_local(&mut self, node: &'ast syn::Local)",
        parameter_visitors,
        "parameter cfg visitors",
    );

    replace_once(
        &mut source,
        "fn inactive_ranges(file: &syn::File, cfg: &scope::CfgContext) -> Vec<Range<usize>> {\n    let mut visitor = InactiveRangeVisitor {\n        cfg,\n        ranges: Vec::new(),\n    };\n    visitor.visit_file(file);\n    visitor.ranges.sort_by_key(|range| (range.start, range.end));\n    visitor.ranges\n}",
        "fn inactive_ranges(file: &syn::File, cfg: &scope::CfgContext) -> Vec<Range<usize>> {\n    let mut visitor = InactiveRangeVisitor {\n        cfg,\n        ranges: Vec::new(),\n    };\n    visitor.visit_file(file);\n    visitor.ranges.sort_by_key(|range| (range.start, range.end));\n    visitor.ranges\n}\n\nfn inactive_expr_ranges(expr: &Expr, cfg: &scope::CfgContext) -> Vec<Range<usize>> {\n    let mut visitor = InactiveRangeVisitor {\n        cfg,\n        ranges: Vec::new(),\n    };\n    visitor.visit_expr(expr);\n    visitor.ranges.sort_by_key(|range| (range.start, range.end));\n    visitor.ranges\n}",
        "expression inactive ranges",
    );

    let source_processing = r###"fn normalized_source_tokens(path: &Path, cfg: &scope::CfgContext) -> Result<Vec<Token>, Error> {
    let source = fs::read_to_string(path)?;
    let excluded_ranges = if let Ok(file) = syn::parse_file(&source) {
        inactive_ranges(&file, cfg)
    } else if let Ok(expr) = syn::parse_str::<Expr>(&source) {
        inactive_expr_ranges(&expr, cfg)
    } else {
        Vec::new()
    };
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

fn include_literal(tokens: TokenStream) -> Option<LitStr> {
    let parser = |input: ParseStream<'_>| {
        let literal: LitStr = input.parse()?;
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("include! expects one string literal"));
        }
        Ok(literal)
    };
    parser.parse2(tokens).ok()
}

fn built_in_include(path: &syn::Path) -> bool {
    let segments: Vec<_> = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    matches!(segments.as_slice(), [name] if name == "include")
        || matches!(segments.as_slice(), [prefix, name]
            if matches!(prefix.as_str(), "std" | "core") && name == "include")
}

fn include_path(value: &syn::Macro, source_dir: &Path) -> Option<PathBuf> {
    if !built_in_include(&value.path) {
        return None;
    }
    let literal = include_literal(value.tokens.clone())?;
    let path = PathBuf::from(literal.value());
    Some(if path.is_absolute() {
        path
    } else {
        source_dir.join(path)
    })
}

struct IncludeVisitor<'a> {
    source_dir: &'a Path,
    excluded_ranges: &'a [Range<usize>],
    paths: Vec<PathBuf>,
}

impl IncludeVisitor<'_> {
    fn excluded(&self, value: &syn::Macro) -> bool {
        let range = value.span().byte_range();
        self.excluded_ranges
            .iter()
            .any(|excluded| excluded.start <= range.start && range.end <= excluded.end)
    }
}

impl<'ast> Visit<'ast> for IncludeVisitor<'_> {
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if !self.excluded(node) {
            if let Some(path) = include_path(node, self.source_dir) {
                self.paths.push(path);
            }
            visit::visit_macro(self, node);
        }
    }
}

fn included_paths(path: &Path, cfg: &scope::CfgContext) -> Result<Vec<PathBuf>, Error> {
    let source = fs::read_to_string(path)?;
    let source_dir = path.parent().unwrap_or_else(|| Path::new("."));
    if let Ok(file) = syn::parse_file(&source) {
        let ranges = inactive_ranges(&file, cfg);
        let mut visitor = IncludeVisitor {
            source_dir,
            excluded_ranges: &ranges,
            paths: Vec::new(),
        };
        visitor.visit_file(&file);
        return Ok(visitor.paths);
    }
    if let Ok(expr) = syn::parse_str::<Expr>(&source) {
        let ranges = inactive_expr_ranges(&expr, cfg);
        let mut visitor = IncludeVisitor {
            source_dir,
            excluded_ranges: &ranges,
            paths: Vec::new(),
        };
        visitor.visit_expr(&expr);
        return Ok(visitor.paths);
    }
    Ok(Vec::new())
}

fn path_matches_filters(root: &Path, path: &Path, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    filters.iter().any(|filter| relative.contains(filter))
}

fn collect_sources(
    root: &Path,
    include_tests: bool,
    filters: &[String],
) -> Result<Vec<(String, Vec<Token>)>, Error> {
    let active = scope::discover(root, include_tests, &[]).map_err(Error::Argument)?;
    let mut queue: VecDeque<_> = active
        .into_iter()
        .map(|file| (file.path, file.cfg))
        .collect();
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    while let Some((path, cfg)) = queue.pop_front() {
        let canonical = path.canonicalize()?;
        if !seen.insert(canonical.clone()) {
            continue;
        }
        for included in included_paths(&path, &cfg)? {
            if included.is_file() {
                queue.push_back((included, cfg.clone()));
            }
        }
        if path_matches_filters(root, &canonical, filters) {
            let name = canonical
                .strip_prefix(root)
                .unwrap_or(&canonical)
                .to_string_lossy()
                .replace('\\', "/");
            output.push((name, normalized_source_tokens(&canonical, &cfg)?));
        }
    }
    output.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(output)
}

"###;
    replace_between(
        &mut source,
        "fn normalized_active_tokens(",
        "fn window_hash(",
        source_processing,
        "source and include processing",
    );

    replace_once(
        &mut source,
        "    let files = scope::discover(root, include_tests, filters).map_err(Error::Argument)?;\n    let mut names = Vec::new();\n    let mut token_sets = Vec::new();\n    for active in files {\n        names.push(\n            active\n                .path\n                .strip_prefix(root)\n                .unwrap_or(&active.path)\n                .to_string_lossy()\n                .replace('\\\\', \"/\"),\n        );\n        token_sets.push(normalized_active_tokens(&active)?);\n    }",
        "    let sources = collect_sources(root, include_tests, filters)?;\n    let mut names = Vec::with_capacity(sources.len());\n    let mut token_sets = Vec::with_capacity(sources.len());\n    for (name, tokens) in sources {\n        names.push(name);\n        token_sets.push(tokens);\n    }",
        "Cargo-aware source collection",
    );

    let tests = r###"    #[test]
    fn cfg_disabled_function_and_generic_parameters_are_removed() {
        let dir = project("dry-parameter-cfg-fixture");
        fs::write(
            dir.path().join("src/lib.rs"),
            "mod baseline; mod configured;\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("src/baseline.rs"),
            "pub fn convert<T,>(value: T,) -> T { value }\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("src/configured.rs"),
            "pub fn convert<T, #[cfg(any())] U>(value: T, #[cfg(any())] hidden: U) -> T { value }\n",
        )
        .unwrap();
        let files = scope::discover(dir.path(), false, &[]).unwrap();
        let values = |name: &str| {
            let file = files
                .iter()
                .find(|file| file.path.file_name().and_then(|value| value.to_str()) == Some(name))
                .unwrap();
            normalized_source_tokens(&file.path, &file.cfg)
                .unwrap()
                .into_iter()
                .map(|token| token.value)
                .collect::<Vec<_>>()
        };
        assert_eq!(values("baseline.rs"), values("configured.rs"));
    }

    #[test]
    fn expression_position_includes_participate_in_duplicate_detection() {
        let dir = project("dry-expression-include-fixture");
        fs::write(
            dir.path().join("src/lib.rs"),
            "pub const A: i32 = include!(\"a_expr.rs\"); pub const B: i32 = include!(\"b_expr.rs\");\n",
        )
        .unwrap();
        let body = "{ let first = 1; let second = first + 2; if second > 3 { second * 4 } else { second - 5 } }\n";
        fs::write(dir.path().join("src/a_expr.rs"), body).unwrap();
        fs::write(dir.path().join("src/b_expr.rs"), body).unwrap();
        let duplicates = find_duplicates(dir.path(), 12, 20, 100, false, &[]).unwrap();
        assert!(duplicates.iter().any(|group| {
            group.locations.iter().any(|location| location.file.ends_with("a_expr.rs"))
                && group
                    .locations
                    .iter()
                    .any(|location| location.file.ends_with("b_expr.rs"))
        }));
    }

    #[test]
    fn impl_member_includes_participate_in_duplicate_detection() {
        let dir = project("dry-impl-include-fixture");
        fs::write(
            dir.path().join("src/lib.rs"),
            "pub struct A; impl A { include!(\"a_impl.rs\"); } pub struct B; impl B { include!(\"b_impl.rs\"); }\n",
        )
        .unwrap();
        let body = "pub fn compute(&self, input: i32) -> i32 { let adjusted = input + 1; if adjusted > 3 { adjusted * 4 } else { adjusted - 5 } }\n";
        fs::write(dir.path().join("src/a_impl.rs"), body).unwrap();
        fs::write(dir.path().join("src/b_impl.rs"), body).unwrap();
        let duplicates = find_duplicates(dir.path(), 12, 20, 100, false, &[]).unwrap();
        assert!(duplicates.iter().any(|group| {
            group.locations.iter().any(|location| location.file.ends_with("a_impl.rs"))
                && group
                    .locations
                    .iter()
                    .any(|location| location.file.ends_with("b_impl.rs"))
        }));
    }

"###;
    insert_before(
        &mut source,
        "    #[test]\n    fn inactive_inline_cfg_items_do_not_create_duplicates()",
        tests,
        "DRY include and parameter regression tests",
    );

    fs::write("src/scoped_lib.rs", source).unwrap();
}
