%global debug_package %{nil}
Name:           libinput-rs
Version:        0.1.1
Release:        4%{?dist}
Summary:        A complete, drop-in Rust replacement for libinput.so

License:        MIT
URL:            https://github.com/SisyphusAeolides/libinput-rs
# Points directly to the GitHub tag tarball without the 'v' prefix
Source0:        https://github.com/SisyphusAeolides/libinput-rs/archive/%{version}/%{name}-%{version}.tar.gz

# Tell DNF we replace the package name
Provides:       libinput = 1.99.0-1
Provides:       libinput(x86-64) = 1.99.0-1
Obsoletes:      libinput < 1.31.0

# Tell DNF we provide the exact shared library and symbol versions Mutter demands
Provides:       libinput.so.10()(64bit)
Provides:       libinput.so.10(LIBINPUT_0.12.0)(64bit)
Provides:       libinput.so.10(LIBINPUT_0.14.0)(64bit)
Provides:       libinput.so.10(LIBINPUT_0.19.0)(64bit)
Provides:       libinput.so.10(LIBINPUT_0.20.0)(64bit)
Provides:       libinput.so.10(LIBINPUT_0.21.0)(64bit)
Provides:       libinput.so.10(LIBINPUT_1.1)(64bit)
Provides:       libinput.so.10(LIBINPUT_1.2)(64bit)
Provides:       libinput.so.10(LIBINPUT_1.3)(64bit)
Provides:       libinput.so.10(LIBINPUT_1.4)(64bit)
Provides:       libinput.so.10(LIBINPUT_1.5)(64bit)
Provides:       libinput.so.10(LIBINPUT_1.7)(64bit)
Provides:       libinput.so.10(LIBINPUT_1.9)(64bit)
Provides:       libinput.so.10(LIBINPUT_1.15)(64bit)
Provides:       libinput.so.10(LIBINPUT_1.19)(64bit)
Provides:       libinput.so.10(LIBINPUT_1.26)(64bit)

# Added lld and clang for ABI symbol generation
BuildRequires:  cargo, rust, systemd-rpm-macros, gcc, lld, clang
Requires:       systemd, udev
Recommends:     forged

%description
libinput-rs is a complete, drop-in Rust replacement for libinput.so with the same
C ABI and versioned symbols. It works transparently with Wayland compositors
while adding multi-touch gestures, key repeat synthesis, and memory safety.
It also includes an optional standalone evdev/uinput daemon.

%prep
%setup -q

%build
# Force LLVM linker so we don't crash when merging Rust + C version scripts
export RUSTFLAGS="-C link-arg=-fuse-ld=lld"
# Removed --bin so it builds both the daemon AND the .so library
cargo build --release --offline --frozen

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_libdir}
mkdir -p %{buildroot}%{_sysconfdir}/libinput-rs
mkdir -p %{buildroot}%{_unitdir}
mkdir -p %{buildroot}%{_datadir}/libinput-rs/forge

# Install the ABI replacement library directly to system lib path
install -m 0755 target/release/libinput.so %{buildroot}%{_libdir}/libinput.so.10

# Install the standalone daemon and configs
install -m 0755 target/release/libinput-rs %{buildroot}%{_bindir}/libinput-rs
install -m 0644 src/config.json %{buildroot}%{_sysconfdir}/libinput-rs/config.json
install -m 0644 systemd/libinput-rs.service %{buildroot}%{_unitdir}/libinput-rs.service
install -m 0644 libinput-elan-reset.service %{buildroot}%{_unitdir}/libinput-elan-reset.service
install -m 0644 forge/libinput-rs.forge.toml \
  %{buildroot}%{_datadir}/libinput-rs/forge/libinput-rs.forge.toml

%post
%systemd_post libinput-rs.service
# Refresh linker cache for the new .so.10
/sbin/ldconfig
udevadm control --reload-rules && udevadm trigger || true
# Success A: enable under forged when present (non-fatal).
if [ -d /etc/forge/units ] && [ -f %{_datadir}/libinput-rs/forge/libinput-rs.forge.toml ]; then
  cp -n %{_datadir}/libinput-rs/forge/libinput-rs.forge.toml \
    /etc/forge/units/60-libinput-rs.forge.toml 2>/dev/null || true
fi

%preun
%systemd_preun libinput-rs.service

%postun
%systemd_postun_with_restart libinput-rs.service
/sbin/ldconfig

%files
# Expose the shared library to RPM tracking
%{_libdir}/libinput.so.10
%{_bindir}/libinput-rs
%{_sysconfdir}/libinput-rs/config.json
%{_unitdir}/libinput-rs.service
%{_unitdir}/libinput-elan-reset.service
%dir %{_datadir}/libinput-rs
%dir %{_datadir}/libinput-rs/forge
%{_datadir}/libinput-rs/forge/libinput-rs.forge.toml

%changelog
* Mon Jul 20 2026 Kenny Glowner <sisyphuscode@fedoraproject.org> - 0.1.1-4
- Convert to complete libinput.so replacement with versioned C ABI symbols
- Obsolete original C libinput package
- Switch to LLD linker to resolve Rust cdylib version script conflicts

* Wed Jul 15 2026 Kenny Glowner <sisyphuscode@fedoraproject.org> - 0.1.1-3
- Include forge/ in COPR source tarball (fix missing libinput-rs.forge.toml)

* Wed Jul 15 2026 Kenny Glowner <sisyphuscode@fedoraproject.org> - 0.1.1-2
- Ship forge unit and %post install into /etc/forge/units/ when forged is present

* Tue Jul 14 2026 Kenny Glowner <sisyphuscode@fedoraproject.org> - 0.1.1-1
- Bump evdev 0.13 / mio 1 / nix 0.29; honest companion-daemon summary
- Fix forge unit ExecStart path; re-vendor

* Mon Jun 29 2026 Sisyphus <sisyphus@sisyphuslinux.org> - 0.1.0-7
- Prior COPR packaging train
