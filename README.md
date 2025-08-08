# UV-UP

Interactive Python dependency updater for `pyproject.toml` files.

## Installation

```bash
bun install
bun run build
cp uv-up ~/.local/bin/
```

Or you can grab an executable for your platfrom from: https://github.com/obviyus/uv-up/releases

## Usage

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

## Example

```
🐍 UV-UP - Python Dependency Updater

Select a project:
▶ my-api-project (15 deps • 3 updates available)
  data-processor (8 deps • up to date)

📦 my-api-project (2 selected)

    Package      Current    Latest     Status
    ──────────────────────────────────────────
▶ ✓ fastapi      0.104.1    0.109.2    MINOR
  ✓ pydantic     2.5.0      2.6.1      MINOR
  ☐ uvicorn      0.24.0     0.27.0     MINOR
```

## Requirements

- [Bun](https://bun.sh/) runtime
- Python projects with `pyproject.toml` files

Notes:

- `uv-up` preserves extras and environment markers (PEP 508) and respects the original quote style when updating.
- Non-PyPI specs (URLs, file: paths, direct VCS) are detected and left untouched.
