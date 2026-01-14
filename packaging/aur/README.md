# AUR Packages for rustledger

This directory contains PKGBUILD files for the Arch User Repository (AUR).

## Packages

- **rustledger-bin** - Pre-built binaries (recommended for most users)
- **rustledger** - Build from source

## Initial Setup

1. Create an AUR account at https://aur.archlinux.org/register

2. Add your SSH key to your AUR account

3. Create the AUR packages:

```bash
# Clone empty AUR repos (first time only)
git clone ssh://aur@aur.archlinux.org/rustledger-bin.git /tmp/rustledger-bin
git clone ssh://aur@aur.archlinux.org/rustledger.git /tmp/rustledger

# Copy PKGBUILDs and generate .SRCINFO
cd /tmp/rustledger-bin
cp /path/to/rustledger/packaging/aur/rustledger-bin/PKGBUILD .
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "Initial release"
git push

cd /tmp/rustledger
cp /path/to/rustledger/packaging/aur/rustledger/PKGBUILD .
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "Initial release"
git push
```

## Updating Packages

When releasing a new version:

1. Update `pkgver` in both PKGBUILDs (use underscore for prerelease: `1.0.0_rc.18`)

2. Update checksums:
   ```bash
   # For rustledger-bin, get checksums from release
   curl -sL "https://github.com/rustledger/rustledger/releases/download/v1.0.0-rc.18/rustledger-v1.0.0-rc.18-x86_64-unknown-linux-gnu.tar.gz" | sha256sum
   curl -sL "https://github.com/rustledger/rustledger/releases/download/v1.0.0-rc.18/rustledger-v1.0.0-rc.18-aarch64-unknown-linux-gnu.tar.gz" | sha256sum

   # For rustledger (source), get checksum from GitHub
   curl -sL "https://github.com/rustledger/rustledger/archive/refs/tags/v1.0.0-rc.18.tar.gz" | sha256sum
   ```

3. Regenerate .SRCINFO and push:
   ```bash
   makepkg --printsrcinfo > .SRCINFO
   git add PKGBUILD .SRCINFO
   git commit -m "Update to 1.0.0-rc.18"
   git push
   ```

## Testing Locally

```bash
# Test build without installing
makepkg -s

# Test build and install
makepkg -si

# Clean up
makepkg -c
```

## Version Format

AUR uses underscores for version separators, not hyphens:
- Release version: `1.0.0` → pkgver=`1.0.0`
- Prerelease: `1.0.0-rc.18` → pkgver=`1.0.0_rc.18`

The `_pkgver` variable in PKGBUILD converts back to hyphen format for download URLs.
