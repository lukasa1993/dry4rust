# dry4rust

`dry4rust` finds normalized duplicate Rust code with a Tree-sitter syntax tree. It reports maximal non-overlapping blocks, including duplicate blocks in one file.

```bash
pipx install git+https://github.com/lukasa1993/dry4rust.git
dry4rust --min-tokens 30 --fail
```

Exit status: `0` pass, `1` parse or execution error, `2` duplicate groups found when `--fail` is active.

## Development

```bash
python -m pip install -e . pytest
pytest -q
```
