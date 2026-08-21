from __future__ import annotations

import hashlib
import os
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Iterator, Sequence

from tree_sitter_language_pack import get_parser

LANGUAGE = 'rust'
PARSER_BY_EXTENSION = {}
DISPLAY_LANGUAGE = 'Rust'
EXTENSIONS = ('.rs',)
EXCLUDED_DIRS = frozenset(('.git', '.hg', '.idea', '.pytest_cache', '.tox', '.venv', '.build', 'build', 'coverage', 'dist', 'node_modules', 'target', 'vendor', 'venv', 'DerivedData', 'Pods'))
TEST_DIRS = frozenset(('tests',))
TEST_SUFFIXES = ('_test.rs',)
COMMENT_TYPES = frozenset({"comment", "line_comment", "block_comment"})
STRING_TYPES = frozenset({"string_literal", "raw_string_literal", "char_literal", "string", "heredoc_body"})
NUMBER_TYPES = frozenset({"number_literal", "integer_literal", "float_literal", "integer", "float", "number"})
IDENTIFIER_TYPES = frozenset({
    "identifier", "field_identifier", "type_identifier", "simple_identifier", "word",
    "property_identifier", "function_identifier", "namespace_identifier",
})


class DryError(RuntimeError):
    pass


@dataclass(frozen=True)
class Token:
    value: str
    line: int
    index: int


@dataclass(frozen=True)
class Location:
    file: str
    start_line: int
    end_line: int
    start_token: int
    end_token: int


@dataclass(frozen=True)
class Duplicate:
    token_count: int
    locations: tuple[Location, ...]

    def to_dict(self) -> dict[str, object]:
        return {"token_count": self.token_count, "locations": [asdict(location) for location in self.locations]}


def _is_test_path(relative: str) -> bool:
    path = Path(relative)
    if any(part in TEST_DIRS or part.lower() in {value.lower() for value in TEST_DIRS} for part in path.parts):
        return True
    return path.name.endswith(TEST_SUFFIXES)


def discover_files(root: Path, filters: Sequence[str] = (), include_tests: bool = False) -> list[Path]:
    output: list[Path] = []
    for directory, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(name for name in dirnames if name not in EXCLUDED_DIRS)
        for filename in sorted(filenames):
            if not filename.endswith(EXTENSIONS):
                continue
            path = Path(directory, filename)
            relative = path.relative_to(root).as_posix()
            if not include_tests and _is_test_path(relative):
                continue
            if filters and not any(fragment in relative for fragment in filters):
                continue
            output.append(path)
    return output


def _walk_leaves(node: Any) -> Iterator[Any]:
    if not node.children:
        yield node
        return
    for child in node.children:
        yield from _walk_leaves(child)


def _line(node: Any) -> int:
    point = node.start_point
    if hasattr(point, "row"):
        return int(point.row) + 1
    return int(point[0]) + 1


def _text(node: Any, source: bytes) -> str:
    return source[node.start_byte : node.end_byte].decode("utf-8", errors="replace")


def parser_for_path(path: Path) -> Any:
    return get_parser(PARSER_BY_EXTENSION.get(path.suffix.lower(), LANGUAGE))


def normalized_tokens(path: Path) -> list[Token]:
    source = path.read_bytes()
    tree = parser_for_path(path).parse(source)
    if tree.root_node.has_error:
        raise DryError(f"source contains parse errors: {path}")
    output: list[Token] = []
    for node in _walk_leaves(tree.root_node):
        if node.type in COMMENT_TYPES:
            continue
        value = _text(node, source)
        ancestors: set[str] = set()
        parent = getattr(node, "parent", None)
        while parent is not None:
            ancestors.add(parent.type)
            parent = getattr(parent, "parent", None)
        if node.type in STRING_TYPES or ancestors & STRING_TYPES:
            normalized = "STR"
        elif node.type in NUMBER_TYPES or ancestors & NUMBER_TYPES:
            normalized = "NUM"
        elif node.type in IDENTIFIER_TYPES:
            normalized = "ID"
        else:
            normalized = value
        if normalized.strip():
            output.append(Token(normalized, _line(node), len(output)))
    return output


def _hash_window(tokens: Sequence[Token], start: int, size: int) -> str:
    digest = hashlib.sha256()
    for token in tokens[start : start + size]:
        digest.update(token.value.encode("utf-8"))
        digest.update(b"\0")
    return digest.hexdigest()


def _overlap(first: Location, second: Location) -> bool:
    return first.file == second.file and not (first.end_token <= second.start_token or second.end_token <= first.start_token)


def _extend(a: Sequence[Token], ai: int, b: Sequence[Token], bi: int, minimum: int) -> tuple[int, int, int]:
    left = 0
    while ai - left - 1 >= 0 and bi - left - 1 >= 0 and a[ai - left - 1].value == b[bi - left - 1].value:
        left += 1
    right = minimum
    while ai + right < len(a) and bi + right < len(b) and a[ai + right].value == b[bi + right].value:
        right += 1
    return ai - left, bi - left, right + left


def find_duplicates(
    root: Path,
    min_tokens: int = 30,
    filters: Sequence[str] = (),
    max_groups: int = 50,
    include_tests: bool = False,
    max_occurrences_per_window: int = 100,
) -> list[Duplicate]:
    if min_tokens < 4:
        raise ValueError("min_tokens must be at least 4")
    files = discover_files(root, filters, include_tests)
    token_map = {path.relative_to(root).as_posix(): normalized_tokens(path) for path in files}
    windows: dict[str, list[tuple[str, int]]] = {}
    for filename, tokens in token_map.items():
        for start in range(0, len(tokens) - min_tokens + 1):
            digest = _hash_window(tokens, start, min_tokens)
            values = windows.setdefault(digest, [])
            if len(values) < max_occurrences_per_window:
                values.append((filename, start))

    pairs: dict[tuple[str, int, str, int, int], Duplicate] = {}
    for occurrences in windows.values():
        if len(occurrences) < 2:
            continue
        for first_index in range(len(occurrences)):
            first_file, first_start = occurrences[first_index]
            for second_file, second_start in occurrences[first_index + 1 :]:
                first_tokens = token_map[first_file]
                second_tokens = token_map[second_file]
                a_start, b_start, size = _extend(first_tokens, first_start, second_tokens, second_start, min_tokens)
                first = Location(first_file, first_tokens[a_start].line, first_tokens[a_start + size - 1].line, a_start, a_start + size)
                second = Location(second_file, second_tokens[b_start].line, second_tokens[b_start + size - 1].line, b_start, b_start + size)
                if _overlap(first, second):
                    continue
                ordered = sorted((first, second), key=lambda item: (item.file, item.start_token, item.end_token))
                key = (ordered[0].file, ordered[0].start_token, ordered[1].file, ordered[1].start_token, size)
                pairs[key] = Duplicate(size, tuple(ordered))

    candidates = sorted(pairs.values(), key=lambda item: (-item.token_count, item.locations[0].file, item.locations[0].start_token,
                                                           item.locations[1].file, item.locations[1].start_token))
    selected: list[Duplicate] = []
    for candidate in candidates:
        first, second = candidate.locations[:2]
        contained = False
        for existing in selected:
            old_first, old_second = existing.locations[:2]
            if (first.file, second.file) == (old_first.file, old_second.file) and                        old_first.start_token <= first.start_token and first.end_token <= old_first.end_token and                        old_second.start_token <= second.start_token and second.end_token <= old_second.end_token:
                contained = True
                break
        if contained:
            continue
        selected.append(candidate)
        if len(selected) >= max_groups:
            break
    return selected
