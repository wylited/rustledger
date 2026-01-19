# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.4.x   | Yes |
| < 0.4   | No |

Only the latest minor version receives security fixes.

## Reporting a Vulnerability

**Do not open a public issue for security vulnerabilities.**

Instead, use GitHub's private vulnerability reporting:

1. Go to the [Security tab](https://github.com/rustledger/rustledger/security)
2. Click "Report a vulnerability"
3. Fill out the form

Or [contact us directly](https://rustledger.github.io/#contact).

## What to Include

- Description of the vulnerability
- Steps to reproduce
- Affected versions
- Potential impact
- Suggested fix (optional)

## What to Expect

| Severity | Response | Fix Timeline |
|----------|----------|--------------|
| Critical | 24 hours | 24-72 hours |
| High     | 48 hours | 7 days |
| Medium   | 7 days   | 30 days |
| Low      | 14 days  | 90 days |

We'll credit you in the release notes (unless you prefer anonymity).

## Scope

Security issues we care about:

- Remote code execution
- Path traversal (e.g., via `include` directives)
- Denial of service (e.g., parser hangs on malformed input)
- Memory safety issues
- Credential/secret exposure
- Supply chain attacks

Out of scope:

- Issues requiring physical access
- Social engineering
- Vulnerabilities in dependencies (report upstream, but let us know)

## Security Measures

### Pre-commit Hooks
- `detect-private-keys` - Blocks commits containing private keys
- `gitleaks` - Comprehensive secret scanning with pattern matching

### CI/CD Security
- `cargo-deny` - RustSec advisory database, license compliance, dependency bans
- `gitleaks-action` - Backup secret scanning in CI
- `dependency-review` - Checks PRs for vulnerable dependencies
- SBOM generation - CycloneDX format for supply chain transparency

### Code Quality
- `clippy` - Strict linting with `-D warnings`
- `rustfmt` - Consistent code formatting
- Required code review for all changes

### GitHub Security Features
- Secret scanning enabled
- Dependabot alerts and updates
- Branch protection on `main`

## Safe Harbor

We consider security research conducted in good faith to be authorized. We will not pursue legal action against researchers who:

- Act in good faith
- Avoid privacy violations
- Avoid data destruction
- Do not exploit issues beyond verification
- Report findings promptly
