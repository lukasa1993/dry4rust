use std::fs;

fn replace_once(text: &mut String, old: &str, new: &str, label: &str) {
    let start = text
        .find(old)
        .unwrap_or_else(|| panic!("missing patch anchor: {}", label));
    assert!(
        text[start + old.len()..].find(old).is_none(),
        "duplicate patch anchor: {}",
        label
    );
    text.replace_range(start..start + old.len(), new);
}

fn main() {
    let mut cargo = fs::read_to_string("Cargo.toml").unwrap();
    replace_once(
        &mut cargo,
        "version = \"2.0.1\"",
        "version = \"2.0.2\"",
        "package version",
    );
    fs::write("Cargo.toml", cargo).unwrap();

    let mut source = fs::read_to_string("src/lib.rs").unwrap();
    replace_once(
        &mut source,
        "        \"as\" | \"break\"",
        "        \"_\" | \"as\" | \"break\"",
        "underscore keyword",
    );
    replace_once(
        &mut source,
        "            | \"final\"\n            | \"macro\"",
        "            | \"final\"\n            | \"gen\"\n            | \"macro\"\n            | \"macro_rules\"",
        "Rust 2024 and macro_rules keywords",
    );
    replace_once(
        &mut source,
        "            | \"priv\"\n            | \"typeof\"",
        "            | \"priv\"\n            | \"raw\"\n            | \"safe\"\n            | \"typeof\"\n            | \"union\"",
        "weak Rust keywords",
    );
    let anchor = "    #[test]\n    fn token_normalization_ignores_comments_and_values() {";
    let test = r###"    #[test]
    fn current_rust_keywords_are_preserved_by_normalization() {
        for keyword in ["_", "gen", "macro_rules", "raw", "safe", "union"] {
            assert!(is_keyword(keyword), "{keyword} must be treated as Rust syntax");
        }
        assert!(!is_keyword("ordinary_identifier"));
    }

"###;
    let index = source.find(anchor).expect("missing keyword regression-test anchor");
    source.insert_str(index, test);
    fs::write("src/lib.rs", source).unwrap();
}
