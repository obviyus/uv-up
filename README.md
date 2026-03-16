# UV-UP

Interactive dependency updater for `uv` projects.

## Installation

```bash
cargo build --release
cp target/release/uv-up ~/.local/bin/
```

## Usage

Run inside a repo containing one or more `pyproject.toml` files:

```bash
uv-up
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

On apply, `uv-up` updates the selected requirement strings in `pyproject.toml` and then runs `uv lock` in that project directory. If `uv lock` fails, the manifest is restored.

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
