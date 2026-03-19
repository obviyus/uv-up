# UVLIFT

Interactive dependency updater for `uv` projects.

## Installation

```bash
cargo build --release
cp target/release/uvlift ~/.local/bin/
```

If you control the PyPI name:

```bash
uvx uvlift
```

`uvx` installs Python packages, not Cargo crates. The `pyproject.toml` in this repo uses `maturin` to package the Rust binary for that flow.

Local smoke test:

```bash
uvx --from . uvlift
```

For "just works" installs, publish wheels for macOS, Linux, and Windows. If PyPI only has an sdist, users need a Rust toolchain.

## Usage

Run inside a repo containing one or more `pyproject.toml` files:

```bash
uvlift
```

Keys:

- ↑/↓ or j/k: navigate
- Space: toggle selection
- a: select all outdated
- u: toggle only outdated
- r: refresh versions for current project
- Enter: continue/confirm
- ←: back
- y/n: confirm/cancel
- q or Esc: quit

On apply, `uvlift` updates the selected requirement strings in `pyproject.toml` and then runs `uv lock` in that project directory. If `uv lock` fails, the manifest is restored.

## Scope

- Scans:
  - `[project].dependencies`
  - `[project.optional-dependencies]`
  - `[dependency-groups]`
- Skips dependencies managed by `[tool.uv.sources]`
- Supports updating only single-clause version specs:
  - `==`
  - `~=`
  - `>=`
- Fails loud on unsupported dependency specs instead of rewriting them incorrectly

Unsupported examples:

- unpinned requirements like `"httpx"`
- multi-clause ranges like `"httpx>=0.27,<1"`
- direct URL requirements
- source-managed requirements via `tool.uv.sources`

## Requirements

- Rust toolchain
- `uv`
- Python projects with `pyproject.toml` files
