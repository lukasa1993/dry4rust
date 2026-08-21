from pathlib import Path

from dry4rust.core import find_duplicates


FIRST = "pub fn choose(a: bool, b: bool) -> i32 {\n    if a && b { 1 } else { 0 }\n}\n"
SECOND = "pub fn decide(a: bool, b: bool) -> i32 {\n    if a && b { 1 } else { 0 }\n}\n"


def test_cross_file_duplicate_is_found(tmp_path: Path) -> None:
    (tmp_path / "a.rs").write_text(FIRST, encoding="utf-8")
    (tmp_path / "b.rs").write_text(SECOND, encoding="utf-8")
    duplicates = find_duplicates(tmp_path, min_tokens=8)
    assert duplicates


def test_non_overlapping_same_file_duplicate_is_found(tmp_path: Path) -> None:
    path = tmp_path / "sample.rs"
    path.write_text(FIRST + "\n" + SECOND, encoding="utf-8")
    duplicates = find_duplicates(tmp_path, min_tokens=8)
    assert any(item.locations[0].file == item.locations[1].file for item in duplicates)
