# Photyx — Build & Release Reference

Living reference for building Photyx locally, producing installers for
Linux/Windows/macOS, and shipping releases (manually or via
CI). Update this doc whenever the build setup changes.

---

## 0. Project layout that matters for builds

Photyx is a **Cargo workspace**, not a single crate:

```
Photyx/                     <- workspace root Cargo.toml (members list only)
├── Cargo.toml              <- [workspace], no [package], no version
├── package.json            <- npm scripts, its own "version" field
├── crates/photyx-xisf/     <- workspace member
├── src-tauri/
│   ├── Cargo.toml          <- [package] version — THE version, since Issue 161
│   ├── tauri.conf.json     <- no "version" field (falls back to Cargo.toml)
│   └── src/
├── src-svelte/              <- SvelteKit frontend
└── build/                   <- frontend build output (adapter-static), gitignored
```

**Important consequence:** because this is a workspace, `cargo
build`/`cargo check` — run from either the repo root or from
`src-tauri/` — resolve the workspace and put compiled output at the
**workspace root**: `Photyx/target/release/photyx`, *not*
`Photyx/src-tauri/target/...`. Keep this in mind for CI caching paths
and any script that looks for the binary.

### Three version numbers that must not be confused

| File | Field | Who reads it |
|---|---|---|
| `src-tauri/Cargo.toml` | `[package] version` | **Source of truth.** Cargo's own build output, `getVersion()` (via Tauri's fallback), `tauri-action`'s `__VERSION__` substitution |
| `src-tauri/tauri.conf.json` | `version` | Deliberately **absent** (Issue 161) so it falls back to Cargo.toml. Do not re-add it unless you want to fork the two apart again |
| `package.json` | `version` | npm/Node tooling only (e.g. what `npm run tauri build`'s banner prints). Currently **not kept in sync** — cosmetic drift, harmless, but don't be surprised if it disagrees with the other two |

**SemVer only.** Cargo enforces strict SemVer on `[package] version` —
no bare suffixes like `0.11.0B`. Use a proper prerelease identifier
instead:

- `0.11.0-beta` / `0.11.0-beta.1` — beta builds
- `0.11.0-rc.1` — release candidate
- `0.11.0` — stable

---

## 1. Local development builds (no bundling)

For day-to-day iteration you almost never need a bundled installer.

**Hot-reload dev mode** (frontend + backend, auto-rebuild on save):
```bash
npm run tauri dev
```

**Rust-only correctness check** (fastest signal, no codegen/link, no bundling):
```bash
cd src-tauri && cargo check && cd ..
```
This is what you've been using between deltas in this chat — it's the
right tool for "did I break the syntax," and it's much faster than a full
build because it skips code generation and linking entirely.

**Full compiled binary, no installer** — what you're already doing:
```bash
npm run tauri build -- --no-bundle
./target/release/photyx
```
This runs the real `tauri build` pipeline (frontend build → Rust release
compile) but skips the platform-packaging step (`.deb`/`.msi`/`.dmg`/etc.),
so you get a runnable binary fast without producing installers you don't
need yet.

**Run the test suite** (e.g. the Issue 159 unit tests):
```bash
cd src-tauri && cargo test && cd ..
# or a single module:
cargo test analyze_frames
```

---

## 2. Platform-specific bundled builds

`tauri.conf.json`'s `bundle.targets` is already set to `"all"`, so a
plain `npm run tauri build` on any given platform produces every
installer type that platform supports — no extra flags needed once the
platform's own toolchain is installed.

**You cannot meaningfully cross-compile Windows or macOS bundles from
Linux for this app.** Photyx links native C libraries (`cfitsio` via
`fitsio-sys`) and Tauri bundles are platform-native installers — both
defeat casual cross-compilation. Build Windows on a Windows machine
(or `windows-latest` CI runner) and macOS on a Mac (or `macos-latest`
runner).  This is also why the GitHub Actions workflow in §4 uses a
build matrix instead of one Linux job cross-compiling everything.

### Linux (primary dev platform)

System packages (Ubuntu/Debian; adjust for your distro):
```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
  patchelf xdg-utils build-essential \
  libcfitsio-dev
```
Note: some newer Debian/Ubuntu releases have renamed
`libappindicator3-dev` to `libayatana-appindicator3-dev` — if the package
isn't found, try that name instead.

```bash
npm run tauri build
```
Produces (per `bundle.targets = "all"`) a `.deb` and an `.AppImage` under
`src-tauri/target/release/bundle/` — remember, since this is a workspace,
double-check whether your Cargo config puts this at the workspace root
instead (`target/release/bundle/...`), matching §0 above.

### Windows

Needs: Visual Studio Build Tools (MSVC toolchain), and `cfitsio` via
vcpkg — matching your existing per-session vcpkg setup (`J:\vcpkg`).

```powershell
# One-time (or per session, per your existing notes):
vcpkg install cfitsio
# Ensure VCPKG_ROOT and any fitsio-sys-specific env vars are set exactly
# as in your current working local setup before running the build —
# confirm the exact variable names against what already works for you
# locally, since this wasn't independently re-verified while writing this doc.

npm run tauri build
```
Produces an `.msi` and/or NSIS `.exe` installer under
`target/release/bundle/`.

### macOS

Needs: Xcode Command Line Tools, and `cfitsio` (via Homebrew: `brew
install cfitsio`, or vcpkg).

```bash
npm run tauri build
```
Produces a `.app` bundle and `.dmg` under `target/release/bundle/`.

**Apple Silicon vs Intel:** by default this builds for the host
architecture only. To build both and ship separately (matching what
the GitHub Actions matrix in §4 does): ```bash npm run tauri build --
--target aarch64-apple-darwin # Apple Silicon npm run tauri build --
--target x86_64-apple-darwin # Intel ``` There's no universal-binary
flag built into `tauri build` — the two targets ship as separate
`.dmg`s, which is also what the official Tauri CI example does (see
§4).

---

## 3. Delivering releases on GitHub

Releases are built on **git tags**. `gh` (which you already use for
issues) is the fastest way to cut one, matching the tooling you
already have.

### Tag/version conventions

- Stable: `v0.11.0` (tag) ↔ `0.11.0` (Cargo.toml)
- Beta: `v0.11.0-beta.1` ↔ `0.11.0-beta.1`
- Release candidate: `v0.11.0-rc.1` ↔ `0.11.0-rc.1`

Keep the tag and `src-tauri/Cargo.toml`'s version in agreement — bump
Cargo.toml first, commit, *then* tag.

### Creating a release manually

```bash
# Bump the version first
#   edit src-tauri/Cargo.toml -> version = "0.11.0-beta.1"
git add src-tauri/Cargo.toml
git commit -m "Bump version to 0.11.0-beta.1"
git push

# Tag + release in one step (gh creates the tag if it doesn't exist)
gh release create v0.11.0-beta.1 \
  --title "v0.11.0-beta.1" \
  --notes "Beta build for external testing" \
  --prerelease
```

`--prerelease` is what marks it as beta/RC rather than a stable
"Latest release" — do this for every beta and RC build. Drop the flag
only for an actual stable release.

**Attaching built installers** so testers can download and run directly —
append file paths after the flags:
```bash
gh release create v0.11.0-beta.1 \
  --title "v0.11.0-beta.1" \
  --notes "Beta build for external testing" \
  --prerelease \
  target/release/bundle/deb/photyx_0.11.0-beta.1_amd64.deb \
  target/release/bundle/appimage/photyx_0.11.0-beta.1_amd64.AppImage
```

**Auto-generated notes** instead of writing them by hand (summarizes
merged PRs/commits since the last tag):
```bash
gh release create v0.11.0-beta.1 --prerelease --generate-notes
```

**Draft first, publish later** (useful if you want to review before
testers see it):
```bash
gh release create v0.11.0-beta.1 --prerelease --draft --notes "..."
# later, once ready:
gh release edit v0.11.0-beta.1 --draft=false
```

**Web UI equivalent**, if you'd rather click through: repo →
**Releases** (right sidebar) → **Draft a new release** → pick/create
the tag → title/notes → check **"Set as a pre-release"** for beta/RC →
drag installer files into the assets area → **Publish release**.

---

## 4. Automating builds with GitHub Actions

("Workers" — the actual term is **runners**: GitHub-hosted VMs —
Linux, Windows, macOS — that execute your workflow's **jobs**. A
**workflow** is the YAML file; each job runs on one runner and
executes a sequence of **steps**, each of which is either a shell
command or a reusable **action**.)

The standard, officially-recommended way to build + release a Tauri
app is
[`tauri-apps/tauri-action`](https://github.com/tauri-apps/tauri-action)
— it runs `tauri build` on each platform in a matrix, creates the
GitHub release, and uploads all the platform installers to it
automatically.

### Workflow file

Save as `.github/workflows/release.yml`. Adjust the trigger to match
how you want to cut releases — this example triggers on pushing a
version tag (`v0.11.0`, `v0.11.0-beta.1`, etc.), which fits the
tagging convention in §3:

```yaml
name: 'release'

on:
  workflow_dispatch:
  push:
    tags:
      - 'v*'

jobs:
  publish-tauri:
    permissions:
      contents: write
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: 'macos-latest'   # Apple Silicon
            args: '--target aarch64-apple-darwin'
          - platform: 'macos-latest'   # Intel
            args: '--target x86_64-apple-darwin'
          - platform: 'ubuntu-22.04'
            args: ''
          - platform: 'windows-latest'
            args: ''

    runs-on: ${{ matrix.platform }}

    steps:
      - uses: actions/checkout@v7

      - name: install Linux dependencies
        if: matrix.platform == 'ubuntu-22.04'
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
            patchelf xdg-utils libcfitsio-dev

      - name: install Windows dependencies (vcpkg + cfitsio)
        if: matrix.platform == 'windows-latest'
        run: |
          vcpkg install cfitsio
          echo "VCPKG_ROOT=$env:VCPKG_INSTALLATION_ROOT" >> $env:GITHUB_ENV
        shell: pwsh
        # NOTE: confirm this matches whatever env vars fitsio-sys actually
        # needs on your machine (see §2 Windows section) — GitHub's
        # windows-latest runners ship vcpkg pre-installed at
        # $env:VCPKG_INSTALLATION_ROOT, which is a good starting point,
        # but verify against your known-working local setup before
        # trusting this in CI.

      - name: install macOS dependencies (cfitsio via Homebrew)
        if: matrix.platform == 'macos-latest'
        run: brew install cfitsio

      - name: setup node
        uses: actions/setup-node@v6
        with:
          node-version: lts/*
          cache: 'npm'

      - name: install Rust stable
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.platform == 'macos-latest' && 'aarch64-apple-darwin,x86_64-apple-darwin' || '' }}

      - name: Rust cache
        uses: swatinem/rust-cache@v2
        with:
          # Photyx is a Cargo workspace — target/ lives at the repo root,
          # not inside src-tauri/. This differs from Tauri's default
          # single-crate template (which uses 'src-tauri -> target').
          workspaces: '. -> target'

      - name: install frontend dependencies
        run: npm install

      - uses: tauri-apps/tauri-action@v1
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tagName: v__VERSION__
          releaseName: 'Photyx v__VERSION__'
          releaseBody: 'See the assets below to download and install this version.'
          releaseDraft: true
          prerelease: ${{ contains(github.ref_name, '-beta') || contains(github.ref_name, '-rc') }}
          args: ${{ matrix.args }}
```

Notes specific to this workflow:

- **`__VERSION__` substitution** reads from `src-tauri/Cargo.toml`
  (since `tauri.conf.json`'s own `version` field was deliberately
  removed — Issue 161's fallback behavior applies here too, not just
  locally).
- **`prerelease` is computed from the tag name** — pushing
  `v0.11.0-beta.1` or `v0.11.0-rc.1` automatically marks the GitHub
  release as a pre-release; a plain `v0.11.0` doesn't. Adjust the
  `contains(...)`  condition if you land on a different naming scheme.
- **`releaseDraft: true`** — the workflow creates a draft release
  rather than publishing immediately, so you can review before testers
  see it (`gh release edit <tag> --draft=false` to publish, matching
  §3).
- **Linux runner pinned to `ubuntu-22.04`**, not `ubuntu-latest` —
  this matches current official Tauri guidance and avoids surprises
  when GitHub rolls `ubuntu-latest` forward to a new default image
  with different package names/versions available.

### Triggering it

```bash
# Bump version, commit, then:
git tag v0.11.0-beta.1
git push origin v0.11.0-beta.1
```
The workflow picks up the tag push automatically and does the rest.

### First-time setup gotcha

The workflow's `GITHUB_TOKEN` only has **read** permissions by default
— you'll get a "Resource not accessible by integration" error
otherwise. Fix once, repo-wide: **Settings → Actions → General →
Workflow permissions → Read and write permissions**.

### If you outgrow this later

- **Arm Linux builds** (if ever needed): GitHub now offers
  `ubuntu-22.04-arm`/`ubuntu-24.04-arm` public runners that slot
  directly into the same matrix — no emulation needed for that
  architecture specifically.
- **Code signing** (removes "unidentified developer"/SmartScreen
  warnings on macOS/Windows): a separate, more involved setup — Tauri
  has dedicated guides for
  [macOS](https://v2.tauri.app/distribute/sign/macos/) and
  [Windows](https://v2.tauri.app/distribute/sign/windows/) signing if
  this becomes worth the overhead once beta testing wraps up.

---

## Quick reference

| Task | Command |
|---|---|
| Hot-reload dev | `npm run tauri dev` |
| Fast Rust check | `cd src-tauri && cargo check` |
| Run tests | `cd src-tauri && cargo test` |
| Local build, no installer | `npm run tauri build -- --no-bundle` |
| Full bundled build (current platform) | `npm run tauri build` |
| macOS: specific arch | `npm run tauri build -- --target aarch64-apple-darwin` |
| Cut a release (manual) | `gh release create vX.Y.Z --prerelease --notes "..."` |
| Cut a release (CI) | `git tag vX.Y.Z && git push origin vX.Y.Z` |
