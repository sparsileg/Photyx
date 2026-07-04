#!/usr/bin/env python3
"""
css_audit.py — Cross-reference CSS class/id selectors defined in
static/css/*.css and static/themes/*.css against their usage across
src-svelte/ (and src-tauri/, in case any Rust code emits HTML/class
attributes, e.g. report export).

USAGE
    python3 css_audit.py /path/to/<project root>

OUTPUT
    1. UNUSED SELECTORS   — defined in CSS, no static reference found.
       Split into three sub-lists:
         1a. likely unused (no evidence of any use)
         1b. possibly used via a dynamic prefix (e.g. `tp-${field.key}`)
         1c. defined ONLY in theme files (candidates for leftovers from
             another project's theme files)
    2. UNDEFINED REFS     — referenced in source, not defined in any CSS file
    3. DYNAMIC / MANUAL REVIEW — class/id expressions built at runtime that
       could not be fully resolved statically. Quoted string literals inside
       these expressions ARE extracted and counted as used; the expression is
       still listed here so the non-literal parts can be checked by hand.

WHAT COUNTS AS A USAGE
    - class="..." / class='...'   (including inside TS template strings)
    - className="..."             (component prop passthrough, e.g. Dropdown)
    - class={expr} / className={expr} / id={expr}
        -> quoted literals inside expr counted as used; rest flagged
    - class:name Svelte directive
    - classList.add/remove/toggle/replace(...) — all string args
    - id="literal"
    - getElementById('x'), getElementsByClassName('x')
    - querySelector/querySelectorAll/closest/matches('selector') — the
      selector string is parsed for .class and #id tokens
    - setAttribute('class'|'id', '...')
    - template literals with a static prefix (`pref-${key}`) contribute a
      PREFIX HINT: unused selectors matching the prefix are reclassified
      into section 1b rather than reported as unused

LIMITATIONS (regex heuristic, not a real CSS/JS/Svelte parser):
  - Truly dynamic names (e.g. class="notif-item {n.type}" where n.type is
    data-driven) can never be resolved statically; they appear in section 3
    and any matching "unused" selectors need manual judgment.
  - Hex colors after '#' are excluded from id extraction by a length/hex
    filter — heuristic, not a tokenizer.
  - @keyframes selectors (0%, from, to) are naturally skipped since they
    don't start with '.' or '#'.
"""

import argparse
import re
from pathlib import Path
from collections import defaultdict

HEX_COLOR_RE = re.compile(r'^[0-9a-fA-F]{3}$|^[0-9a-fA-F]{4}$|^[0-9a-fA-F]{6}$|^[0-9a-fA-F]{8}$')

CLASS_TOKEN_RE = re.compile(r'\.([a-zA-Z_-][a-zA-Z0-9_-]*)')
ID_TOKEN_RE    = re.compile(r'#([a-zA-Z_-][a-zA-Z0-9_-]*)')

# ── CSS-side extraction ──────────────────────────────────────────────────

def strip_css_comments(text: str) -> str:
    return re.sub(r'/\*.*?\*/', '', text, flags=re.DOTALL)


def extract_css_selectors(css_text: str):
    """Extract class/id names from selector positions (text before each
    '{'), not from property values, to avoid false positives from hex
    colors and url()s."""
    text = strip_css_comments(css_text)
    classes = set()
    ids = set()

    pos = 0
    while True:
        brace_idx = text.find('{', pos)
        if brace_idx == -1:
            break
        selector_text = text[pos:brace_idx]
        pos = brace_idx + 1

        for m in CLASS_TOKEN_RE.finditer(selector_text):
            classes.add(m.group(1))
        for m in ID_TOKEN_RE.finditer(selector_text):
            name = m.group(1)
            if not HEX_COLOR_RE.match(name):
                ids.add(name)

    return classes, ids


def load_css_definitions(css_dirs):
    defined_classes = defaultdict(list)  # name -> [file paths]
    defined_ids     = defaultdict(list)

    for d in css_dirs:
        if not d.is_dir():
            continue
        for f in sorted(d.glob('*.css')):
            text = f.read_text(encoding='utf-8', errors='replace')
            classes, ids = extract_css_selectors(text)
            for c in classes:
                defined_classes[c].append(str(f))
            for i in ids:
                defined_ids[i].append(str(f))

    return defined_classes, defined_ids


# ── Source-side extraction ───────────────────────────────────────────────

# class="..." or className="..." — quoted attribute (may contain {expr})
CLASS_ATTR_RE = re.compile(r'\bclass(?:Name)?\s*=\s*(["\'])(.*?)\1', re.DOTALL)
# class={expr} / className={expr} — bare expression, no quotes
CLASS_ATTR_BARE_RE = re.compile(r'\bclass(?:Name)?\s*=\s*\{([^}]*)\}')
CLASS_DIRECTIVE_RE = re.compile(r'\bclass:([a-zA-Z_-][a-zA-Z0-9_-]*)')
# classList.add/remove/toggle/replace(...) — capture the whole arg list
CLASSLIST_CALL_RE  = re.compile(r'classList\.(?:add|remove|toggle|replace)\(([^)]*)\)')
ID_ATTR_STATIC_RE  = re.compile(r'\bid\s*=\s*(["\'])([a-zA-Z_-][a-zA-Z0-9_-]*)\1')
ID_ATTR_DYNAMIC_RE = re.compile(r'\bid\s*=\s*\{([^}]*)\}')
DYNAMIC_EXPR_IN_ATTR_RE = re.compile(r'\{[^}]*\}')

# DOM query APIs
GET_BY_ID_RE       = re.compile(r'getElementById\(\s*(["\'])([a-zA-Z_-][a-zA-Z0-9_-]*)\1')
GET_BY_CLASS_RE    = re.compile(r'getElementsByClassName\(\s*(["\'])([a-zA-Z_-][a-zA-Z0-9_-]*)\1')
QUERY_SEL_RE       = re.compile(r'(?:querySelector|querySelectorAll|closest|matches)\(\s*(["\'])(.*?)\1')
SET_ATTR_RE        = re.compile(r'setAttribute\(\s*["\'](class|id)["\']\s*,\s*(["\'])(.*?)\2')

# Quoted string literals inside an arbitrary JS/TS expression
STRING_LIT_RE      = re.compile(r'''["']([a-zA-Z_-][a-zA-Z0-9_ -]*)["']''')
# Template literal with a static prefix before the first ${...}
TEMPLATE_PREFIX_RE = re.compile(r'`([a-zA-Z_-][a-zA-Z0-9_-]*)\$\{')


def _add_tokens(raw: str, target: set):
    for tok in raw.split():
        tok = tok.strip()
        if tok:
            target.add(tok)


def _extract_literals_and_prefixes(expr: str, target: set, prefixes: set):
    """Pull quoted string literals out of a JS/TS expression and count each
    whitespace-separated token as a used name. Also collect static prefixes
    from template literals (`foo-${...}`)."""
    for m in STRING_LIT_RE.finditer(expr):
        _add_tokens(m.group(1), target)
    for m in TEMPLATE_PREFIX_RE.finditer(expr):
        prefixes.add(m.group(1))


def extract_source_usage(text, filepath, used_classes, used_ids,
                         class_prefixes, id_prefixes, dynamic_notes):
    # class="..." / className="..." — may contain {expr} interpolations
    for m in CLASS_ATTR_RE.finditer(text):
        raw = m.group(2)
        if '{' in raw:
            literal_part = DYNAMIC_EXPR_IN_ATTR_RE.sub(' ', raw)
            _add_tokens(literal_part, used_classes)
            # Literals + prefixes inside each {expr}
            for expr_m in re.finditer(r'\{([^}]*)\}', raw):
                _extract_literals_and_prefixes(expr_m.group(1), used_classes, class_prefixes)
            line_no = text.count('\n', 0, m.start()) + 1
            dynamic_notes.append((filepath, line_no, 'class', raw.strip()))
        else:
            _add_tokens(raw, used_classes)

    # class={expr} / className={expr} — bare expression
    for m in CLASS_ATTR_BARE_RE.finditer(text):
        expr = m.group(1)
        _extract_literals_and_prefixes(expr, used_classes, class_prefixes)
        line_no = text.count('\n', 0, m.start()) + 1
        dynamic_notes.append((filepath, line_no, 'class', expr.strip()))

    # class:name Svelte directive
    for m in CLASS_DIRECTIVE_RE.finditer(text):
        used_classes.add(m.group(1))

    # classList.add/remove/toggle/replace — all string args
    for m in CLASSLIST_CALL_RE.finditer(text):
        for lit in STRING_LIT_RE.finditer(m.group(1)):
            _add_tokens(lit.group(1), used_classes)

    # id="literal"
    for m in ID_ATTR_STATIC_RE.finditer(text):
        used_ids.add(m.group(2))

    # id={expr}
    for m in ID_ATTR_DYNAMIC_RE.finditer(text):
        expr = m.group(1)
        _extract_literals_and_prefixes(expr, used_ids, id_prefixes)
        line_no = text.count('\n', 0, m.start()) + 1
        dynamic_notes.append((filepath, line_no, 'id', expr.strip()))

    # getElementById / getElementsByClassName
    for m in GET_BY_ID_RE.finditer(text):
        used_ids.add(m.group(2))
    for m in GET_BY_CLASS_RE.finditer(text):
        used_classes.add(m.group(2))

    # querySelector / querySelectorAll / closest / matches — parse the
    # selector string for .class and #id tokens
    for m in QUERY_SEL_RE.finditer(text):
        sel = m.group(2)
        for cm in CLASS_TOKEN_RE.finditer(sel):
            used_classes.add(cm.group(1))
        for im in ID_TOKEN_RE.finditer(sel):
            name = im.group(1)
            if not HEX_COLOR_RE.match(name):
                used_ids.add(name)

    # setAttribute('class'|'id', '...')
    for m in SET_ATTR_RE.finditer(text):
        kind, value = m.group(1), m.group(3)
        if kind == 'class':
            _add_tokens(value, used_classes)
        else:
            used_ids.add(value.strip())


def scan_source_dirs(source_dirs, extensions):
    used_classes = set()
    used_ids = set()
    class_prefixes = set()
    id_prefixes = set()
    dynamic_notes = []  # (file, line, 'class'|'id', raw_snippet)
    literal_pool = set()  # every quoted string-literal token anywhere in source

    for d in source_dirs:
        if not d.is_dir():
            continue
        for ext in extensions:
            for f in sorted(d.rglob(f'*{ext}')):
                text = f.read_text(encoding='utf-8', errors='replace')
                extract_source_usage(text, str(f), used_classes, used_ids,
                                     class_prefixes, id_prefixes, dynamic_notes)
                # Weak-evidence pass: any quoted string token could be a
                # class/id name assigned via data (e.g. { cls: 'status-ok' },
                # functions returning 'lv-warn'). Collected separately so
                # matches move to a review bucket instead of silently
                # counting as used.
                for m in STRING_LIT_RE.finditer(text):
                    _add_tokens(m.group(1), literal_pool)

    return used_classes, used_ids, class_prefixes, id_prefixes, dynamic_notes, literal_pool


# ── Reporting helpers ────────────────────────────────────────────────────

def matches_prefix(name: str, prefixes: set) -> str | None:
    for p in sorted(prefixes, key=len, reverse=True):
        if name.startswith(p):
            return p
    return None


def theme_only(files: list, themes_dir: Path) -> bool:
    return all(Path(p).parent == themes_dir for p in files)


# ── Main ──────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('project_root', type=Path, help='Path to the project root')
    args = parser.parse_args()

    root = args.project_root
    themes_dir = root / 'static' / 'themes'
    css_dirs = [
        root / 'static' / 'css',
        themes_dir,
    ]
    svelte_src_dirs = [root / 'src-svelte']
    tauri_src_dirs  = [root / 'src-tauri' / 'src']

    defined_classes, defined_ids = load_css_definitions(css_dirs)

    used_classes, used_ids, class_prefixes, id_prefixes, dynamic_notes, literal_pool = scan_source_dirs(
        svelte_src_dirs, ['.svelte', '.ts', '.js', '.html']
    )
    rust = scan_source_dirs(tauri_src_dirs, ['.rs'])
    used_classes  |= rust[0]
    used_ids      |= rust[1]
    class_prefixes |= rust[2]
    id_prefixes    |= rust[3]
    dynamic_notes += rust[4]
    literal_pool  |= rust[5]

    unused_classes = sorted(set(defined_classes) - used_classes)
    unused_ids     = sorted(set(defined_ids) - used_ids)

    # Split unused into: prefix-matched, string-literal evidence,
    # theme-only, and truly unused
    truly_unused, prefix_matched, literal_matched, theme_only_list = [], [], [], []
    for c in unused_classes:
        p = matches_prefix(c, class_prefixes)
        if p:
            prefix_matched.append(('.' + c, p, defined_classes[c]))
        elif c in literal_pool:
            literal_matched.append(('.' + c, defined_classes[c]))
        elif theme_only(defined_classes[c], themes_dir):
            theme_only_list.append(('.' + c, defined_classes[c]))
        else:
            truly_unused.append(('.' + c, defined_classes[c]))
    for i in unused_ids:
        p = matches_prefix(i, id_prefixes)
        if p:
            prefix_matched.append(('#' + i, p, defined_ids[i]))
        elif i in literal_pool:
            literal_matched.append(('#' + i, defined_ids[i]))
        elif theme_only(defined_ids[i], themes_dir):
            theme_only_list.append(('#' + i, defined_ids[i]))
        else:
            truly_unused.append(('#' + i, defined_ids[i]))

    def fnames(paths):
        return ', '.join(sorted(set(Path(p).name for p in paths)))

    print('=' * 78)
    print('1a. LIKELY UNUSED — defined in CSS, no static reference found')
    print('=' * 78)
    if not truly_unused:
        print('  (none found)')
    else:
        for sel, files in truly_unused:
            print(f'  {sel:<42} defined in: {fnames(files)}')

    print()
    print('=' * 78)
    print('1b. POSSIBLY USED VIA DYNAMIC PREFIX — verify manually')
    print('=' * 78)
    if not prefix_matched:
        print('  (none found)')
    else:
        for sel, prefix, files in prefix_matched:
            print(f'  {sel:<42} matches prefix `{prefix}…` — defined in: {fnames(files)}')

    print()
    print('=' * 78)
    print('1c. NAME APPEARS AS A STRING LITERAL IN SOURCE — verify manually')
    print('    (e.g. { cls: \'status-ok\' } or a function returning class names)')
    print('=' * 78)
    if not literal_matched:
        print('  (none found)')
    else:
        for sel, files in literal_matched:
            print(f'  {sel:<42} defined in: {fnames(files)}')

    print()
    print('=' * 78)
    print('1d. DEFINED ONLY IN THEME FILES — likely leftovers from another project')
    print('=' * 78)
    if not theme_only_list:
        print('  (none found)')
    else:
        for sel, files in theme_only_list:
            print(f'  {sel:<42} defined in: {fnames(files)}')

    print()
    print('=' * 78)
    print('2. CLASS/ID REFERENCES IN SOURCE WITH NO MATCHING CSS DEFINITION')
    print('=' * 78)
    undefined_classes = sorted(used_classes - set(defined_classes))
    undefined_ids     = sorted(used_ids - set(defined_ids))
    if not undefined_classes and not undefined_ids:
        print('  (none found)')
    else:
        for c in undefined_classes:
            print(f'  .{c}')
        for i in undefined_ids:
            print(f'  #{i}')

    print()
    print('=' * 78)
    print('3. DYNAMIC CLASS/ID EXPRESSIONS — string literals inside these were')
    print('   counted as used; review the non-literal parts by hand')
    print('=' * 78)
    if not dynamic_notes:
        print('  (none found)')
    else:
        for filepath, line_no, kind, snippet in dynamic_notes:
            rel = filepath
            try:
                rel = str(Path(filepath).relative_to(root))
            except ValueError:
                pass
            print(f'  [{kind}] {rel}:{line_no}  ->  {snippet}')

    print()
    print('=' * 78)
    print('SUMMARY')
    print('=' * 78)
    print(f'  CSS classes defined:        {len(defined_classes)}')
    print(f'  CSS ids defined:            {len(defined_ids)}')
    print(f'  Classes referenced:         {len(used_classes)}')
    print(f'  Ids referenced:             {len(used_ids)}')
    print(f'  Likely unused:              {len(truly_unused)}')
    print(f'  Possibly used (prefix):     {len(prefix_matched)}')
    print(f'  Possibly used (literal):    {len(literal_matched)}')
    print(f'  Theme-only (check origin):  {len(theme_only_list)}')
    print(f'  Undefined references:       {len(undefined_classes) + len(undefined_ids)}')
    print(f'  Dynamic exprs to review:    {len(dynamic_notes)}')


if __name__ == '__main__':
    main()
