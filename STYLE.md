# Style Guide

Code style guidelines for altium-designer-mcp contributors.

## Rust Code Style

### Formatting

- Run `cargo fmt` before committing
- Use 4 spaces for indentation (rustfmt default)
- Maximum line length: 100 characters

### Linting

- All code must pass `cargo clippy -- -D warnings`
- No `unsafe` code allowed (enforced via `Cargo.toml` lints)

### Naming Conventions

| Item            | Convention      | Example                    |
|-----------------|-----------------|----------------------------|
| Modules         | `snake_case`    | `ipc_calculator`           |
| Types/Structs   | `PascalCase`    | `PackageDimensions`        |
| Functions       | `snake_case`    | `calculate_footprint`      |
| Constants       | `SCREAMING_CASE`| `DEFAULT_COURTYARD_MARGIN` |
| Variables       | `snake_case`    | `pad_width`                |

### Documentation

- Add rustdoc comments (`///`) to all public items
- Include examples in documentation where helpful
- Document panic conditions with `# Panics` section
- Document errors with `# Errors` section

### Error Handling

- Use `thiserror` for custom error types
- Prefer `Result` over `Option` when failure should be communicated
- Provide meaningful error messages

### Testing

- Write unit tests for new functionality
- Place unit tests in the same file, in a `#[cfg(test)]` module
- Use descriptive test names: `test_<function>_<scenario>_<expected>`
- Test both success and failure cases

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`, `ci`

## IPC-7351B Specific

- All dimensions in millimetres (mm)
- Use `f64` for dimensional values
- Document units in variable names or comments when not obvious
- Follow IPC naming conventions for package types
