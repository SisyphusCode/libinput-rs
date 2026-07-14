# libinput-rs

Companion Linux input preprocessor: grabs physical devices (`EVIOCGRAB`),
runs a small Rust state machine (tap-to-click, two-finger scroll, DWT), and
emits via `/dev/uinput`.

It runs **alongside** system `libinput` — it is **not** a `libinput.so`
drop-in replacement.

## Features

- Hotplug via inotify on `/dev/input`
- Sub-pixel absolute→relative mapping for touchpads
- Two-finger natural scrolling and tap-to-click
- Disable-while-typing (500ms)
- systemd unit + optional forged unit under `forge/`

## Build / install (Fedora)

```bash
git clone https://github.com/SisyphusAeolides/libinput-rs.git
cd libinput-rs
./build_package.sh
sudo dnf localinstall build_workspace/RPMS/x86_64/libinput-rs-*.rpm
sudo systemctl enable --now libinput-rs
```

COPR:

```bash
sudo dnf copr enable sisyphuscode/libinput-rs
sudo dnf install libinput-rs
```

## Configuration

`/etc/libinput-rs/config.json`:

```json
{
  "tap_to_click": true,
  "natural_scrolling": true,
  "pointer_acceleration": 1.0,
  "disable_while_typing": true
}
```

## Sisyphus / Success A

Optional under forged (not required for stock GNOME/Plasma). Unit template:
`forge/libinput-rs.service` (`ExecStart=/usr/bin/libinput-rs`).

## License

MIT
