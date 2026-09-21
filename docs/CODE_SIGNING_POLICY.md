# Code signing policy

The Windows binaries of altium-designer-mcp are to be signed through
[SignPath Foundation](https://signpath.org)'s programme for open-source projects.
**The application is pending: until it is approved, the binaries are unsigned**, and
Windows SmartScreen warns on first run. Every release already carries a signed SLSA build
provenance attestation, which `gh attestation verify` checks (see the release notes).

When signing is enabled, signed releases carry this attribution:

> Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by
> [SignPath Foundation](https://signpath.org)

## What is signed

Only binaries built by this repository's release workflow
(`.github/workflows/release.yml`) from its own source code, at a signed tag. Every signed
binary carries the product name `altium-designer-mcp` and the release's version in its
Windows version resource (embedded by `build.rs`). Each release is signed only after a
person approves the signing request.

## Team roles

| Role | Members |
|------|---------|
| Committers and reviewers | [Members of The Embedded Society](https://github.com/orgs/embedded-society/people) |
| Approvers | [Owners of The Embedded Society](https://github.com/orgs/embedded-society/people?query=role%3Aowner) |

Every change reaches `main` through a pull request that a repository ruleset requires to
be reviewed and to pass CI; release tags must be signed and can be created only by
organisation administrators. The organisation requires two-factor authentication of all
members.

## Privacy

This program will not transfer any information to other networked systems unless
specifically requested by the user or the person installing or operating it. It reads
and writes only the library files in the folders it is given; see the
[privacy policy](../README.md#privacy-policy).
