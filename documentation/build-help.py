"""
build-help.py
Builds Photgyx help system using MkDocs and copies output to destination.
Usage: python build-help.py --output <path> [--clean]
"""

import os
import sys
import shutil
import subprocess
import argparse
from pathlib import Path

def build_help(output_path, clean=False):
    script_dir = Path(__file__).parent.resolve()
    os.chdir(script_dir)
    dest = (script_dir / Path(output_path)).resolve()

    print(f"Building Photyx help system...")
    print(f"Source:      {script_dir}")
    print(f"Destination: {dest}")
    print()

    # Run mkdocs build
    result = subprocess.run(["mkdocs", "build"], capture_output=True, text=True)
    if result.returncode != 0:
        print("ERROR: MkDocs build failed:")
        print(result.stderr)
        return False

    print("MkDocs build successful.")

    # Copy site/ to destination
    site_dir = script_dir / "site"
    if not site_dir.exists():
        print(f"ERROR: site/ directory not found after build")
        return False

    # Clear destination and copy fresh output
    if dest.exists():
        shutil.rmtree(dest)
    shutil.copytree(site_dir, dest)

    if clean:
        for f in ["sitemap.xml", "sitemap.xml.gz"]:
            sitemap = dest / f
            if sitemap.exists():
                sitemap.unlink()
                print(f"Removed {f}")

        # Remove lunr language files except English
        lunr_dir = dest / "assets" / "javascripts" / "lunr" / "min"
        if lunr_dir.exists():
            for f in lunr_dir.iterdir():
                if f.name != "lunr.en.min.js":
                    f.unlink()
                    print(f"Removed {f.name}")

   # Remove site/ directory
    shutil.rmtree(site_dir)
    print(f"Removed site/")

    print(f"Copied to {dest}")
    print(f"Done!")
    return True


#-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-#`

if __name__ == '__main__':
    parser = argparse.ArgumentParser(
        description='Build Photyx help system using MkDocs.'
    )
    parser.add_argument(
        '--output', '-o',
        required=True,
        help='Output path for generated HTML'
    )
    parser.add_argument(
        '--clean', '-c',
        action='store_true',
        help='Remove sitemap and language files files from output'
    )
    args = parser.parse_args()

    success = build_help(args.output, args.clean)
    if not success:
        exit(1)

#-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-#`
#-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-#`
#-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-#`
