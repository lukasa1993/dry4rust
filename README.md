# dry4rust

Native Rust duplicate-code analysis. The executable validates Rust with `syn`, tokenizes it with the Rust lexer, normalizes identifiers and literals, detects cross-file and non-overlapping same-file duplication, extends maximal blocks, suppresses contained matches, and verifies token equality after hash bucketing.

## Requirements

- Rust 1.82 or later.
- No Python, Node, JVM, or other language runtime.

## Install

```bash
cargo install --git https://github.com/lukasa1993/dry4rust --locked --force
```

The repository commits `Cargo.lock`. Use `--locked` so installation uses the dependency graph tested by CI.

## Run

```bash
dry4rust --min-tokens 30 --fail
```

Exit status: `0` pass, `1` parse/analysis error, `2` duplicates found when `--fail` is active.
