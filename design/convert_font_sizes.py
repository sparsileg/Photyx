#!/usr/bin/env python3
# convert_font_sizes.py — Convert font-size px values to rem in a CSS file.
# Base: 16px = 1rem
#
# Usage:
#   python3 convert_font_sizes.py <file.css>          # dry run (shows changes)
#   python3 convert_font_sizes.py <file.css> --apply  # applies changes, saves .bak

import re
import sys
import shutil
from pathlib import Path

CONVERSIONS = {
    '9':  '0.56',
    '10': '0.63',
    '11': '0.69',
    '12': '0.75',
    '13': '0.81',
    '14': '0.88',
    '15': '0.94',
    '16': '1',
    '18': '1.13',
    '20': '1.25',
    '26': '1.63',
    '28': '1.75',
    '33': '2.06',
    '36': '2.25',
}

# Matches: optional whitespace, font-size:, optional whitespace, digits, px, optional rest of line
PATTERN = re.compile(r'^(\s*font-size:\s*)(\d+)px([\s;].*)$')

def convert_line(line: str) -> tuple[str, bool]:
    """Return (converted_line, was_changed)."""
    m = PATTERN.match(line.rstrip('\n'))
    if not m:
        return line, False
    px_val = m.group(2)
    if px_val not in CONVERSIONS:
        return line, False
    rem_val = CONVERSIONS[px_val]
    converted = f"{m.group(1)}{rem_val}rem{m.group(3)}\n"
    return converted, True

def process(path: Path, apply: bool) -> None:
    lines = path.read_text(encoding='utf-8').splitlines(keepends=True)
    changes = []
    new_lines = []

    for i, line in enumerate(lines, start=1):
        new_line, changed = convert_line(line)
        new_lines.append(new_line)
        if changed:
            changes.append((i, line.rstrip('\n'), new_line.rstrip('\n')))

    if not changes:
        print(f"No font-size px values found in {path.name}")
        return

    print(f"\n{'DRY RUN' if not apply else 'APPLYING'}: {path}")
    print(f"  {len(changes)} change(s):\n")
    for lineno, old, new in changes:
        print(f"  Line {lineno:4d}:  {old.strip()}")
        print(f"           →  {new.strip()}")
        print()

    if apply:
        bak = path.with_suffix(path.suffix + '.bak')
        shutil.copy2(path, bak)
        path.write_text(''.join(new_lines), encoding='utf-8')
        print(f"  Written to {path} (backup: {bak.name})")
    else:
        print(f"  Run with --apply to write changes.")

def main():
    if len(sys.argv) < 2:
        print("Usage: python3 convert_font_sizes.py <file.css> [--apply]")
        sys.exit(1)

    path = Path(sys.argv[1])
    if not path.exists():
        print(f"Error: {path} not found")
        sys.exit(1)

    apply = '--apply' in sys.argv
    process(path, apply)

if __name__ == '__main__':
    main()
