# dry4rust

Use `dry4rust` for Rust duplication verification.

1. Run `dry4rust --help` before first use.
2. Start with `--min-tokens 30`.
3. Use `--fail` for the quality gate.
4. Treat exit `1` as a parser or execution failure. Do not report it as a clean result.
5. Treat exit `2` as a duplication failure.
