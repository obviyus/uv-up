# UV-UP

Interactive Python dependency updater for `pyproject.toml` files.

## Installation

```bash
bun install
bun build index.tsx --compile --minify --sourcemap --outfile uv-up
cp uv-up ~/.local/bin/
```

## Usage

```bash
uv-up
```

Navigate with arrow keys, select with space, confirm with enter.

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
