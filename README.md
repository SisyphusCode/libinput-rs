# libinput-rs 🦀

`libinput-rs` is a high-performance, memory-safe drop-in daemon for managing Linux input devices. Written entirely in Rust, it intercepts hardware pointer and keyboard events at the kernel level and processes them through an efficient, zero-cost state machine before emitting highly tuned relative events to your display server via `/dev/uinput`.

Designed specifically to run safely alongside the existing Linux input stack, `libinput-rs` acts as a powerful hardware pre-processor that compositor libraries (like the standard `libinput.so`) seamlessly read from.

## Features

- **Memory Safe & Blazing Fast:** Leverages Rust's ownership model and the non-blocking `mio` event loop to process inputs with ultra-low latency.
- **Dynamic Hotplugging:** Actively monitors `/dev/input/` via `inotify` to instantly track and seamlessly grab new devices (mice, touchpads, keyboards) without requiring daemon restarts.
- **Advanced Gestures Engine:** 
  - Sub-pixel smooth precision translation of absolute touchpad coordinates into standard relative pointer movements.
  - Fluid **Two-Finger Natural Scrolling** using precise coordinate accumulators.
  - Responsive **Tap-to-Click** temporal recognition.
- **Disable-While-Typing (DWT):** Passively monitors connected keyboards. Instantly pauses touchpad coordinate tracking if a keystroke occurred within the last 500ms to prevent accidental palm clicks while typing.
- **Enterprise-Ready Packaging:** Includes a complete RPM `.spec` file designed for deployment on RHEL 10 and Fedora systems, gracefully co-existing with standard system libraries.

## Architecture

1. **Hardware Grabbing:** Physical pointer devices are dynamically intercepted and grabbed exclusively (`EVIOCGRAB`).
2. **Event Loop Processing:** The daemon feeds raw events into a high-speed processor utilizing the `evdev` crate.
3. **State Machine Tuning:** Coordinate delta accumulators, gesture detection, and hardware scaling factors (`0.18`) convert high-resolution hardware noise into clean, compositor-ready events.
4. **Virtual Emission:** The refined relative inputs are emitted to a `/dev/uinput` virtual device, which is natively picked up by Xorg/Wayland compositors.

## Installation

You can build and install the daemon locally using the provided RPM packaging script:

```bash
# Clone the repository
git clone https://github.com/SisyphusCode/libinput.rs.git
cd libinput.rs

# Build and create RPM
./build_package.sh

# Install the generated RPM safely alongside system packages
sudo dnf localinstall build_workspace/RPMS/x86_64/libinput-rs-*.rpm
```

## Running the Daemon

Once installed, the package configures a systemd service that launches immediately. You can manage the daemon using standard `systemctl` commands:

```bash
sudo systemctl start libinput-rs
sudo systemctl status libinput-rs
sudo systemctl enable libinput-rs
```

## Configuration

The daemon looks for its configuration at `/etc/libinput-rs/config.json`. The following options are supported:

```json
{
  "tap_to_click": true,
  "natural_scrolling": true,
  "pointer_acceleration": 1.0,
  "disable_while_typing": true
}
```

## Contributing

Pull requests are always welcome! Whether it's adding three-finger swipe support, tuning gesture thresholds, or extending hotplug capabilities, contributions help make the Linux input ecosystem stronger and more memory-safe.

---
*Built with ❤️ in Rust.*
