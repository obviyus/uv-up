# UVLIFT

Interactive dependency updater for `uv` projects.

## Installation

Published package:

```bash
uvx uvlift
```

Supported prebuilt platforms:

- macOS arm64
- Linux x86_64
- Linux arm64
- Windows x86_64

If a wheel is not available for your platform, `uvx` falls back to building from source, which requires a Rust toolchain.

From source:

```bash
cargo build --release
cp target/release/uvlift ~/.local/bin/
```

`uvx` installs Python packages, not Cargo crates. This repo uses `maturin` to package the Rust binary for that flow.

Local smoke test:

```bash
uvx --from . uvlift
```

## Usage

Run inside a repo containing one or more `pyproject.toml` files:

```bash
uvlift
```

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
