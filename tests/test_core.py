from pathlib import Path

from dry4rust.core import find_duplicates, tokenize_file


def test_normalizes_identifiers_and_numbers(tmp_path: Path) -> None:
    path = tmp_path / ("sample" + '.rs')
    path.write_text('fn a(x: i32) -> i32 { if x > 0 { return x + 1; } x }\n', encoding="utf-8")
    values = [token.value for token in tokenize_file(path)]
    assert "ID" in values
    assert "NUM" in values


def test_finds_duplicate_blocks(tmp_path: Path) -> None:
    (tmp_path / ("a" + '.rs')).write_text('fn a(x: i32) -> i32 { if x > 0 { return x + 1; } x }\n', encoding="utf-8")
    (tmp_path / ("b" + '.rs')).write_text('fn b(y: i32) -> i32 { if y > 0 { return y + 2; } y }\n', encoding="utf-8")
    duplicates = find_duplicates(tmp_path, min_tokens=8)
    assert duplicates
    assert len(duplicates[0].locations) >= 2
