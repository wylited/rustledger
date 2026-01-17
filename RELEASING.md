# Releasing rustledger

This document describes how to release a new version of rustledger.

## Overview

Releases are automated via GitHub Actions:

1. **You:** Push a version tag (`v0.3.0`)
2. **`release-build.yml`:** Builds all binaries, creates GitHub Release
3. **`release-publish.yml`:** Distributes to package managers

## Prerequisites

- You must be on `main` branch
- All CI checks must be passing
- `cargo-release` installed: `cargo install cargo-release`

## Release Process

### 1. Create version bump PR

Since `main` is protected, you can't push directly. Create a PR:

```bash
# Ensure you're on main and up to date
git checkout main && git pull

# Create release branch
git checkout -b chore/release-vX.Y.Z

# Bump version (updates all Cargo.toml files)
cargo release minor --no-publish --no-push --no-tag --execute
# Use: patch (0.2.0 → 0.2.1), minor (0.2.0 → 0.3.0), or major (0.2.0 → 1.0.0)

# Push and create PR
git push -u origin chore/release-vX.Y.Z
gh pr create --title "chore: release vX.Y.Z" --body "Bump version to X.Y.Z"
```

### 2. Merge the PR

Wait for CI to pass, then merge via the merge queue.

### 3. Create and push the tag

```bash
git checkout main && git pull
git tag vX.Y.Z
git push origin vX.Y.Z
```

**Important:** Do NOT manually create a GitHub Release. The workflow does this automatically after builds complete.

### 4. Monitor the release

```bash
# Watch the release build
gh run watch

# Or check specific workflows
gh run list --limit 5
```

The release takes ~30-45 minutes to build all platforms.

## What Gets Released

### Binaries (10 targets)

| Platform | Target |
|----------|--------|
| Linux x64 | `x86_64-unknown-linux-gnu` |
| Linux x64 (static) | `x86_64-unknown-linux-musl` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` |
| Linux ARM64 (static) | `aarch64-unknown-linux-musl` |
| macOS x64 | `x86_64-apple-darwin` |
| macOS ARM64 | `aarch64-apple-darwin` |
| Windows x64 | `x86_64-pc-windows-msvc` |
| Windows ARM64 | `aarch64-pc-windows-msvc` |

### Package Managers

| Channel | Registry/Repo |
|---------|---------------|
| crates.io | `rustledger`, `rustledger-*` |
| npm | `@rustledger/wasm`, `@rustledger/mcp-server` |
| Docker | `ghcr.io/rustledger/rustledger` |
| Homebrew | `rustledger/homebrew-rustledger` |
| Scoop | `rustledger/scoop-rustledger` |
| COPR | `copr.fedoraproject.org/rustledger` |

## Troubleshooting

### Release Publish failed

If `release-publish.yml` fails but `release-build.yml` succeeded:

```bash
# Re-run just the publish workflow
gh run rerun <run-id>
```

### Race condition (publish before build)

This happens if you manually create a GitHub Release before builds complete. The workflow creates the release automatically—don't create it manually.

### Protected branch prevents push

Use the PR workflow described above instead of pushing directly to main.

### cargo-release not found

```bash
cargo install cargo-release
```

## Configuration

### `release.toml`

```toml
shared-version = true       # All crates share same version
tag-prefix = "v"            # Tags: v0.3.0
publish = false             # CI handles crates.io
push = true                 # Default; we override with --no-push for protected branches
allow-branch = ["main"]     # Only release from main
```

### Workflow files

- `.github/workflows/release-build.yml` - Builds binaries, creates release
- `.github/workflows/release-publish.yml` - Distributes to package managers

## Version Numbering

We follow [Semantic Versioning](https://semver.org/):

- **Major** (1.0.0): Breaking API changes
- **Minor** (0.2.0): New features, backward compatible
- **Patch** (0.1.1): Bug fixes, backward compatible
