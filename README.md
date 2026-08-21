# dry4rust

`dry4rust` finds duplicated normalized token blocks in Rust source files.

## Install

```bash
pipx install git+https://github.com/lukasa1993/dry4rust.git
```

## Run

```bash
dry4rust --min-tokens 30 --fail
```

Identifiers and numeric literals are normalized. Comments and string contents do not affect matching. Use positional path fragments to limit the scan. Use `--json` for machine-readable output.

Exit status `2` means that duplication was found while `--fail` was active.

## Development

```bash
python -m pip install -e . pytest
pytest -q
```
