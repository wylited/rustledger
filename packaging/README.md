# Linux Package Distribution

This directory contains packaging files for Linux distributions.

## Fedora/RHEL (COPR)

**Install:**
```bash
sudo dnf copr enable rustledger/rustledger
sudo dnf install rustledger
```

**Package location:** `rpm/rustledger.spec`

**COPR project:** https://copr.fedorainfracloud.org/coprs/rustledger/rustledger/

### Setup (maintainer)

1. Create Fedora account at https://accounts.fedoraproject.org/
2. Go to https://copr.fedorainfracloud.org/
3. Create new project "rustledger" with settings:
   - Chroots: `fedora-rawhide-x86_64`, `fedora-rawhide-aarch64`, `fedora-41-*`, `fedora-40-*`
   - Build options: Enable "Internet access during builds" for cargo
4. Add a package using SCM integration:
   - Package name: `rustledger`
   - SCM type: `git`
   - Clone URL: `https://github.com/rustledger/rustledger.git`
   - Committish: Leave empty (uses latest tag)
   - Spec file: `packaging/rpm/rustledger.spec`
   - Source type: `SCM`
5. Get API token from https://copr.fedorainfracloud.org/api/
6. Add `COPR_API_TOKEN` secret to GitHub repository

## Ubuntu/Debian (PPA)

**Install:**
```bash
sudo add-apt-repository ppa:rustledger/rustledger
sudo apt update
sudo apt install rustledger
```

**Package location:** `debian/`

**PPA:** https://launchpad.net/~rustledger/+archive/ubuntu/rustledger

### Setup (maintainer)

1. Create Launchpad account at https://launchpad.net/
2. Create PPA at https://launchpad.net/~/+activate-ppa named "rustledger"
3. Generate GPG key for signing:
   ```bash
   gpg --full-generate-key  # RSA 4096, no expiry
   gpg --armor --export-secret-keys KEY_ID > launchpad.gpg
   ```
4. Upload GPG public key to keyservers and Launchpad
5. Generate SSH key for uploads:
   ```bash
   ssh-keygen -t ed25519 -C "rustledger-ci@launchpad" -f launchpad_ssh
   ```
6. Add SSH public key to Launchpad profile
7. Add GitHub secrets:
   - `LAUNCHPAD_GPG_PRIVATE_KEY`: Content of launchpad.gpg
   - `LAUNCHPAD_SSH_PRIVATE_KEY`: Content of launchpad_ssh

## Version Format

- RPM: `1.0.0~rc.18` (tilde for prereleases, sorts before 1.0.0)
- DEB: `1.0.0~rc.18-1` (tilde + debian revision)

## Migration to Official Repos

### Fedora Official
1. File Package Review Request in Bugzilla
2. Find sponsor to review
3. Package spec follows Fedora guidelines (already compatible)

### Debian Official
1. File ITP (Intent To Package) bug
2. Find Debian Developer sponsor
3. Package follows Debian Policy (already compatible)
4. Once in Debian → auto-syncs to Ubuntu
