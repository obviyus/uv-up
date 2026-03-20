# uvlift

An interactive TUI for updating dependencies in [uv](https://docs.astral.sh/uv/)-managed Python projects.

Point it at a repo, pick what to update, and `uvlift` rewrites `pyproject.toml` and runs `uv lock` for you. If locking fails, it rolls back.

![demo](https://raw.githubusercontent.com/obviyus/uvlift/master/assets/demo.jpg)

## Installation

```bash
# run directly
uvx uvlift

# or install as a tool
uv tool install uvlift
```

Prebuilt wheels for macOS arm64, Linux x86_64/arm64, and Windows x86_64. Other platforms build from source (requires a Rust toolchain).

## Usage

```bash
uvlift
```

Recursively finds all `pyproject.toml` files under the current directory. If there's only one project, it skips straight to the dependency list.

### What it scans

- `[project].dependencies`
- `[project.optional-dependencies]`
- `[dependency-groups]` (PEP 735)

Dependencies in `[tool.uv.sources]` are skipped automatically.

### Supported version specifiers

`==`, `~=`, and `>=` — single-clause only.

Multi-clause ranges (`>=0.27,<1`), unpinned deps, direct URLs, and source-managed deps are marked **UNSUPPORTED** and left untouched.

## Building from source

```bash
cargo build --release
cp target/release/uvlift ~/.local/bin/
```

The repo uses [maturin](https://www.maturin.rs/) (`bindings = "bin"`) to package the Rust binary into a Python wheel, which is what makes `uvx uvlift` work without a Rust toolchain.

```bash
# local smoke test
uvx --from . uvlift
```

## Requirements

- `uv` on `PATH`
- A terminal with cursor-position query support
