#!/usr/bin/env python3
"""Ловит текст, испорченный неверной кодировкой, и умеет его чинить.

Что именно ловим. Русский текст, записанный в UTF-8 и прочитанный как CP1251,
превращается в «РєР»РёРµРЅС‚» вместо «клиент»: каждая буква занимает два байта,
и каждый байт читается как отдельный символ. Это не опечатка и не стилистика —
это потеря, которую компилятор не видит, потому что строка остаётся валидной.
(Кавычки вокруг примера выше не украшение: без них проверка нашла бы порчу в
собственной документации и заблокировала коммит этого файла.)

Как отличаем от нормального текста. Прошлая версия этого файла держала список
«подозрительных» кодов и ловила на нём `…`, `«»`, `•` в обычных русских
подписях — 269 находок, из которых настоящими были единицы. Признак теперь
другой и точный: попытка обратного разбора сама является фильтром. Испорченный
прогон раскодируется по построению — ровно этой операцией его и испортили,
только в обратную сторону; нормальная строка почти никогда не образует
валидный UTF-8 и отсеивается. Дальше результат обязан быть вдвое короче
(буква = два байта), содержать кириллицу и не нести ни следов порчи, ни
символов, которых в русском тексте не бывает.

Часть текста испорчена дважды — разбор поэтому повторяется, пока прогон
укорачивается.

Что почти невозможно починить: если в тексте уже есть U+FFFD или C1-мусор,
байты потеряны, и «починка» дала бы просто другой мусор. Такие места
считаются отдельно и перечислены в KNOWN_DAMAGED — это долг с адресом, а не
тишина.

Запуск:
    python tools/check_text_encoding.py                # проверить crates/
    python tools/check_text_encoding.py crates/backend # проверить часть дерева
    python tools/check_text_encoding.py --fix          # починить восстановимое
    python tools/check_text_encoding.py <файлы...>     # так его зовёт pre-commit

Код возврата 1, если найдено восстановимое (то есть свежая порча) или если
неразбираемого стало больше, чем записано в KNOWN_DAMAGED.
"""
from __future__ import annotations

import argparse
import io
import re
import sys
from pathlib import Path

DEFAULT_ROOTS = [Path("crates")]

TEXT_EXTENSIONS = {
    ".rs", ".sql", ".md", ".toml", ".html", ".css", ".js", ".ts",
    ".json", ".yml", ".yaml", ".py", ".ps1",
}

SKIP_DIRS = {".git", "target", "node_modules", "dist"}

# Минифицированный вендорный JS содержит произвольные байты в литералах и к
# нашему тексту отношения не имеет.
SKIP_SUFFIXES = (".min.js",)

NON_ASCII = "[^" + chr(0x00) + "-" + chr(0x7F) + "]"
RUN = re.compile(NON_ASCII + "+")
# Р, С — CP1251-чтение первого байта русской буквы; Ð, Ñ — то же в latin-1.
MANGLED = re.compile("[" + chr(0x420) + chr(0x421) + chr(0xD0) + chr(0xD1) + "]" + NON_ASCII)
# U+FFFD и C1: байты уже потеряны, восстановлению не подлежит.
LOST = re.compile("[" + chr(0xFFFD) + chr(0x80) + "-" + chr(0x9F) + "]")
CYRILLIC = re.compile("[" + chr(0x400) + "-" + chr(0x4FF) + "]")
# Символов из этого набора в русском тексте не бывает, а в недоразобранном
# mojibake они повсюду: их наличие означает, что разбор не закончен.
SUSPECT = set("ЂЃѓ‡‰ЉЊЌЋЏђљњќћџЎўЈҐЅѝѕїјєЄЇІі")

# Места, где байты потеряны до этой проверки и восстановить их нельзя.
# Числа держат долг от роста: стало больше — значит, порча свежая.
KNOWN_DAMAGED = {
    "crates/backend/src/api/handlers/a012_wb_sales.rs": 56,
    "crates/backend/src/domain/a004_nomenclature/excel_import.rs": 3,
    "crates/backend/src/usecases/u504_import_from_wildberries/wildberries_api_client.rs": 20,
    "crates/contracts/src/domain/a004_nomenclature/aggregate.rs": 3,
}


def unmangle(run: str) -> str | None:
    """Раскодировать прогон обратно. None — не порча либо не восстановимо."""
    current = run
    for _ in range(4):
        try:
            nxt = current.encode("cp1251").decode("utf-8")
        except (UnicodeEncodeError, UnicodeDecodeError):
            return None
        # Буква в UTF-8 — два байта, поэтому испорченный текст ровно вдвое
        # длиннее. Это отсекает случайные совпадения лучше запрета на короткие
        # прогоны: «Рё» → «и» — законная правка.
        if len(nxt) * 2 > len(current):
            return None
        current = nxt
        if not MANGLED.search(current):
            clean = (
                CYRILLIC.search(current)
                and not LOST.search(current)
                and not (SUSPECT & set(current))
            )
            return current if clean else None
    return None


def iter_files(root: Path) -> list[Path]:
    if root.is_file():
        return [root] if root.suffix.lower() in TEXT_EXTENSIONS else []
    if not root.exists():
        return []
    out: list[Path] = []
    for path in root.rglob("*"):
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        if not path.is_file() or path.suffix.lower() not in TEXT_EXTENSIONS:
            continue
        if path.name.endswith(SKIP_SUFFIXES):
            continue
        out.append(path)
    return out


def check_file(path: Path, fix: bool) -> tuple[int, int, list[str]]:
    """Вернуть (восстановимых, повреждённых, примеры) и, если fix, починить."""
    try:
        text = io.open(path, encoding="utf-8").read()
    except UnicodeDecodeError as exc:
        return (0, 1, [f"{path}: файл не читается как UTF-8: {exc}"])

    fixed: list[tuple[str, str]] = []
    damaged: list[str] = []

    def repl(match: re.Match[str]) -> str:
        run = match.group(0)
        out = unmangle(run)
        if out is not None:
            fixed.append((run, out))
            return out
        if LOST.search(run):
            damaged.append(run)
        return run

    new = RUN.sub(repl, text)
    if fix and fixed:
        io.open(path, "w", encoding="utf-8", newline="\n").write(new)

    samples = [
        f"    {run[:40]!r}  ->  {out[:40]!r}" for run, out in fixed[:3]
    ]
    return (len(fixed), len(damaged), samples)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="*", type=Path, default=DEFAULT_ROOTS)
    parser.add_argument("--fix", action="store_true",
                        help="починить восстановимое на месте")
    args = parser.parse_args()

    files: list[Path] = []
    for root in args.paths:
        files.extend(iter_files(root))

    total_fixable = 0
    excess_damage = 0
    for path in sorted(set(files)):
        fixable, damaged, samples = check_file(path, args.fix)
        rel = path.as_posix()
        allowed = KNOWN_DAMAGED.get(rel, 0)
        if fixable:
            total_fixable += fixable
            verb = "починено" if args.fix else "восстановимо"
            print(f"{rel}: {verb} {fixable}")
            print("\n".join(samples))
        if damaged > allowed:
            excess_damage += damaged - allowed
            print(f"{rel}: повреждено безвозвратно {damaged}"
                  f" (в KNOWN_DAMAGED записано {allowed})")

    if args.fix:
        print(f"починено прогонов: {total_fixable}")
        return 1 if excess_damage else 0

    if total_fixable or excess_damage:
        if total_fixable:
            print(f"\nНайдена испорченная кодировка: {total_fixable} прогон(ов).")
            print("Починить:  python tools/check_text_encoding.py --fix")
        if excess_damage:
            print(f"\nБезвозвратно повреждённых стало больше на {excess_damage}.")
            print("Если это осознанно — обнови KNOWN_DAMAGED в этом файле.")
        return 1

    print(f"кодировка в порядке ({len(set(files))} файлов)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
