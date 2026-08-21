from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from . import __version__
from .core import DryError, find_duplicates


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description="Find normalized duplicate code in Rust projects.")
    value.add_argument("filters", nargs="*")
    value.add_argument("--root", type=Path, default=Path("."))
    value.add_argument("--min-tokens", type=int, default=30)
    value.add_argument("--max-groups", type=int, default=50)
    value.add_argument("--max-occurrences-per-window", type=int, default=100)
    value.add_argument("--include-tests", action="store_true")
    value.add_argument("--json", action="store_true", dest="json_output")
    value.add_argument("--fail", action="store_true")
    value.add_argument("--version", action="version", version=f"%(prog)s {__version__}")
    return value


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    root = args.root.resolve()
    try:
        duplicates = find_duplicates(
            root,
            args.min_tokens,
            args.filters,
            args.max_groups,
            args.include_tests,
            args.max_occurrences_per_window,
        )
    except (OSError, ValueError, DryError) as error:
        print(f"dry4rust: {error}", file=sys.stderr)
        return 1
    payload = {
        "schema_version": 1,
        "tool": "dry4rust",
        "version": __version__,
        "root": root.as_posix(),
        "summary": {"groups": len(duplicates), "min_tokens": args.min_tokens},
        "duplicates": [duplicate.to_dict() for duplicate in duplicates],
    }
    if args.json_output:
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        print("DRY Report")
        print("==========")
        if not duplicates:
            print("No duplicate groups found.")
        for duplicate in duplicates:
            locations = " <-> ".join(f"{item.file}:{item.start_line}-{item.end_line}" for item in duplicate.locations)
            print(f"{duplicate.token_count} tokens: {locations}")
    return 2 if args.fail and duplicates else 0


if __name__ == "__main__":
    raise SystemExit(main())
