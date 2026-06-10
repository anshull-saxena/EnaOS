# Contributing to EnaOS

Thank you for your interest in EnaOS. This is an early-stage project, and contributions of all kinds are welcome.

## Before You Start

EnaOS is a **polyglot monorepo** with three main components:

| Component | Language | Location | Status |
| :--- | :--- | :--- | :--- |
| **enad** (system daemon) | Rust | `runtimes/enad/` | ✅ Release 0.1.0 |
| **Ena Bar** (native shell) | Rust (GTK4) | `shell/ena-bar/` | ✅ Release 0.1.0 |
| **AI Runtime** | Python (FastAPI) | `runtimes/ai-runtime/` | ✅ Release 0.1.0 |

Each component has its own `Cargo.toml` or `pyproject.toml`. There is no top-level build system — build each component individually.

## Quick Start

```bash
# Clone
git clone https://github.com/anshull-saxena/EnaOS.git
cd EnaOS

# Build daemon
cd runtimes/enad && cargo build --release

# Build GTK4 bar
cd shell/ena-bar && cargo build --release

# Install AI runtime
cd runtimes/ai-runtime && pip install -r requirements.txt

# Run tests
cd runtimes/enad && cargo test
```

## Development Setup

### Environment Variables

```bash
# Verbose logging
RUST_LOG=debug

# AI Runtime configuration
OLLAMA_HOST=http://localhost:11434
OPENAI_API_KEY=sk-...  # Optional: cloud fallback
```

### Running for Development

```bash
# Terminal 1: enad daemon
cd runtimes/enad
cargo run --release -- --socket /tmp/enad.sock

# Terminal 2: AI Runtime (optional, requires Ollama)
cd runtimes/ai-runtime
python3 -m src.main

# Terminal 3: GTK4 bar (requires Wayland compositor)
cd shell/ena-bar
cargo run --release -- --socket-path /tmp/enad.sock
```

### Testing

```bash
# Run all tests (71+ tests for enad)
cd runtimes/enad && cargo test

# Test with output
cargo test -- --nocapture

# Test a specific test
cargo test test_cmd_execute

# Check compilation without running
cargo check

# Lint
cargo clippy

# Format
cargo fmt
```

### Debugging Common Issues

| Symptom | Likely Cause | Fix |
|---------|-------------|-----|
| `Address already in use` | Stale socket from previous run | `rm -f /tmp/enad.sock` |
| `gtk4-layer-shell` not found | Missing dev package | `apt install libgtk4-layer-shell-dev` |
| Integration tests fail with EAGAIN | macOS non-blocking socket | Tests use tokio async — should work |
| Bar doesn't appear on screen | Compositor doesn't support layer-shell | Use Sway or Hyprland, or install GNOME extension |
| AI Runtime won't start | Ollama not running | `ollama serve` in another terminal |

### Debug Logs

```bash
# enad verbose
RUST_LOG=debug cargo run --release -- --socket /tmp/enad.sock

# ena-bar verbose
cargo run --release -- --socket-path /tmp/enad.sock --verbose

# Test with debug output
RUST_LOG=debug cargo test -- --nocapture
```

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
2. Ensure `cargo build` passes for your component.
3. Ensure `cargo test` passes for enad (not all tests may pass on macOS).
4. Open a pull request using the [PR template](.github/PULL_REQUEST_TEMPLATE.md).
5. Keep changes focused — one PR per feature or fix.
6. Write clear commit messages in the imperative tense.

### CI Pipeline

All PRs are checked by GitHub Actions:
- **enad**: `cargo build`, `cargo test`, `cargo clippy`, `cargo fmt`
- **ena-bar**: `cargo build`, `cargo clippy`

Check the [Actions tab](https://github.com/anshull-saxena/EnaOS/actions) for CI results.

## Development Conventions

- **Rust:** Use `cargo fmt` and fix all `cargo clippy` warnings before committing.
- **Python:** Use `ruff` for linting and formatting.
- **No simulated state:** The bar is a thin renderer — all business logic lives in enad.
- **Graceful degradation:** If a subsystem is unavailable, log and continue. enad must never crash.
- **Compositor support:** Window tracking should work on GNOME, Sway, and Hyprland.
- **IPC protocol:** Wire format is `{"id": ..., "kind": {"type": ..., "body": ...}}`. Never flatten `kind` to top-level fields.

## Release Process

EnaOS uses trunk-based development. Releases are tagged from `main`:

```bash
# Tag a new release
git tag -a v0.1.0-developer-preview -m "EnaOS Developer Preview 0.1"
git push origin v0.1.0-developer-preview

# A GitHub release will be created by CI on tag push
# See .github/workflows/ci.yml for automated release tasks
```

Release assets needed:
- Release notes (generated from CHANGELOG.md)
- Screenshots of the bar in action
- Demo GIFs showing key workflows
- Build artifacts (optional, v0.1.0: source build only)

## Release Cadence

| Version | Type | Cadence | Status |
|---------|------|---------|--------|
| 0.1.x | Developer Preview | On demand | Current |
| 0.2.x | Beta | TBD | Planned |
| 1.0.0 | Stable | TBD | Future |

## Code of Conduct

All contributors must follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
