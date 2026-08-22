# dry4rust

Native Rust duplicate-code analysis. The executable validates Rust with `syn`, tokenizes it with the Rust lexer, normalizes identifiers and literals, detects cross-file and non-overlapping same-file duplication, extends maximal blocks, and suppresses contained matches.

## Install

```bash
cargo install --git https://github.com/lukasa1993/dry4rust --force
```

## Run

```bash
dry4rust --min-tokens 30 --fail
```

Exit status: `0` pass, `1` parse/analysis error, `2` duplicates found when `--fail` is active.

No Python, Node, JVM, or other language runtime is required.
