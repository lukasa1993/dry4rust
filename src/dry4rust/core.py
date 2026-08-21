from __future__ import annotations

import os
import re
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Sequence

LANGUAGE = 'rust'
EXTENSIONS = tuple(['.rs'])
EXCLUDED_DIRS = {".git", ".hg", ".idea", ".pytest_cache", ".tox", ".venv", "build", "coverage", "dist", "node_modules", "target", "vendor", "venv", ".build"}

KEYWORDS = {
    "typescript": {"if", "else", "for", "while", "switch", "case", "default", "function", "return", "class", "extends", "implements", "const", "let", "var", "async", "await", "try", "catch", "finally", "throw", "new", "import", "export", "from", "true", "false", "null", "undefined", "type", "interface"},
    "rust": {"if", "else", "for", "while", "loop", "match", "fn", "impl", "trait", "struct", "enum", "mod", "pub", "use", "let", "mut", "return", "async", "await", "move", "where", "true", "false", "self", "Self", "crate"},
    "swift": {"if", "else", "for", "while", "repeat", "switch", "case", "default", "func", "class", "struct", "enum", "actor", "extension", "protocol", "return", "guard", "defer", "do", "catch", "throw", "try", "async", "await", "let", "var", "true", "false", "nil", "self"},
    "objective-c": {"if", "else", "for", "while", "do", "switch", "case", "default", "return", "typedef", "struct", "enum", "static", "const", "void", "int", "long", "float", "double", "char", "BOOL", "YES", "NO", "nil", "self", "super", "interface", "implementation"},
    "bash": {"if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "in", "function", "select", "time", "coproc", "true", "false", "return", "exit", "local", "readonly", "declare"},
}[LANGUAGE]

TOKEN_RE = re.compile(r"[A-Za-z_$][A-Za-z0-9_$]*|0[xX][0-9A-Fa-f]+|\d+(?:\.\d+)?|===|!==|==|!=|<=|>=|&&|\|\||\?\?|=>|::|->|\+=|-=|\*=|/=|[-+*/%<>{}()[\],.;:?=!]|")


@dataclass(frozen=True)
class Token:
    value: str
    line: int


@dataclass(frozen=True)
class Location:
    file: str
    start_line: int
    end_line: int


@dataclass(frozen=True)
class Duplicate:
    token_count: int
    locations: tuple[Location, ...]

    def to_dict(self) -> dict[str, object]:
        return {"token_count": self.token_count, "locations": [asdict(location) for location in self.locations]}


def discover_files(root: Path, filters: Sequence[str] = ()) -> list[Path]:
    files: list[Path] = []
    for directory, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(name for name in dirnames if name not in EXCLUDED_DIRS)
        for filename in sorted(filenames):
            if not filename.endswith(EXTENSIONS):
                continue
            path = Path(directory, filename)
            relative = path.relative_to(root).as_posix()
            if filters and not any(fragment in relative for fragment in filters):
                continue
            files.append(path)
    return files


def mask_non_code(text: str) -> str:
    out = list(text)
    index = 0
    state = "code"
    quote = ""
    while index < len(text):
        char = text[index]
        next_char = text[index + 1] if index + 1 < len(text) else ""
        if state == "code":
            if LANGUAGE == "bash" and char == "#":
                state = "line"
                out[index] = " "
            elif LANGUAGE != "bash" and char == "/" and next_char == "/":
                state = "line"
                out[index] = out[index + 1] = " "
                index += 1
            elif LANGUAGE != "bash" and char == "/" and next_char == "*":
                state = "block"
                out[index] = out[index + 1] = " "
                index += 1
            elif char in {'"', "'", "`"} and (LANGUAGE == "typescript" or char != "`"): 
                state = "string"
                quote = char
                out[index] = " "
            else:
                out[index] = char
        elif state == "line":
            if char == "\n":
                state = "code"
                out[index] = "\n"
            else:
                out[index] = " "
        elif state == "block":
            out[index] = "\n" if char == "\n" else " "
            if char == "*" and next_char == "/":
                out[index + 1] = " "
                index += 1
                state = "code"
        else:
            out[index] = "\n" if char == "\n" else " "
            if char == "\\" and quote != "'" and index + 1 < len(text):
                out[index + 1] = " "
                index += 1
            elif char == quote:
                state = "code"
        index += 1
    return "".join(out)


def tokenize_file(path: Path) -> list[Token]:
    text = path.read_text(encoding="utf-8", errors="replace")
    masked = mask_non_code(text)
    out: list[Token] = []
    for match in TOKEN_RE.finditer(masked):
        value = match.group(0)
        if re.fullmatch(r"[A-Za-z_$][A-Za-z0-9_$]*", value):
            normalized = value if value in KEYWORDS else "ID"
        elif re.fullmatch(r"0[xX][0-9A-Fa-f]+|\d+(?:\.\d+)?", value):
            normalized = "NUM"
        else:
            normalized = value
        out.append(Token(normalized, text.count("\n", 0, match.start()) + 1))
    return out


def find_duplicates(root: Path, min_tokens: int = 30, filters: Sequence[str] = (), max_groups: int = 50) -> list[Duplicate]:
    if min_tokens < 4:
        raise ValueError("min_tokens must be at least 4")
    groups: dict[tuple[str, ...], list[Location]] = {}
    for path in discover_files(root, filters):
        tokens = tokenize_file(path)
        relative = path.relative_to(root).as_posix()
        for start in range(0, max(0, len(tokens) - min_tokens + 1)):
            window = tokens[start : start + min_tokens]
            key = tuple(token.value for token in window)
            groups.setdefault(key, []).append(Location(relative, window[0].line, window[-1].line))

    candidates: list[Duplicate] = []
    for locations in groups.values():
        unique = tuple(dict.fromkeys(locations))
        if len(unique) < 2:
            continue
        candidates.append(Duplicate(min_tokens, unique))
    candidates.sort(key=lambda item: (-len(item.locations), item.locations[0].file, item.locations[0].start_line))

    selected: list[Duplicate] = []
    seen: list[tuple[str, int, str, int]] = []
    for duplicate in candidates:
        first, second = duplicate.locations[:2]
        pair = (first.file, first.start_line, second.file, second.start_line)
        if any(pair[0] == old[0] and pair[2] == old[2] and abs(pair[1] - old[1]) <= 2 and abs(pair[3] - old[3]) <= 2 for old in seen):
            continue
        seen.append(pair)
        selected.append(duplicate)
        if len(selected) >= max_groups:
            break
    return selected
