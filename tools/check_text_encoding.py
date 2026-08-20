#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


DEFAULT_ROOTS = [
    Path("crates/frontend/src/domain/a027_wb_documents"),
    Path("crates/backend/src/api/handlers/a027_wb_documents.rs"),
]

TEXT_EXTENSIONS = {
    ".rs",
    ".sql",
    ".md",
    ".toml",
    ".html",
    ".css",
    ".js",
    ".ts",
    ".json",
    ".yml",
    ".yaml",
    ".py",
    ".ps1",
}

SKIP_DIRS = {
    ".git",
    "target",
    "node_modules",
}

# Typical bytes-decoded-as-Windows-1251 artifacts in Russian text:
# U+0402/U+0452, U+0403/U+0453, U+201A/U+201E, U+2020/U+2021,
# U+2022/U+2026, U+20AC, NBSP/C2 residue, replacement char.
MOJIBAKE_CODEPOINTS = {
    0x00A0,
    0x00C2,
    0x0402,
    0x0403,
    0x0405,
    0x0406,
    0x0408,
    0x0409,
    0x040A,
    0x040B,
    0x040E,
    0x0452,
    0x0453,
    0x0454,
    0x0455,
    0x0456,
    0x0458,
    0x0459,
    0x045A,
    0x045B,
    0x045E,
    0x0491,
    0x201A,
    0x201C,
    0x201D,
    0x201E,
    0x2020,
    0x2021,
    0x2022,
    0x2026,
    0x2030,
    0x2039,
    0x203A,
    0x20AC,
    0xFFFD,
}

STRING_RE = re.compile(r'"([^"\\]*(?:\\.[^"\\]*)*)"')


def iter_files(root: Path) -> list[Path]:
    if root.is_file():
        return [root] if root.suffix.lower() in TEXT_EXTENSIONS else []
    if not root.exists():
        return []

    result: list[Path] = []
    for path in root.rglob("*"):
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        if path.is_file() and path.suffix.lower() in TEXT_EXTENSIONS:
            result.append(path)
    return result


def suspicious_literal(literal: str) -> bool:
    if "???" in literal:
        return True
    return any(ord(ch) in MOJIBAKE_CODEPOINTS or 0x80 <= ord(ch) <= 0x9F for ch in literal)


def check_file(path: Path) -> list[str]:
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError as exc:
        return [f"{path}: not valid UTF-8: {exc}"]

    errors: list[str] = []
    for match in STRING_RE.finditer(text):
        literal = match.group(1)
        if suspicious_literal(literal):
            line = text.count("\n", 0, match.start()) + 1
            preview = literal.encode("unicode_escape").decode("ascii")[:180]
            errors.append(f"{path}:{line}: suspicious text literal: {preview}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("paths", nargs="*", type=Path, default=DEFAULT_ROOTS)
    args = parser.parse_args()

    files: list[Path] = []
    for root in args.paths:
        files.extend(iter_files(root))

    errors: list[str] = []
    for path in sorted(set(files)):
        errors.extend(check_file(path))

    if errors:
        print("\n".join(errors))
        return 1

    print(f"encoding check passed ({len(set(files))} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
