# Security Policy

## Supported Versions

| Version | Supported              |
| ------- | ---------------------- |
| 0.x.x   | Yes (development) |

Once we reach v1.0, we will maintain security updates for the current major version and one previous major version.

## Reporting a Vulnerability

**Please do NOT report security vulnerabilities through public GitHub issues.**

### How to Report

1. **Preferred:** Use [GitHub Security Advisories](https://github.com/embedded-society/altium-designer-mcp/security/advisories/new) to report vulnerabilities privately.

2. **Alternative:** Email the repository owner directly at <matejg03@gmail.com>.

### What to Include

When reporting a vulnerability, please include:

- A clear description of the vulnerability
- Steps to reproduce the issue
- Potential impact assessment
- Any suggested fixes (optional but appreciated)

### What Qualifies as a Security Issue

| Severity | Examples |
|----------|----------|
| **High** | Arbitrary file access outside configured library paths |
| **High** | Path traversal vulnerabilities |
| **Medium** | Denial of service vulnerabilities |
| **Medium** | Information disclosure |
| **Low** | Issues requiring local access or unlikely scenarios |

### Response Timeline

| Action | Timeframe |
|--------|----------|
| Initial acknowledgement | Within 48 hours |
| Preliminary assessment | Within 1 week |
| Fix development | Depends on severity and complexity |
| Security advisory publication | After fix is available |

### What to Expect

1. **Acknowledgement:** We will acknowledge receipt of your report within 48 hours.

2. **Communication:** We will keep you informed of our progress and may ask for additional information.

3. **Credit:** Unless you prefer to remain anonymous, we will credit you in our security advisory and release notes.

4. **Disclosure:** We follow responsible disclosure practices. We ask that you give us reasonable time to address the issue before any public disclosure.

## Security Best Practices for Users

### Configuration

The MCP server configuration file should be kept secure:

**Config file location:**

- **Linux/macOS:** `~/.altium-designer-mcp/config.json`
- **Windows:** `%USERPROFILE%\.altium-designer-mcp\config.json`

See `config/example-config.json` for the full structure.

### Library Path Security

- Only configure library paths to directories you trust
- Avoid using paths that contain untrusted user content
- Use absolute paths when possible

## Acknowledgements

We thank the security researchers and community members who help keep this project secure.

---

*This security policy was last updated on 2025-01-17.*
