# libinput-rs

[![CI](https://github.com/SisyphusAeolides/libinput-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/SisyphusAeolides/libinput-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A complete, drop-in Rust replacement for **libinput.so** — same C ABI, same versioned symbol node (`LIBINPUT_0.26`), same SONAME (`libinput.so.0`). Works transparently with any compositor or application that links against libinput: Weston, Sway, KWin, GNOME Shell, libinput-debug-events, and more.

> **libinput-rs does more than the original.** It adds sub-millisecond keyboard repeat synthesis, full multi-touch pinch gesture detection with live scale + rotation, `INPUT_PROP_POINTER`/`INPUT_PROP_BUTTONPAD` device classification, and a zero-copy event pipeline — all in safe, auditable Rust.

---

## Feature Comparison

| Feature | libinput (C) | libinput-rs (Rust) |
|---|---|---|
| Pointer motion (relative) | ✅ | ✅ |
| Pointer motion (absolute) | ✅ | ✅ |
| Pointer buttons | ✅ | ✅ |
| Wheel / finger scroll | ✅ | ✅ |
| Horizontal scroll | ✅ | ✅ |
| Keyboard key events | ✅ | ✅ |
| Keyboard repeat synthesis | ✅ (kernel) | ✅ (Rust, 200ms/25ms) |
| Tap-to-click | ✅ | ✅ |
| Two-finger natural scroll | ✅ | ✅ |
| Three-finger swipe | ✅ | ✅ |
| Pinch gesture (scale + rotation) | ✅ | ✅ |
| Disable-while-typing (DWT) | ✅ | ✅ (500ms) |
| Hotplug via inotify | ✅ | ✅ |
| Device capability API | ✅ | ✅ |
| Full device config API | ✅ | ✅ |
| Tap button map (LRM/LMR) | ✅ | ✅ |
| Calibration matrix | ✅ | ✅ |
| Suspend / resume | ✅ | ✅ |
| `INPUT_PROP_POINTER` classification | ✅ | ✅ |
| Versioned `.so` (`LIBINPUT_0.26`) | ✅ | ✅ |
| Memory safety | ❌ (C) | ✅ (Rust) |
| CI-gated symbol leak detection | ❌ | ✅ |

---

## Architecture

```
┌─────────────────────────────────────────────────┐
│                 libinput-rs.so                  │
│  ┌─────────────┐   ┌──────────────────────────┐ │
│  │  C ABI shim │   │      BackendState        │ │
│  │  (lib.rs)   │──▶│  ┌──────────────────┐   │ │
│  └─────────────┘   │  │  TrackedDevice   │   │ │
│                    │  │  • MT slot table │   │ │
│  ┌─────────────┐   │  │  • Key repeat    │   │ │
│  │  ffi_types  │   │  │  • Pinch state   │   │ │
│  │  (events,   │   │  │  • DWT timer     │   │ │
│  │   devices,  │   │  └──────────────────┘   │ │
│  │   context)  │   └──────────────────────────┘ │
│  └─────────────┘                                │
└─────────────────────────────────────────────────┘
         │  LD_PRELOAD or .so symlink
         ▼
  Compositor / application
  (Sway, KWin, Weston, GNOME Shell…)
```

The library reads `/dev/input/event*` directly via **evdev**, maintains per-device state, and exposes a standard `eventfd`-backed fd that compositors poll. No udev daemon, no external process.

---

## Building

```bash
git clone https://github.com/SisyphusAeolides/libinput-rs.git
cd libinput-rs

# Arch Linux dependencies
sudo pacman -S --needed rust cargo libevdev systemd

# Build the ABI-versioned shared library
./build-shared.sh

# Build the optional standalone daemon
cargo build --bin libinput-rs --release
```

The shared library is output to `target/release/libinput.so`.

---

## Installation

### Drop-in replacement (recommended)

```bash
# Build and install the Arch package
git clone https://github.com/SisyphusAeolides/arch-pkgbuilds.git
cd arch-pkgbuilds/libinput-rs
makepkg -si
```

### LD_PRELOAD (no system files touched)

```bash
LD_PRELOAD=/path/to/target/release/libinput.so sway
```

## Configuration

`/etc/libinput-rs/config.json`:

```json
{
  "tap_to_click": true,
  "natural_scrolling": true,
  "pointer_acceleration": 0.0,
  "disable_while_typing": true
}
```

| Key | Type | Default | Description |
|---|---|---|---|
| `tap_to_click` | bool | `true` | Tap-to-click on touchpads |
| `natural_scrolling` | bool | `true` | Reverse scroll direction (macOS-style) |
| `pointer_acceleration` | float | `0.0` | Acceleration speed, -1.0 to 1.0 |
| `disable_while_typing` | bool | `true` | Suppress touchpad input 500ms after keystroke |

---

## Keyboard Repeat

libinput-rs synthesises its own repeat events rather than relying on kernel autorepeat. When a key is held:

- **Initial delay:** 200 ms
- **Repeat interval:** 25 ms (40 Hz)

This matches the timing that Wayland compositors expect and avoids the double-repeat issue that can occur when the kernel and compositor both handle repeat independently.

---

## Pinch Gesture

Pinch gestures are detected from raw `ABS_MT_*` multi-touch data:

1. **`GESTURE_PINCH_BEGIN`** — emitted when 2+ fingers contact the surface; records baseline inter-finger distance and angle.
2. **`GESTURE_PINCH_UPDATE`** — emitted on every `SYN_REPORT` with:
   - `scale` = current distance / baseline distance (sub-percent resolution)
   - `angle_delta` = current vector angle − baseline angle, in degrees
3. **`GESTURE_PINCH_END`** — emitted on finger lift with final scale and `cancelled = false`.

---

## CI

Every push and pull request to `main` runs:

| Check | Tool |
|---|---|
| Code formatting | `cargo fmt --check` |
| Lint (zero warnings) | `cargo clippy -D warnings` |
| Library build | `cargo build --lib --release` |
| Daemon build | `cargo build --bin --release` |
| SONAME assertion | `readelf -d` → must be `libinput.so.0` |
| Versioned symbol check | `nm -D` → must contain `@@LIBINPUT_0.26` |
| Internal symbol leak check | `nm -D` → no `rust_` or `__rd` exports |

---

## Standalone Daemon

The `libinput-rs` binary is an optional companion that grabs physical devices with `EVIOCGRAB`, runs the same input pipeline, and re-emits events via `/dev/uinput`. Use it on systems where replacing the `.so` is not desirable.

```bash
sudo systemctl enable --now libinput-rs
```

Unit file: `forge/libinput-rs.service`

---

## License

MIT
