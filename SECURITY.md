# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.3.x   | Yes |
| < 0.3   | No |

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

## What to Expect

- **Response:** Within 7 days
- **Fix timeline:** Depends on severity, typically 30-90 days
- **Credit:** We'll credit you in the release notes (unless you prefer anonymity)

## Scope

Security issues we care about:

- Remote code execution
- Path traversal (e.g., via `include` directives)
- Denial of service (e.g., parser hangs on malformed input)
- Memory safety issues
- Credential/secret exposure

Out of scope:

- Issues requiring physical access
- Social engineering
- Vulnerabilities in dependencies (report upstream, but let us know)
