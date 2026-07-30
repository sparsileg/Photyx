#!/usr/bin/env bash
# convert_all_css.sh — Apply font-size px→rem conversion to all CSS files.
# Base: 16px = 1rem
#
# Usage:
#   bash convert_all_css.sh /path/to/static/css   # dry run
#   bash convert_all_css.sh /path/to/static/css --apply

CSS_DIR="${1:?Usage: convert_all_css.sh <css_dir> [--apply]}"
APPLY="${2:-}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

if [ ! -d "$CSS_DIR" ]; then
    echo "Error: directory not found: $CSS_DIR"
    exit 1
fi

FILES=$(find "$CSS_DIR" -maxdepth 1 -name "*.css" | sort)

if [ -z "$FILES" ]; then
    echo "No CSS files found in $CSS_DIR"
    exit 1
fi

echo "CSS directory: $CSS_DIR"
echo "Mode: ${APPLY:+APPLY}${APPLY:-DRY RUN}"
echo "----------------------------------------"

for f in $FILES; do
    python3 "$SCRIPT_DIR/convert_font_sizes.py" "$f" $APPLY
done

echo "----------------------------------------"
echo "Done."
