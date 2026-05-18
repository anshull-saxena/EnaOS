# Security Policy

## Supported Versions

EnaOS is in active development. Only the latest commit on the `main` branch receives security patches.

| Version | Supported |
| :--- | :--- |
| `main` (latest) | ✅ |
| Older releases | ❌ |

## Reporting a Vulnerability

EnaOS runs as a system-level daemon with access to desktop state. If you discover a security vulnerability, please report it privately.

**Do not** report security vulnerabilities via public GitHub issues.

Send details to **[ansh001kt@gmail.com](mailto:ansh001kt@gmail.com)** with:

- Component affected (enad, ena-bar, ai-runtime)
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested remediation (if any)

You will receive a response within 48 hours. We will keep you informed as the issue is triaged and resolved.

## Scope

- **enad** (Unix socket IPC, D-Bus, process lifecycle)
- **ena-bar** (GTK4 frontend, IPC client)
- **AI Runtime** (FastAPI server, Ollama integration)
- Build tooling and dependencies

## Out of Scope

- Third-party packages and their vulnerabilities (report to their maintainers)
- Compositor-level security (Wayland compositors are outside our control)

## Disclosure Policy

We believe in coordinated disclosure. We will work with you to understand the issue and develop a fix before public disclosure.
