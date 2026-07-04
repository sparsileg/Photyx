# css_audit.py — CSS Selector Cross-Reference Tool

## Purpose

`css_audit.py` cross-references every CSS class and id selector defined in
Photyx's stylesheets against their actual usage in the source code. It answers
three questions:

1. Which selectors are defined in CSS but never referenced anywhere? (dead CSS)
2. Which classes/ids are referenced in code but defined in no CSS file?
   (typos, or references to removed styles)
3. Which class/id names are constructed dynamically at runtime and therefore
   can't be verified automatically? (manual review list)

It is a **static, regex-based heuristic** — not a full CSS/JS/Svelte parser.
It is designed to *sort evidence into confidence buckets* rather than give a
single yes/no answer, so that the "likely unused" list is trustworthy and
everything uncertain is surfaced for human judgment instead of silently
guessed at.

## Usage

```bash
python3 css_audit.py /path/to/Photyx
```

The single argument is the project root. No dependencies beyond the Python 3
standard library. Exit output goes to stdout; redirect to a file if desired:

```bash
python3 css_audit.py ~/projects/Photyx > css_audit_report.txt
```

## What It Scans

| Role | Locations | File types |
|---|---|---|
| CSS definitions | `static/css/`, `static/themes/` | `*.css` |
| Source usage | `src-svelte/` (recursive) | `.svelte`, `.ts`, `.js`, `.html` |
| Source usage | `src-tauri/src/` (recursive) | `.rs` (in case Rust emits HTML, e.g. report export) |

### Selector extraction (CSS side)

Class and id names are extracted only from **selector positions** — the text
preceding each `{` block — never from property values. This avoids false
positives from hex colors (`#aaaa33`) and `url()` values. Hex-color-shaped
tokens after `#` are additionally filtered by a length/hex-digit check.
`@keyframes` frame selectors (`0%`, `from`, `to`) are naturally skipped since
they don't begin with `.` or `#`.

### Usage detection (source side)

The following patterns count as a reference:

| Pattern | Example | Notes |
|---|---|---|
| Quoted class attribute | `class="kw-btn kw-btn-write"` | Also matches inside TS template strings that build HTML |
| `className` prop | `className="tp-profile-dropdown"` | Component prop passthrough (e.g. `Dropdown`) |
| Svelte class directive | `class:kw-selected={...}` | Name is always literal |
| Bare expression | `class={expr}`, `id={expr}` | Quoted literals inside `expr` are counted as used; the expression is also listed in section 3 |
| Interpolated attribute | `class="notif-item {n.type}"` | Literal tokens counted; whole attribute listed in section 3 |
| `classList` calls | `classList.add('a', 'b')` | `add`/`remove`/`toggle`/`replace`, all string args |
| Static id | `id="viewer-canvas"` | |
| DOM queries | `getElementById('x')`, `querySelector('.a #b')` | Also `querySelectorAll`, `closest`, `matches`, `getElementsByClassName`; selector strings are parsed for `.class`/`#id` tokens |
| `setAttribute` | `setAttribute('class', 'foo bar')` | Both `class` and `id` |
| Template-literal prefix | `` id={`tp-${field.key}`} `` | The static prefix (`tp-`) is recorded as a *prefix hint* — see section 1b |

Additionally, **every quoted string literal in every scanned source file** is
collected into a weak-evidence pool. A selector whose name appears in that
pool (e.g. `{ cls: 'status-success' }`, a function returning `'lv-warn'`) is
never reported as unused — it is routed to section 1c for manual verification
instead.

## Output Sections

### 1a. Likely unused
Defined in CSS; no reference of any kind found — not as an attribute, not as
a string literal anywhere, not matching any dynamic prefix. These are the
strongest candidates for deletion. Each entry lists the defining file(s).

### 1b. Possibly used via dynamic prefix
Not referenced directly, but the name matches a static prefix captured from a
template literal (e.g. `#tp-fwhm` matching `` `tp-${field.key}` ``). Verify
what values the dynamic part can take before deleting.

### 1c. Name appears as a string literal
Not referenced in any class/id position, but the exact name appears in an
ordinary string somewhere in the source — typically a lookup map or a
function that returns class names. Usually these are genuinely in use; check
the listed defining file against wherever the literal appears.

### 1d. Defined only in theme files
Selectors defined exclusively in `static/themes/*.css` with no app usage.
In Photyx's case these are typically leftovers inherited from the theme
files' origin in an earlier project (e.g. `.tutorial-modal`, `.toast-*`,
`.logo-ring`). Safe-to-delete candidates, but grouped separately because
they indicate stale content in the theme files themselves rather than in
the per-panel stylesheets.

### 2. Undefined references
Classes/ids referenced in source with no matching CSS definition anywhere.
Some are intentional (structural class names that need no styling, ids used
only as JS handles); others are typos or references to styles that were
deleted. Review each.

### 3. Dynamic expressions — manual review
Every class/id expression the script could not fully resolve, with file and
line number. String literals inside these were already counted as used
(sections 1a–1c account for that), but the *non-literal* parts — `{n.type}`,
`catClass(row.category)`, `meta.cls` — can produce names the script can't
predict. Cross-check this list against section 1a before deleting anything:
if a dynamic expression could plausibly generate a "likely unused" name,
that name isn't actually unused.

### Summary
Counts for every category, printed last.

## Recommended Workflow

1. Run against a clean checkout (not a partially-modified working tree).
2. Work through **section 3 first** — understand what each dynamic expression
   can generate, and mentally strike any section-1a entries it could produce.
3. Delete confirmed-dead selectors from **1a**, in small batches, rebuilding
   and spot-checking the affected panels between batches.
4. Review **1d** as a separate cleanup of the theme files themselves.
5. Treat **1b/1c** as verification tasks, not deletion lists.
6. Use **section 2** to catch typos — a class referenced but never defined
   sometimes indicates a selector that was renamed on only one side.

## Limitations

- **Regex heuristic.** No real parsing of CSS, TypeScript, or Svelte.
  Unusual formatting (selectors split across lines mid-name, exotic string
  building) can evade detection.
- **Truly dynamic names are unresolvable.** `class="notif-item {n.type}"`
  where `n.type` comes from runtime data can never be verified statically;
  the tool's job is to route these to review, not decide them.
- **The literal pool is deliberately over-inclusive.** Any quoted string
  matching a selector name suppresses an "unused" verdict, even if the
  string has nothing to do with CSS (e.g. a pcode command coincidentally
  named like a class). This trades some false "possibly used" entries for
  a trustworthy "likely unused" list — the safe direction for a deletion
  tool to err.
- **No specificity or cascade analysis.** The tool only checks existence of
  references, not whether a rule actually wins the cascade or has visible
  effect.
- **CSS variables are out of scope.** Undefined `var(--x)` usage is a
  separate class of problem (see the theme-variable audit done previously);
  this tool covers selectors only.
