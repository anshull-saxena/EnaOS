# Contributing to EnaOS

Thank you for your interest in EnaOS. This is an early-stage project, and contributions of all kinds are welcome.

## Before You Start

EnaOS is a **polyglot monorepo** with three main components:

| Component | Language | Location |
| :--- | :--- | :--- |
| **enad** (system daemon) | Rust | `runtimes/enad/` |
| **AI Runtime** | Python | `runtimes/ai-runtime/` |
| **Ena Bar** (native shell) | Rust (GTK4) | `shell/ena-bar/` |

Each component has its own `Cargo.toml` or `pyproject.toml`. There is no top-level build system — build each component individually.

## How to Contribute

### Reporting Bugs

Open an issue using the [Bug Report template](.github/ISSUE_TEMPLATE/bug_report.md). Include:

- Your Linux distribution and compositor (GNOME, Sway, Hyprland)
- Component and version (enad, ena-bar, ai-runtime)
- Steps to reproduce, expected behavior, actual behavior
- Relevant logs from `enad` or `ena-bar` (use `RUST_LOG=debug`)

### Suggesting Features

Open an issue using the [Feature Request template](.github/ISSUE_TEMPLATE/feature_request.md). Describe the problem you're solving and your proposed approach.

### Pull Requests

1. Fork the repository and create a branch from `main`.
2. If your change touches the daemon or bar, ensure `cargo build` passes cleanly.
3. If your change touches the AI runtime, ensure `python3 -c "import ast; ast.parse(open('src/...').read())"` passes.
4. Open a pull request using the [PR template](.github/PULL_REQUEST_TEMPLATE.md).
5. Keep changes focused — one PR per feature or fix.
6. Write clear commit messages in the imperative tense ("Fix battery parsing", not "Fixed battery parsing").

## Development Conventions

- **Rust:** Use `cargo fmt` and fix all `cargo clippy` warnings before committing.
- **Python:** Use `ruff` for linting and formatting.
- **No simulated state:** The bar is a thin renderer — all business logic lives in enad.
- **Graceful degradation:** If a subsystem is unavailable, log and continue. enad must never crash.
- **Compositor support:** Window tracking should work on GNOME, Sway, and Hyprland.

## Code of Conduct

All contributors must follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
