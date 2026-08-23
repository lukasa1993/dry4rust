use std::fs;

fn replace_once(text: &mut String, old: &str, new: &str, label: &str) {
    let start = text
        .find(old)
        .unwrap_or_else(|| panic!("missing anchor: {label}"));
    assert!(
        text[start + old.len()..].find(old).is_none(),
        "duplicate anchor: {label}"
    );
    text.replace_range(start..start + old.len(), new);
}

fn replace_between(text: &mut String, start: &str, end: &str, new: &str, label: &str) {
    let from = text
        .find(start)
        .unwrap_or_else(|| panic!("missing start anchor: {label}"));
    let relative = text[from..]
        .find(end)
        .unwrap_or_else(|| panic!("missing end anchor: {label}"));
    text.replace_range(from..from + relative, new);
}

fn main() {
    let mut library = fs::read_to_string("src/scoped_lib.rs").unwrap();
    if library.contains("fn expression_includes_from_file(") {
        return;
    }

    replace_once(
        &mut library,
        "use rustc_lexer::{LiteralKind, TokenKind};",
        "use proc_macro2::TokenStream;\nuse rustc_lexer::{LiteralKind, TokenKind};",
        "proc-macro token stream import",
    );
    replace_once(
        &mut library,
        "use std::collections::{hash_map::DefaultHasher, HashMap};",
        "use std::collections::{hash_map::DefaultHasher, HashMap, HashSet, VecDeque};",
        "collection imports",
    );
    replace_once(
        &mut library,
        "use std::path::Path;",
        "use std::path::{Path, PathBuf};",
        "path imports",
    );
    replace_once(
        &mut library,
        "use syn::spanned::Spanned;",
        "use syn::parse::{ParseStream, Parser};\nuse syn::spanned::Spanned;",
        "syn parser imports",
    );
    replace_once(
        &mut library,
        "use syn::{Attribute, Expr, ForeignItem, ImplItem, Item, TraitItem};",
        "use syn::{Attribute, Expr, ForeignItem, ImplItem, Item, LitStr, TraitItem};",
        "syn AST imports",
    );

    let inactive_expr = r###"fn inactive_expr_ranges(expr: &Expr, cfg: &scope::CfgContext) -> Vec<Range<usize>> {
    let mut visitor = InactiveRangeVisitor {
        cfg,
        ranges: Vec::new(),
    };
    visitor.visit_expr(expr);
    visitor.ranges.sort_by_key(|range| (range.start, range.end));
    visitor.ranges
}

"###;
    let marker = "fn is_keyword(text: &str) -> bool {";
    let index = library.find(marker).expect("missing keyword marker");
    library.insert_str(index, inactive_expr);

    let token_code = r###"fn normalized_tokens_from_source(source: &str, excluded_ranges: &[Range<usize>]) -> Vec<Token> {
    let shebang = rustc_lexer::strip_shebang(source).unwrap_or(0);
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
    output
}

fn normalized_active_tokens(active: &scope::ActiveFile) -> Result<Vec<Token>, Error> {
    let source = fs::read_to_string(&active.path)?;
    let syntax = syn::parse_file(&source).map_err(|source_error| Error::Parse {
        path: active.path.clone(),
        source: source_error,
    })?;
    let excluded_ranges = inactive_ranges(&syntax, &active.cfg);
    Ok(normalized_tokens_from_source(&source, &excluded_ranges))
}

fn include_literal(tokens: TokenStream) -> Option<LitStr> {
    let parser = |input: ParseStream<'_>| {
        let literal: LitStr = input.parse()?;
        if input.peek(syn::Token![,]) {
            input.parse::<syn::Token![,]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("include! expects one string literal"));
        }
        Ok(literal)
    };
    parser.parse2(tokens).ok()
}

fn expression_include_path(node: &syn::ExprMacro, source_dir: &Path) -> Option<PathBuf> {
    if !node.mac.path.is_ident("include") {
        return None;
    }
    let literal = include_literal(node.mac.tokens.clone())?;
    let path = PathBuf::from(literal.value());
    if path.extension().and_then(|value| value.to_str()) != Some("rs") {
        return None;
    }
    Some(if path.is_absolute() {
        path
    } else {
        source_dir.join(path)
    })
}

struct ExpressionIncludeVisitor<'a> {
    source_dir: &'a Path,
    excluded_ranges: &'a [Range<usize>],
    paths: Vec<PathBuf>,
}

impl<'ast> Visit<'ast> for ExpressionIncludeVisitor<'_> {
    fn visit_expr_macro(&mut self, node: &'ast syn::ExprMacro) {
        let range = node.span().byte_range();
        let excluded = self
            .excluded_ranges
            .iter()
            .any(|item| item.start <= range.start && range.end <= item.end);
        if !excluded {
            if let Some(path) = expression_include_path(node, self.source_dir) {
                self.paths.push(path);
            }
            visit::visit_expr_macro(self, node);
        }
    }
}

fn expression_includes_from_file(
    file: &syn::File,
    path: &Path,
    cfg: &scope::CfgContext,
) -> Vec<PathBuf> {
    let ranges = inactive_ranges(file, cfg);
    let source_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut visitor = ExpressionIncludeVisitor {
        source_dir,
        excluded_ranges: &ranges,
        paths: Vec::new(),
    };
    visitor.visit_file(file);
    visitor.paths
}

fn expression_includes_from_expr(
    expr: &Expr,
    path: &Path,
    cfg: &scope::CfgContext,
) -> Vec<PathBuf> {
    let ranges = inactive_expr_ranges(expr, cfg);
    let source_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut visitor = ExpressionIncludeVisitor {
        source_dir,
        excluded_ranges: &ranges,
        paths: Vec::new(),
    };
    visitor.visit_expr(expr);
    visitor.paths
}

fn normalized_expression_tokens(
    path: &Path,
    cfg: &scope::CfgContext,
) -> Result<(Vec<Token>, Vec<PathBuf>), Error> {
    let source = fs::read_to_string(path)?;
    let syntax = syn::parse_str::<Expr>(&source).map_err(|source_error| Error::Parse {
        path: path.to_path_buf(),
        source: source_error,
    })?;
    let excluded_ranges = inactive_expr_ranges(&syntax, cfg);
    let tokens = normalized_tokens_from_source(&source, &excluded_ranges);
    let includes = expression_includes_from_expr(&syntax, path, cfg);
    Ok((tokens, includes))
}

"###;
    replace_between(
        &mut library,
        "fn normalized_active_tokens(",
        "fn window_hash(",
        token_code,
        "active and included token normalization",
    );

    let discovery = r###"    let files = scope::discover(root, include_tests, filters).map_err(Error::Argument)?;
    let mut names = Vec::new();
    let mut token_sets = Vec::new();
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    for active in files {
        let source = fs::read_to_string(&active.path)?;
        let syntax = syn::parse_file(&source).map_err(|source_error| Error::Parse {
            path: active.path.clone(),
            source: source_error,
        })?;
        if let Ok(canonical) = active.path.canonicalize() {
            visited.insert(canonical);
        }
        for path in expression_includes_from_file(&syntax, &active.path, &active.cfg) {
            queue.push_back((path, active.cfg.clone()));
        }
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
    while let Some((path, cfg)) = queue.pop_front() {
        let canonical = path.canonicalize()?;
        if !visited.insert(canonical.clone()) {
            continue;
        }
        let (tokens, nested) = normalized_expression_tokens(&canonical, &cfg)?;
        names.push(
            canonical
                .strip_prefix(root)
                .unwrap_or(&canonical)
                .to_string_lossy()
                .replace('\\', "/"),
        );
        token_sets.push(tokens);
        for path in nested {
            queue.push_back((path, cfg.clone()));
        }
    }
"###;
    replace_between(
        &mut library,
        "    let files = scope::discover(root, include_tests, filters).map_err(Error::Argument)?;",
        "    let mut windows:",
        discovery,
        "expression include discovery",
    );

    let test = r###"    #[test]
    fn expression_position_includes_participate_in_duplicate_detection() {
        let dir = project("dry-expression-include-fixture");
        fs::write(
            dir.path().join("src/lib.rs"),
            "pub const FIRST: i32 = include!(\"first.rs\"); pub const SECOND: i32 = include!(\"second.rs\",);\n",
        )
        .unwrap();
        let body = "{ let a = 1; let b = a + 2; if b > 2 { b * 3 } else { b - 1 } }\n";
        fs::write(dir.path().join("src/first.rs"), body).unwrap();
        fs::write(dir.path().join("src/second.rs"), body).unwrap();
        let duplicates = find_duplicates(dir.path(), 10, 20, 100, false, &[]).unwrap();
        assert!(duplicates.iter().any(|group| {
            group.locations.iter().any(|location| location.file.ends_with("first.rs"))
                && group
                    .locations
                    .iter()
                    .any(|location| location.file.ends_with("second.rs"))
        }));
    }

"###;
    let test_marker = "    #[test]\n    fn active_static_include_participates_in_duplicate_detection()";
    let test_index = library
        .find(test_marker)
        .expect("missing static include test marker");
    library.insert_str(test_index, test);

    fs::write("src/scoped_lib.rs", library).unwrap();
}
