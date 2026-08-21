from pathlib import Path

from dry4rust.core import find_duplicates


def test_cross_file_duplicate_is_found(tmp_path: Path) -> None:
    first = tmp_path / ("a_" + 'sample.rs')
    second = tmp_path / ("b_" + 'sample.rs')
    first.write_text('pub fn choose(a: bool, b: bool) -> i32 {\n    if a && b { 1 } else { 0 }\n}\n', encoding="utf-8")
    second.write_text('pub fn decide(a: bool, b: bool) -> i32 {\n    if a && b { 1 } else { 0 }\n}\n', encoding="utf-8")
    duplicates = find_duplicates(tmp_path, min_tokens=8)
    assert duplicates


def test_non_overlapping_same_file_duplicate_is_found(tmp_path: Path) -> None:
    path = tmp_path / 'sample.rs'
    path.write_text('pub fn choose(a: bool, b: bool) -> i32 {\n    if a && b { 1 } else { 0 }\n}\n' + "\n" + 'pub fn decide(a: bool, b: bool) -> i32 {\n    if a && b { 1 } else { 0 }\n}\n', encoding="utf-8")
    duplicates = find_duplicates(tmp_path, min_tokens=8)
    assert any(item.locations[0].file == item.locations[1].file for item in duplicates)
