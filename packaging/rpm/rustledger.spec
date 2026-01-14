%global debug_package %{nil}
# Convert version: 1.0.0~rc.18 -> 1.0.0-rc.18 for upstream
%global version_upstream %(echo %{version} | sed 's/~/-/g')

Name:           rustledger
Version:        1.0.0~rc.18
Release:        1%{?dist}
Summary:        Fast, pure Rust implementation of Beancount double-entry accounting

License:        GPL-3.0-only
URL:            https://rustledger.github.io
Source0:        https://github.com/rustledger/rustledger/archive/refs/tags/v%{version_upstream}.tar.gz

BuildRequires:  rust >= 1.75
BuildRequires:  cargo
BuildRequires:  gcc

ExclusiveArch:  x86_64 aarch64

%description
rustledger is a fast, pure Rust implementation of Beancount, the double-entry
bookkeeping language. It provides a 10x faster alternative to Python beancount
with full syntax compatibility.

Features:
- 10x faster than Python beancount
- Pure Rust - no Python dependencies
- Drop-in replacement with bean-* compatibility commands
- Full Beancount syntax support

%prep
%setup -q -n rustledger-%{version_upstream}

%build
cargo build --release --locked

%install
install -d %{buildroot}%{_bindir}

# Main binaries
install -m 755 target/release/rledger-check %{buildroot}%{_bindir}/
install -m 755 target/release/rledger-format %{buildroot}%{_bindir}/
install -m 755 target/release/rledger-query %{buildroot}%{_bindir}/
install -m 755 target/release/rledger-report %{buildroot}%{_bindir}/
install -m 755 target/release/rledger-doctor %{buildroot}%{_bindir}/
install -m 755 target/release/rledger-extract %{buildroot}%{_bindir}/
install -m 755 target/release/rledger-price %{buildroot}%{_bindir}/

# Compatibility binaries
install -m 755 target/release/bean-check %{buildroot}%{_bindir}/
install -m 755 target/release/bean-format %{buildroot}%{_bindir}/
install -m 755 target/release/bean-query %{buildroot}%{_bindir}/
install -m 755 target/release/bean-report %{buildroot}%{_bindir}/
install -m 755 target/release/bean-doctor %{buildroot}%{_bindir}/
install -m 755 target/release/bean-extract %{buildroot}%{_bindir}/
install -m 755 target/release/bean-price %{buildroot}%{_bindir}/

# License
install -Dm 644 LICENSE %{buildroot}%{_licensedir}/%{name}/LICENSE

%check
cargo test --release

%files
%license LICENSE
%{_bindir}/rledger-check
%{_bindir}/rledger-format
%{_bindir}/rledger-query
%{_bindir}/rledger-report
%{_bindir}/rledger-doctor
%{_bindir}/rledger-extract
%{_bindir}/rledger-price
%{_bindir}/bean-check
%{_bindir}/bean-format
%{_bindir}/bean-query
%{_bindir}/bean-report
%{_bindir}/bean-doctor
%{_bindir}/bean-extract
%{_bindir}/bean-price

%changelog
* Tue Jan 14 2025 rustledger <rustledger@users.noreply.github.com> - 1.0.0~rc.18-1
- Initial package
