%global debug_package %{nil}

Name:           rustledger
Version:        1.0.0~rc.18
Release:        1%{?dist}
Summary:        Fast, pure Rust implementation of Beancount double-entry accounting

License:        GPL-3.0-only
URL:            https://rustledger.github.io
Source0:        https://github.com/rustledger/rustledger/archive/refs/tags/v1.0.0-rc.18.tar.gz

BuildRequires:  rust >= 1.75
BuildRequires:  cargo
BuildRequires:  gcc

ExclusiveArch:  x86_64 aarch64

%description
rustledger is a fast, pure Rust implementation of Beancount, the double-entry
bookkeeping language. It provides a 10x faster alternative to Python beancount
with full syntax compatibility.

%prep
%setup -q -n rustledger-1.0.0-rc.18

%build
cargo build --release

%install
install -d %{buildroot}%{_bindir}

install -m 755 target/release/rledger-check %{buildroot}%{_bindir}/
install -m 755 target/release/rledger-format %{buildroot}%{_bindir}/
install -m 755 target/release/rledger-query %{buildroot}%{_bindir}/
install -m 755 target/release/rledger-report %{buildroot}%{_bindir}/
install -m 755 target/release/rledger-doctor %{buildroot}%{_bindir}/
install -m 755 target/release/rledger-extract %{buildroot}%{_bindir}/
install -m 755 target/release/rledger-price %{buildroot}%{_bindir}/

install -m 755 target/release/bean-check %{buildroot}%{_bindir}/
install -m 755 target/release/bean-format %{buildroot}%{_bindir}/
install -m 755 target/release/bean-query %{buildroot}%{_bindir}/
install -m 755 target/release/bean-report %{buildroot}%{_bindir}/
install -m 755 target/release/bean-doctor %{buildroot}%{_bindir}/
install -m 755 target/release/bean-extract %{buildroot}%{_bindir}/
install -m 755 target/release/bean-price %{buildroot}%{_bindir}/

%files
%license LICENSE
%{_bindir}/rledger-*
%{_bindir}/bean-*

%changelog
* Tue Jan 14 2025 rustledger <rustledger@users.noreply.github.com> - 1.0.0~rc.18-1
- Initial package
