# Contributing to altium-designer-mcp

Thank you for your interest in contributing to altium-designer-mcp! This document provides guidelines and information for contributors.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [How to Contribute](#how-to-contribute)
    - [Reporting Bugs](#reporting-bugs)
    - [Suggesting Features](#suggesting-features)
    - [Pull Requests](#pull-requests)
- [Development Setup](#development-setup)
- [Coding Standards](#coding-standards)
- [Commit Messages](#commit-messages)
- [Testing](#testing)
- [Documentation](#documentation)

---

## Code of Conduct

This project adheres to the Contributor Covenant Code of Conduct.
By participating, you are expected to uphold this code. Please see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for details.

---

## How to Contribute

### Reporting Bugs

Before submitting a bug report:

1. Check the [existing issues](https://github.com/embedded-society/altium-designer-mcp/issues) to avoid duplicates
2. Ensure you're using the latest version
3. Collect relevant information:
    - Operating system and version
    - Rust version (`rustc --version`)
    - altium-designer-mcp version
    - Steps to reproduce
    - Expected vs actual behaviour

When submitting:

- Use the bug report template
- Provide a clear, descriptive title
- Include minimal reproduction steps

### Suggesting Features

We welcome feature suggestions! Before submitting:

1. Check [existing issues](https://github.com/embedded-society/altium-designer-mcp/issues) and
   [discussions](https://github.com/embedded-society/altium-designer-mcp/discussions) for similar ideas
2. Consider how the feature fits the project's goals
3. Think about backwards compatibility

When submitting:

- Use the feature request template
- Explain the problem you're trying to solve
- Describe your proposed solution
- Consider alternatives you've thought about

### Pull Requests

#### Before You Start

1. Open an issue first to discuss significant changes
2. Fork the repository
3. Create a feature branch from `main`
4. Make your changes following our [coding standards](#coding-standards)

#### PR Requirements

- [ ] Code compiles without warnings (`cargo build`)
- [ ] All tests pass (`cargo test`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] Clippy passes (`cargo clippy -- -D warnings`)
- [ ] Documentation is updated if needed
- [ ] CHANGELOG.md is updated for user-facing changes
- [ ] Commit messages follow [conventional commits](#commit-messages)

#### PR Process

1. Submit your PR against the `main` branch
2. Fill out the PR template completely
3. Wait for CI to pass
4. Address any review feedback
5. Once approved, a maintainer will merge

---

## Development Setup

### Prerequisites

- Rust 1.75+ (see `Cargo.toml` for minimum version, `rust-toolchain.toml` for channel)

### Setup

```bash
# Clone the repository
git clone https://github.com/embedded-society/altium-designer-mcp.git
cd altium-designer-mcp

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build the project
cargo build

# Run tests
cargo test

# Run clippy
cargo clippy -- -D warnings
```

---

## Coding Standards

### Rust Style

- Follow `rustfmt` formatting (run `cargo fmt` before committing)
- Follow `clippy` recommendations (run `cargo clippy -- -D warnings`)
- Write idiomatic Rust code
- Prefer safe Rust — `unsafe` code is forbidden (see `Cargo.toml` lints)

### Documentation

- Add rustdoc comments (`///`) to all public items
- Include examples in documentation where helpful
- Keep comments up to date with code changes

### British Spelling

Use British spelling in all documentation and user-facing text:

| American | British |
|----------|---------|
| color | colour |
| behavior | behaviour |
| organization | organisation |
| center | centre |
| license (noun) | licence |
| analyze | analyse |
| initialize | initialise |

**Note:** Code identifiers may use American spelling where it matches Rust/library conventions.

---

## Commit Messages

We use [Conventional Commits](https://www.conventionalcommits.org/). Format:

```text
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

### Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation only |
| `style` | Formatting, no code change |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `perf` | Performance improvement |
| `test` | Adding or updating tests |
| `chore` | Maintenance tasks |
| `ci` | CI/CD changes |

### Examples

```text
feat(altium): implement pad primitive encoding

fix(pcblib): correct binary format for track records

docs: update README with installation instructions

chore: update dependencies
```

### Rules

- Use imperative mood ("Add feature" not "Added feature")
- Don't capitalise the first letter of the description
- No period at the end of the subject line
- Keep the subject line under 72 characters
- Reference issues in the footer: `Fixes #123`

---

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run a specific test
cargo test test_name

# Run tests for a specific module
cargo test module_name::
```

### Writing Tests

- Write unit tests for new functionality
- Place unit tests in the same file as the code, in a `#[cfg(test)]` module
- Place integration tests in the `tests/` directory
- Use descriptive test names that explain what's being tested
- Test both success and failure cases

---

## Documentation

### Types of Documentation

| Location | Purpose |
|----------|--------|
| `README.md` | User-facing overview and quick start |
| `CONTRIBUTING.md` | This file — contributor guidelines |
| `SECURITY.md` | Security policy and vulnerability reporting |
| `CHANGELOG.md` | User-facing change history |
| Rustdoc comments | API documentation |

### Updating Documentation

- Update `README.md` for user-facing changes
- Update `CHANGELOG.md` for all notable changes
- Update rustdoc comments when changing public APIs
- Keep examples up to date and working

---

## Areas Needing Help

- **Testing**: Test with real Altium libraries across different versions
- **Documentation**: Improve user guides and API documentation
- **Edge cases**: Handle unusual primitive configurations and older file formats
- **3D models**: Improve STEP model attachment and component body support

---

## Questions?

- Open a [Discussion](https://github.com/embedded-society/altium-designer-mcp/discussions) for questions
- Check existing issues and discussions first
- Be patient — maintainers are volunteers

Thank you for contributing!
