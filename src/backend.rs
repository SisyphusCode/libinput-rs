//! Evdev backend for the libinput-rs shared library.
//!
//! BackendState owns every open DeviceWrapper and provides a single
//! drain() call that reads all pending kernel events, applies the
//! same motion/scroll/DWT/tap logic from device.rs, and appends
//! finished LibinputEvents to a caller-supplied queue.

use std::collections::HashMap;
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use evdev::{Device, EventType, InputEvent, AbsoluteAxisCode, RelativeAxisCode, KeyCode};
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use std::os::unix::io::AsFd;

use crate::config::InputConfig;
use crate::ffi_types::{
    EventPayload, GestureEvent, KeyboardKeyEvent, LibinputDevice, LibinputEvent,
    LibinputEventType, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    TouchEvent, LibinputContext,
};

// ---------------------------------------------------------------------------
// Per-device tracking state (mirrors DeviceWrapper fields)
// ---------------------------------------------------------------------------

struct TrackedDevice {
    device:       Device,
    path:         PathBuf,
    is_absolute:  bool,
    is_keyboard:  bool,
    // pointer tracking
    touch_active:   bool,
    touch_fingers:  u32,
    last_x:         Option<i32>,
    last_y:         Option<i32>,
    current_dx:     i32,
    current_dy:     i32,
    remainder_x:    f32,
    remainder_y:    f32,
    // tap
    touch_start_time: Option<Instant>,
    tap_emitted:      bool,
    // DWT
    last_typing_time:   Option<Instant>,
    ctrl_pressed:       bool,
    alt_pressed:        bool,
    last_movement_time: Option<Instant>,
    active_click_button: Option<u16>,
    // device pointer (raw borrow — context owns allocation)
    lib_device: *mut LibinputDevice,
}

unsafe impl Send for TrackedDevice {}

impl TrackedDevice {
    fn new(mut device: Device, path: PathBuf, lib_device: *mut LibinputDevice) -> Self {
        let is_absolute = device.supported_events().contains(EventType::ABSOLUTE);
        let is_keyboard = device.supported_events().contains(EventType::KEY)
            && device.supported_keys().is_some_and(|k| k.contains(KeyCode::KEY_A));
        // Grab pointer devices; keyboards are opened read-only for DWT
        if is_absolute {
            let _ = device.grab();
        }
        Self {
            device, path, is_absolute, is_keyboard,
            touch_active: false, touch_fingers: 0,
            last_x: None, last_y: None,
            current_dx: 0, current_dy: 0,
            remainder_x: 0.0, remainder_y: 0.0,
            touch_start_time: None, tap_emitted: false,
            last_typing_time: None,
            ctrl_pressed: false, alt_pressed: false,
            last_movement_time: None,
            active_click_button: None,
            lib_device,
        }
    }

    fn raw_fd(&self) -> RawFd {
        use std::os::unix::io::AsRawFd;
        self.device.as_raw_fd()
    }
}

// ---------------------------------------------------------------------------
// BackendState
// ---------------------------------------------------------------------------

pub struct BackendState {
    devices:              HashMap<RawFd, TrackedDevice>,
    inotify:              Option<Inotify>,
    pub global_typing_time: Option<Instant>,
    config:               InputConfig,
}

unsafe impl Send for BackendState {}

impl BackendState {
    pub fn new(config: InputConfig) -> Self {
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK).ok().and_then(|ino| {
            ino.add_watch("/dev/input",
                AddWatchFlags::IN_CREATE | AddWatchFlags::IN_ATTRIB).ok()?;
            Some(ino)
        });
        Self {
            devices: HashMap::new(),
            inotify,
            global_typing_time: None,
            config,
        }
    }

    /// Scan /dev/input and open every qualifying device.
    /// Emits DEVICE_ADDED into `out` for each one found.
    pub unsafe fn scan_and_open(
        &mut self,
        ctx: *mut LibinputContext,
        out: &mut Vec<LibinputEvent>,
    ) {
        for (path, _) in evdev::enumerate() {
            self.try_open(ctx, &path, out);
        }
    }

    /// Open a single device by path and emit DEVICE_ADDED if successful.
    pub unsafe fn try_open(
        &mut self,
        ctx:  *mut LibinputContext,
        path: &std::path::Path,
        out:  &mut Vec<LibinputEvent>,
    ) {
        if let Ok(device) = Device::open(path) {
            let name = device.name().unwrap_or("Unknown").to_string();
            if name.contains("virtual pointer") || name.contains("libinput-rs") {
                return;
            }
            let is_pointer = name.to_lowercase().contains("touchpad")
                || name.to_lowercase().contains("trackpoint")
                || name.to_lowercase().contains("elan")
                || name.to_lowercase().contains("synaptics")
                || name.to_lowercase().contains("mouse");
            let is_keyboard = device.supported_events().contains(EventType::KEY)
                && device.supported_keys()
                    .is_some_and(|k| k.contains(KeyCode::KEY_A));

            if !is_pointer && !is_keyboard {
                return;
            }

            // Build the LibinputDevice metadata
            let is_absolute = device.supported_events().contains(EventType::ABSOLUTE);
            let lib_dev = Box::into_raw(Box::new(LibinputDevice::new(&name,
                path.to_str().unwrap_or(""),
            )));
            (*lib_dev).has_pointer  = is_pointer;
            (*lib_dev).has_keyboard = is_keyboard;
            (*lib_dev).has_touch    = is_absolute && is_pointer;
            (*ctx).devices.push(lib_dev);

            let fd = {
                use std::os::unix::io::AsRawFd;
                device.as_raw_fd()
            };

            // Check for duplicate fd (already tracked)
            if self.devices.contains_key(&fd) {
                return;
            }

            let tracked = TrackedDevice::new(device, path.to_path_buf(), lib_dev);
            self.devices.insert(fd, tracked);

            out.push(LibinputEvent {
                event_type: LibinputEventType::LIBINPUT_EVENT_DEVICE_ADDED,
                payload:    EventPayload::DeviceAdded,
                context:    ctx,
                device:     lib_dev,
            });
        }
    }

    /// Non-blocking drain: read all pending kernel events, translate them,
    /// and push finished LibinputEvents into `out`.
    pub unsafe fn drain_into_queue(
        &mut self,
        ctx: *mut LibinputContext,
        out: &mut std::collections::VecDeque<LibinputEvent>,
    ) {
        // --- hotplug via inotify ---
        if let Some(ref ino) = self.inotify {
            if let Ok(ievents) = ino.read_events() {
                let mut new_paths: Vec<PathBuf> = Vec::new();
                for iev in ievents {
                    if let Some(name) = iev.name {
                        let p = PathBuf::from("/dev/input").join(&name);
                        let already = self.devices.values().any(|d| d.path == p);
                        if !already {
                            new_paths.push(p);
                        }
                    }
                }
                let mut tmp: Vec<LibinputEvent> = Vec::new();
                for p in new_paths {
                    self.try_open(ctx, &p, &mut tmp);
                }
                for ev in tmp { out.push_back(ev); }
            }
        }

        // --- read events from every open device ---
        let mut dead_fds: Vec<RawFd> = Vec::new();
        let fds: Vec<RawFd> = self.devices.keys().copied().collect();

        for fd in fds {
            let td = match self.devices.get_mut(&fd) {
                Some(d) => d,
                None    => continue,
            };

            let batch: Vec<InputEvent> = match td.device.fetch_events() {
                Ok(b) => b.collect(),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(e) if e.raw_os_error() == Some(nix::libc::ENODEV)
                    || e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    dead_fds.push(fd);
                    continue;
                }
                Err(_) => continue,
            };

            let lib_dev   = td.lib_device;
            let is_abs    = td.is_absolute;
            let is_kbd    = td.is_keyboard;
            let cfg_tap   = td.lib_device.as_ref().map(|d| unsafe { &*d }.tap_enabled)
                .unwrap_or(true);
            let cfg_nat   = td.lib_device.as_ref().map(|d| unsafe { &*d }.natural_scroll)
                .unwrap_or(true);
            let cfg_dwt   = td.lib_device.as_ref().map(|d| unsafe { &*d }.dwt_enabled)
                .unwrap_or(true);
            let cfg_accel = td.lib_device.as_ref()
                .map(|d| unsafe { &*d }.accel_speed as f32 + 1.0)
                .unwrap_or(1.0);

            for ev in batch {
                let ts_usec = (ev.timestamp().tv_sec as u64) * 1_000_000
                    + ev.timestamp().tv_usec as u64;

                // --- keyboard events: track modifier & DWT state ---
                if ev.event_type() == EventType::KEY {
                    let code  = ev.code();
                    let value = ev.value();
                    if code == KeyCode::KEY_LEFTCTRL.0  || code == KeyCode::KEY_RIGHTCTRL.0 {
                        td.ctrl_pressed = value != 0;
                    }
                    if code == KeyCode::KEY_LEFTALT.0 || code == KeyCode::KEY_RIGHTALT.0 {
                        td.alt_pressed = value != 0;
                    }
                    if is_kbd && value != 0 {
                        td.last_typing_time     = Some(Instant::now());
                        self.global_typing_time = Some(Instant::now());
                    }
                }

                // --- relative (mouse/trackpoint): forward as PointerMotion ---
                if !is_abs {
                    if ev.event_type() == EventType::RELATIVE {
                        let code = ev.code();
                        if code == RelativeAxisCode::REL_X.0 {
                            out.push_back(LibinputEvent {
                                event_type: LibinputEventType::LIBINPUT_EVENT_POINTER_MOTION,
                                payload: EventPayload::PointerMotion(PointerMotionEvent {
                                    time_usec: ts_usec,
                                    dx: ev.value() as f64,
                                    dy: 0.0,
                                    dx_unaccel: ev.value() as f64,
                                    dy_unaccel: 0.0,
                                }),
                                context: ctx,
                                device:  lib_dev,
                            });
                        } else if code == RelativeAxisCode::REL_Y.0 {
                            out.push_back(LibinputEvent {
                                event_type: LibinputEventType::LIBINPUT_EVENT_POINTER_MOTION,
                                payload: EventPayload::PointerMotion(PointerMotionEvent {
                                    time_usec: ts_usec,
                                    dx: 0.0,
                                    dy: ev.value() as f64,
                                    dx_unaccel: 0.0,
                                    dy_unaccel: ev.value() as f64,
                                }),
                                context: ctx,
                                device:  lib_dev,
                            });
                        } else if code == RelativeAxisCode::REL_WHEEL.0 {
                            let v = ev.value() as f64;
                            out.push_back(LibinputEvent {
                                event_type: LibinputEventType::LIBINPUT_EVENT_POINTER_SCROLL_WHEEL,
                                payload: EventPayload::PointerAxis(PointerAxisEvent {
                                    time_usec: ts_usec,
                                    axis:   0, // LIBINPUT_POINTER_AXIS_SCROLL_VERTICAL
                                    value:  v * 15.0,
                                    value_discrete: ev.value(),
                                    source: 1, // wheel
                                }),
                                context: ctx,
                                device:  lib_dev,
                            });
                        }
                    } else if ev.event_type() == EventType::KEY {
                        let code = ev.code();
                        if code == KeyCode::BTN_LEFT.0
                            || code == KeyCode::BTN_RIGHT.0
                            || code == KeyCode::BTN_MIDDLE.0
                        {
                            // Linux BTN_LEFT=0x110, RIGHT=0x111, MIDDLE=0x112
                            // libinput passes them straight through as evdev codes
                            out.push_back(LibinputEvent {
                                event_type: LibinputEventType::LIBINPUT_EVENT_POINTER_BUTTON,
                                payload: EventPayload::PointerButton(PointerButtonEvent {
                                    time_usec: ts_usec,
                                    button: code as u32,
                                    state:  ev.value() as u32,
                                }),
                                context: ctx,
                                device:  lib_dev,
                            });
                        }
                    }
                    continue;
                }

                // --- absolute device (touchpad) ---
                let dwt_active = cfg_dwt && self.global_typing_time
                    .map(|t| t.elapsed() < Duration::from_millis(500))
                    .unwrap_or(false);

                if dwt_active { td.tap_emitted = true; }

                match ev.event_type() {
                    EventType::KEY => {
                        let code = ev.code();
                        if code == KeyCode::BTN_TOUCH.0 {
                            td.touch_active = ev.value() != 0;
                            if td.touch_active {
                                td.touch_start_time = Some(Instant::now());
                                td.tap_emitted      = false;
                                td.touch_fingers    = 1;
                                td.last_x           = None;
                                td.last_y           = None;
                            } else {
                                // tap-to-click
                                if cfg_tap && !td.tap_emitted && !dwt_active {
                                    if let Some(start) = td.touch_start_time {
                                        if start.elapsed() < Duration::from_millis(250) {
                                            out.push_back(LibinputEvent {
                                                event_type: LibinputEventType::LIBINPUT_EVENT_POINTER_BUTTON,
                                                payload: EventPayload::PointerButton(PointerButtonEvent {
                                                    time_usec: ts_usec,
                                                    button: KeyCode::BTN_LEFT.0 as u32,
                                                    state:  1,
                                                }),
                                                context: ctx, device: lib_dev,
                                            });
                                            out.push_back(LibinputEvent {
                                                event_type: LibinputEventType::LIBINPUT_EVENT_POINTER_BUTTON,
                                                payload: EventPayload::PointerButton(PointerButtonEvent {
                                                    time_usec: ts_usec,
                                                    button: KeyCode::BTN_LEFT.0 as u32,
                                                    state:  0,
                                                }),
                                                context: ctx, device: lib_dev,
                                            });
                                        }
                                    }
                                }
                                td.last_x           = None;
                                td.last_y           = None;
                                td.current_dx       = 0;
                                td.current_dy       = 0;
                                td.touch_start_time = None;
                                td.touch_fingers    = 0;
                            }
                        } else if code == KeyCode::BTN_TOOL_DOUBLETAP.0 {
                            td.touch_fingers = if ev.value() != 0 { 2 } else { 1 };
                        } else if code == KeyCode::BTN_TOOL_TRIPLETAP.0 {
                            td.touch_fingers = if ev.value() != 0 { 3 } else { 2 };
                        } else {
                            // physical click button with finger-count remapping
                            let mut mapped = code;
                            if code == KeyCode::BTN_LEFT.0 {
                                if ev.value() != 0 {
                                    if td.touch_fingers == 2 { mapped = KeyCode::BTN_RIGHT.0; }
                                    else if td.touch_fingers == 3 { mapped = KeyCode::BTN_MIDDLE.0; }
                                    td.active_click_button = Some(mapped);
                                } else if let Some(active) = td.active_click_button {
                                    mapped = active;
                                    td.active_click_button = None;
                                }
                            }
                            if mapped == KeyCode::BTN_LEFT.0
                                || mapped == KeyCode::BTN_RIGHT.0
                                || mapped == KeyCode::BTN_MIDDLE.0
                            {
                                out.push_back(LibinputEvent {
                                    event_type: LibinputEventType::LIBINPUT_EVENT_POINTER_BUTTON,
                                    payload: EventPayload::PointerButton(PointerButtonEvent {
                                        time_usec: ts_usec,
                                        button: mapped as u32,
                                        state:  ev.value() as u32,
                                    }),
                                    context: ctx, device: lib_dev,
                                });
                            }
                        }
                    }

                    EventType::ABSOLUTE => {
                        let code = ev.code();
                        if code == AbsoluteAxisCode::ABS_X.0
                            || code == AbsoluteAxisCode::ABS_Y.0
                        {
                            if let Some(last) = td.last_movement_time {
                                if last.elapsed() > Duration::from_millis(50) {
                                    td.last_x = None;
                                    td.last_y = None;
                                }
                            }
                            td.last_movement_time = Some(Instant::now());
                        }
                        if code == AbsoluteAxisCode::ABS_X.0 {
                            let val = ev.value();
                            if let Some(px) = td.last_x { td.current_dx += val - px; }
                            td.last_x = Some(val);
                        } else if code == AbsoluteAxisCode::ABS_Y.0 {
                            let val = ev.value();
                            if let Some(py) = td.last_y { td.current_dy += val - py; }
                            td.last_y = Some(val);
                        }
                        // MT slots for touch events
                        else if code == AbsoluteAxisCode::ABS_MT_POSITION_X.0 {
                            out.push_back(LibinputEvent {
                                event_type: LibinputEventType::LIBINPUT_EVENT_TOUCH_MOTION,
                                payload: EventPayload::TouchMotion(TouchEvent {
                                    time_usec: ts_usec,
                                    slot: 0, seat_slot: 0,
                                    x: ev.value() as f64,
                                    y: 0.0,
                                }),
                                context: ctx, device: lib_dev,
                            });
                        } else if code == AbsoluteAxisCode::ABS_MT_POSITION_Y.0 {
                            out.push_back(LibinputEvent {
                                event_type: LibinputEventType::LIBINPUT_EVENT_TOUCH_MOTION,
                                payload: EventPayload::TouchMotion(TouchEvent {
                                    time_usec: ts_usec,
                                    slot: 0, seat_slot: 0,
                                    x: 0.0,
                                    y: ev.value() as f64,
                                }),
                                context: ctx, device: lib_dev,
                            });
                        }
                    }

                    EventType::SYNCHRONIZATION => {
                        if ev.code() == 0 {
                            let has_movement = td.current_dx != 0 || td.current_dy != 0;

                            if dwt_active {
                                td.current_dx  = 0;
                                td.current_dy  = 0;
                                td.remainder_x = 0.0;
                                td.remainder_y = 0.0;
                                td.tap_emitted = true;
                            } else if has_movement {
                                td.tap_emitted = true;

                                let hw_scale: f32 = 0.18;

                                if td.touch_fingers <= 1 {
                                    let total_x = (td.current_dx as f32 * hw_scale)
                                        * cfg_accel + td.remainder_x;
                                    let total_y = (td.current_dy as f32 * hw_scale)
                                        * cfg_accel + td.remainder_y;
                                    let emit_x = total_x.round() as i32;
                                    let emit_y = total_y.round() as i32;
                                    td.remainder_x = total_x - emit_x as f32;
                                    td.remainder_y = total_y - emit_y as f32;

                                    if emit_x != 0 || emit_y != 0 {
                                        out.push_back(LibinputEvent {
                                            event_type: LibinputEventType::LIBINPUT_EVENT_POINTER_MOTION,
                                            payload: EventPayload::PointerMotion(PointerMotionEvent {
                                                time_usec:  ts_usec,
                                                dx:         emit_x as f64,
                                                dy:         emit_y as f64,
                                                dx_unaccel: (td.current_dx as f32 * hw_scale) as f64,
                                                dy_unaccel: (td.current_dy as f32 * hw_scale) as f64,
                                            }),
                                            context: ctx, device: lib_dev,
                                        });
                                    }
                                } else if td.touch_fingers == 2 {
                                    let scroll_scale: f32 = 0.02;
                                    let total_y = (td.current_dy as f32 * scroll_scale)
                                        + td.remainder_y;
                                    let emit_wheel = total_y.round() as i32;
                                    td.remainder_y = total_y - emit_wheel as f32;
                                    td.remainder_x = 0.0;

                                    if emit_wheel != 0 {
                                        let final_wheel = if cfg_nat { -emit_wheel } else { emit_wheel };
                                        out.push_back(LibinputEvent {
                                            event_type: LibinputEventType::LIBINPUT_EVENT_POINTER_SCROLL_FINGER,
                                            payload: EventPayload::PointerAxis(PointerAxisEvent {
                                                time_usec: ts_usec,
                                                axis:   0,
                                                value:  final_wheel as f64 * 15.0,
                                                value_discrete: final_wheel,
                                                source: 2, // finger
                                            }),
                                            context: ctx, device: lib_dev,
                                        });
                                    }
                                } else if td.touch_fingers >= 3 {
                                    // 3-finger swipe -> gesture event
                                    let gscale: f32 = 0.18;
                                    let dx = td.current_dx as f64 * gscale as f64;
                                    let dy = td.current_dy as f64 * gscale as f64;
                                    out.push_back(LibinputEvent {
                                        event_type: LibinputEventType::LIBINPUT_EVENT_GESTURE_SWIPE_UPDATE,
                                        payload: EventPayload::GestureSwipeUpdate(GestureEvent {
                                            time_usec: ts_usec,
                                            finger_count: td.touch_fingers as i32,
                                            dx, dy,
                                            scale: 1.0, angle: 0.0,
                                            cancelled: false,
                                        }),
                                        context: ctx, device: lib_dev,
                                    });
                                }

                                td.current_dx = 0;
                                td.current_dy = 0;
                            }
                        }
                    }

                    _ => {}
                }
            }
        }

        // Remove disconnected devices and emit DEVICE_REMOVED
        for fd in dead_fds {
            if let Some(td) = self.devices.remove(&fd) {
                out.push_back(LibinputEvent {
                    event_type: LibinputEventType::LIBINPUT_EVENT_DEVICE_REMOVED,
                    payload:    EventPayload::DeviceRemoved,
                    context:    ctx,
                    device:     td.lib_device,
                });
            }
        }
    }
}
