## Description

Brief description of the changes in this PR.

## Related Issue

Fixes #(issue number)

## Type of Change

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Documentation update
- [ ] Refactoring (no functional changes)
- [ ] CI/CD changes
- [ ] Security improvement

## Security Checklist ⚠️

Since this is a credential-handling project:

- [ ] No credentials, tokens, or secrets are included in code, comments, or tests
- [ ] No credentials appear in log messages or error messages
- [ ] No credentials are exposed in MCP responses
- [ ] If handling sensitive data, `secrecy` crate is used appropriately
- [ ] Error messages don't leak sensitive information

## Testing

- [ ] I have tested these changes locally
- [ ] I have added tests that prove my fix/feature works
- [ ] New and existing tests pass (`cargo test`)

## Code Quality

- [ ] Code compiles without warnings (`cargo build`)
- [ ] Clippy passes (`cargo clippy -- -D warnings`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] Documentation is updated if needed
- [ ] CHANGELOG.md is updated for user-facing changes

## Screenshots / Logs

If applicable, add screenshots or relevant log output (with credentials redacted).

## Additional Notes

Any additional information reviewers should know.
