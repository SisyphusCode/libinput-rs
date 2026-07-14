%global debug_package %{nil}
Name:           libinput-rs
Version:        0.1.1
Release:        1%{?dist}
Summary:        Companion Linux input preprocessor daemon (evdev + uinput)

License:        MIT
URL:            https://github.com/SisyphusAeolides/libinput-rs
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo, rust, systemd-rpm-macros, gcc
Requires:       systemd, udev

%description
libinput-rs grabs physical input devices, applies a small gesture/DWT state
machine, and emits refined events via /dev/uinput. It is an optional companion
that runs alongside the system libinput stack — not a libinput.so ABI replacement.

%prep
%setup -q

%build
cargo build --release --offline --frozen

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_sysconfdir}/libinput-rs
mkdir -p %{buildroot}%{_unitdir}

install -m 0755 target/release/libinput-rs %{buildroot}%{_bindir}/libinput-rs
install -m 0644 src/config.json %{buildroot}%{_sysconfdir}/libinput-rs/config.json
install -m 0644 systemd/libinput-rs.service %{buildroot}%{_unitdir}/libinput-rs.service
install -m 0644 libinput-elan-reset.service %{buildroot}%{_unitdir}/libinput-elan-reset.service

%post
%systemd_post libinput-rs.service
udevadm control --reload-rules && udevadm trigger || true

%preun
%systemd_preun libinput-rs.service

%postun
%systemd_postun_with_restart libinput-rs.service

%files
%{_bindir}/libinput-rs
%{_sysconfdir}/libinput-rs/config.json
%{_unitdir}/libinput-rs.service
%{_unitdir}/libinput-elan-reset.service

%changelog
* Tue Jul 14 2026 Kenny Glowner <sisyphuscode@fedoraproject.org> - 0.1.1-1
- Bump evdev 0.13 / mio 1 / nix 0.29; honest companion-daemon summary
- Fix forge unit ExecStart path; re-vendor

* Mon Jun 29 2026 Sisyphus <sisyphus@sisyphuslinux.org> - 0.1.0-7
- Prior COPR packaging train
