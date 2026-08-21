# dry4rust

`dry4rust` finds normalized duplicate code in Rust projects with Tree-sitter tokens. It reports cross-file and non-overlapping same-file duplicates, extends matching windows to maximal blocks, and suppresses contained results.

```bash
pipx install git+https://github.com/lukasa1993/dry4rust.git
dry4rust --min-tokens 30 --fail
```

Exit status: `0` pass, `1` analysis error, `2` duplicates found when `--fail` is active.

## Development

```bash
python -m pip install -e . pytest
pytest -q
```
