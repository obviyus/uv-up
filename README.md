# UV-UP 🐍⬆️

An interactive CLI tool for updating Python dependencies in projects with `pyproject.toml` files. Think `yarn upgrade --interactive` but for Python projects using UV package management.

## Features

- 🔍 **Auto-discovery**: Recursively scans for `pyproject.toml` files in your workspace
- 📦 **Interactive selection**: Browse projects and dependencies with keyboard navigation
- 🔄 **Smart updates**: Fetches latest versions from PyPI and shows available updates
- 📊 **Clear visualization**: Tabular view with current vs latest versions and change types
- ⚡ **Fast**: Built with Bun runtime for speedy file operations and package fetching
- 🎯 **Selective updates**: Choose exactly which dependencies to update
- 🛡️ **Safe constraints**: Uses compatible release (`~=`) operator by default

## Installation

### Prerequisites

- [Bun](https://bun.sh/) runtime installed
- Python projects with `pyproject.toml` files

### Install globally

```bash
# Clone the repository
git clone https://github.com/your-username/uv-up.git
cd uv-up

# Install dependencies
bun install

# Make it globally available
bun link

# Now you can run it anywhere
uv-up
```

### Run directly

```bash
# Clone and run in one go
git clone https://github.com/your-username/uv-up.git
cd uv-up
bun install
bun index.tsx
```

## Usage

Navigate to any directory containing Python projects and run:

```bash
uv-up
```

### Navigation

The tool has three modes:

#### 1. Project Selection
- **↑↓**: Navigate between projects
- **Enter**: Select project and view dependencies
- **q**: Quit

#### 2. Dependency Management
- **↑↓**: Navigate between dependencies
- **Space**: Toggle dependency selection
- **Enter**: Continue to confirmation
- **←**: Back to project selection
- **q**: Quit

#### 3. Confirmation
- **y**: Apply selected updates
- **n** or **←**: Back to dependency selection

## Example Output

```
🐍 UV-UP - Python Dependency Updater

Select a project:
▶ my-api-project (15 deps • 3 updates available)
  data-processor (8 deps • up to date)
  ml-pipeline (22 deps • checking...)

📦 my-api-project (2 selected)

    Package                Current    Latest     Status
    ────────────────────────────────────────────────────
▶ ☐ fastapi                0.104.1    0.109.2    MINOR
  ✓ pydantic               2.5.0      2.6.1      MINOR
  ☐ uvicorn               0.24.0     0.27.0     MINOR
  ✓ requests              2.31.0     2.32.1     PATCH
```

## How It Works

1. **Discovery**: Recursively scans the current directory for `pyproject.toml` files
2. **Parsing**: Extracts project metadata and dependency information from TOML files
3. **Version fetching**: Queries PyPI API for latest package versions
4. **Comparison**: Uses semantic versioning to determine update types (major/minor/patch)
5. **Updates**: Modifies `pyproject.toml` files with new version constraints

## Configuration

UV-UP automatically handles:

- **Dependency formats**: Supports extras like `package[extra]>=1.0.0`
- **Environment markers**: Preserves conditional dependencies
- **Version constraints**: Intelligently chooses appropriate operators
- **File formatting**: Maintains original TOML structure

## Development

### Setup

```bash
git clone https://github.com/your-username/uv-up.git
cd uv-up
bun install
```

### Commands

```bash
# Run with hot reload
bun --hot index.tsx

# Lint and format
bun run lint

# Run tests
bun test

# Build for distribution
bun build index.tsx
```

### Project Structure

- `index.tsx` - Single-file React CLI application
- `examples/` - Sample Python projects for testing
- `CLAUDE.md` - Development guidelines and architecture docs

## Technologies

- **[Bun](https://bun.sh/)** - Fast JavaScript runtime and package manager
- **[Ink](https://github.com/vadimdemedes/ink)** - React for interactive command-line apps
- **[React](https://react.dev/)** - Component-based UI library
- **PyPI API** - Package version information

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Run `bun run lint` to ensure code quality
6. Submit a pull request

## License

MIT License - see LICENSE file for details

## Alternatives

- [pip-upgrader](https://github.com/simionbaws/pip-upgrader) - Similar tool for requirements.txt
- [pipenv](https://pipenv.pypa.io/) - Python dependency management with update capabilities
- [poetry](https://python-poetry.org/) - Modern dependency management for Python