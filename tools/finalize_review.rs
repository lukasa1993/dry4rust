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
    let mut scope = fs::read_to_string("src/scope.rs").unwrap();
    if !scope.contains("fn include_literal(") {
        replace_once(
            &mut scope,
            "use syn::parse::Parser;",
            "use syn::parse::{ParseStream, Parser};",
            "ParseStream import",
        );
        let include_code = r###"fn include_literal(tokens: TokenStream) -> Option<LitStr> {
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

fn static_include_path(item: &syn::ItemMacro, source_dir: &Path) -> Option<PathBuf> {
    if !item.mac.path.is_ident("include") {
        return None;
    }
    let literal = include_literal(item.mac.tokens.clone())?;
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

"###;
        replace_between(
            &mut scope,
            "fn static_include_path(",
            "fn walk_items(",
            include_code,
            "static include parser",
        );
        replace_once(
            &mut scope,
            "fn static_include_is_part_of_active_source_graph()",
            "fn static_include_with_trailing_comma_is_part_of_active_source_graph()",
            "include test name",
        );
        replace_once(
            &mut scope,
            r###""include!(\"shared.rs\");\n""###,
            r###""include!(\"shared.rs\",);\n""###,
            "trailing-comma include fixture",
        );
    }
    fs::write("src/scope.rs", scope).unwrap();

    let mut library = fs::read_to_string("src/scoped_lib.rs").unwrap();
    if !library.contains("fn visit_field_value(") {
        let field_value = r###"    fn visit_field_value(&mut self, node: &'ast syn::FieldValue) {
        if self.inactive(&node.attrs, node) {
            return;
        }
        visit::visit_field_value(self, node);
    }

"###;
        let marker = "    fn visit_variant(&mut self, node: &'ast syn::Variant) {";
        let index = library.find(marker).expect("missing variant visitor");
        library.insert_str(index, field_value);
    }

    if library.contains("fn cfg_disabled_match_arms_do_not_create_duplicates()") {
        let tests = r###"    #[test]
    fn cfg_disabled_match_arms_do_not_change_active_tokens() {
        let dir = project("dry-arm-cfg-fixture");
        fs::write(
            dir.path().join("src/lib.rs"),
            "mod baseline; mod with_disabled;\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("src/baseline.rs"),
            "pub fn choose(x: i32) -> i32 { match x { _ => 0 } }\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("src/with_disabled.rs"),
            "pub fn choose(x: i32) -> i32 { match x { #[cfg(any())] 1 => { let a=1; let b=a+2; b*3 }, _ => 0 } }\n",
        )
        .unwrap();
        let files = scope::discover(dir.path(), false, &[]).unwrap();
        let values = |name: &str| {
            let file = files
                .iter()
                .find(|file| file.path.file_name().and_then(|value| value.to_str()) == Some(name))
                .unwrap();
            normalized_active_tokens(file)
                .unwrap()
                .into_iter()
                .map(|token| token.value)
                .collect::<Vec<_>>()
        };
        assert_eq!(values("baseline.rs"), values("with_disabled.rs"));
    }

    #[test]
    fn cfg_disabled_struct_expression_fields_do_not_change_active_tokens() {
        let dir = project("dry-field-value-cfg-fixture");
        fs::write(
            dir.path().join("src/lib.rs"),
            "mod baseline; mod with_disabled;\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("src/baseline.rs"),
            "pub struct S { pub a: i32 } pub fn make() -> S { S { a: 0 } }\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("src/with_disabled.rs"),
            "pub struct S { pub a: i32, #[cfg(any())] pub b: i32 } pub fn make() -> S { S { a: 0, #[cfg(any())] b: { let x=1; let y=x+2; y*3 } } }\n",
        )
        .unwrap();
        let files = scope::discover(dir.path(), false, &[]).unwrap();
        let values = |name: &str| {
            let file = files
                .iter()
                .find(|file| file.path.file_name().and_then(|value| value.to_str()) == Some(name))
                .unwrap();
            normalized_active_tokens(file)
                .unwrap()
                .into_iter()
                .map(|token| token.value)
                .collect::<Vec<_>>()
        };
        assert_eq!(values("baseline.rs"), values("with_disabled.rs"));
    }

"###;
        replace_between(
            &mut library,
            "    #[test]\n    fn cfg_disabled_match_arms_do_not_create_duplicates()",
            "    #[test]\n    fn active_static_include_participates_in_duplicate_detection()",
            tests,
            "cfg-disabled DRY tests",
        );
    }
    fs::write("src/scoped_lib.rs", library).unwrap();
}
