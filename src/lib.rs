//! libinput-rs: drop-in Rust replacement for libinput.so
//!
//! Exports the complete C ABI surface defined by <libinput.h>.
//! Applications that link against libinput can use this library
//! transparently via LD_PRELOAD or by replacing the .so symlink.

#![allow(non_snake_case, clippy::missing_safety_doc)]

mod backend;
mod config;
mod device;
mod event_loop;
mod ffi_types;
mod virtual_device;

use ffi_types::*;
use std::ffi::CStr;
use std::os::unix::io::RawFd;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Drain the evdev backend into the context event queue.
unsafe fn populate_events(ctx: *mut LibinputContext) {
    if ctx.is_null() { return; }
    let ctx_ref = &mut *ctx;
    let mut tmp: std::collections::VecDeque<LibinputEvent> = std::collections::VecDeque::new();
    if let Ok(mut backend) = ctx_ref.backend.lock() {
        backend.drain_into_queue(ctx, &mut tmp);
    }
    ctx_ref.event_queue.extend(tmp);
}

// ---------------------------------------------------------------------------
// Context lifecycle
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_udev_create_context(
    interface: *const LibinputInterface,
    user_data: *mut libc::c_void,
    _udev:     *mut libc::c_void,
) -> *mut LibinputContext {
    if interface.is_null() { return std::ptr::null_mut(); }
    Box::into_raw(Box::new(LibinputContext::new(interface, user_data)))
}

#[no_mangle]
pub unsafe extern "C" fn libinput_path_create_context(
    interface: *const LibinputInterface,
    user_data: *mut libc::c_void,
) -> *mut LibinputContext {
    if interface.is_null() { return std::ptr::null_mut(); }
    Box::into_raw(Box::new(LibinputContext::new(interface, user_data)))
}

#[no_mangle]
pub unsafe extern "C" fn libinput_ref(
    ctx: *mut LibinputContext,
) -> *mut LibinputContext {
    if ctx.is_null() { return std::ptr::null_mut(); }
    (*ctx).inc_ref();
    ctx
}

#[no_mangle]
pub unsafe extern "C" fn libinput_unref(
    ctx: *mut LibinputContext,
) -> *mut LibinputContext {
    if ctx.is_null() { return std::ptr::null_mut(); }
    if (*ctx).dec_ref() == 0 {
        drop(Box::from_raw(ctx));
        return std::ptr::null_mut();
    }
    ctx
}

/// Assign seat and immediately scan /dev/input for all qualifying devices.
#[no_mangle]
pub unsafe extern "C" fn libinput_udev_assign_seat(
    ctx:       *mut LibinputContext,
    seat_name: *const libc::c_char,
) -> libc::c_int {
    if ctx.is_null() || seat_name.is_null() { return -1; }
    (*ctx).seat.logical_name = CStr::from_ptr(seat_name)
        .to_string_lossy().into_owned();

    // Scan all input devices and emit DEVICE_ADDED for each
    let mut tmp: Vec<LibinputEvent> = Vec::new();
    if let Ok(mut backend) = (*ctx).backend.lock() {
        backend.scan_and_open(ctx, &mut tmp);
    }
    for ev in tmp {
        (*ctx).event_queue.push_back(ev);
    }
    if !(*ctx).event_queue.is_empty() {
        (*ctx).signal_fd();
    }
    0
}

/// Add a device by path and open it through the backend.
#[no_mangle]
pub unsafe extern "C" fn libinput_path_add_device(
    ctx:  *mut LibinputContext,
    path: *const libc::c_char,
) -> *mut LibinputDevice {
    if ctx.is_null() || path.is_null() { return std::ptr::null_mut(); }
    let devnode = CStr::from_ptr(path).to_string_lossy().into_owned();
    let p = std::path::PathBuf::from(&devnode);
    let mut tmp: Vec<LibinputEvent> = Vec::new();
    if let Ok(mut backend) = (*ctx).backend.lock() {
        backend.try_open(ctx, &p, &mut tmp);
    }
    for ev in tmp {
        (*ctx).event_queue.push_back(ev);
    }
    // Return the last device that was just added (if any)
    (*ctx).devices.last().copied().unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn libinput_path_remove_device(
    dev: *mut LibinputDevice,
) {
    if dev.is_null() { return; }
    (*dev).name = String::new();
}

// ---------------------------------------------------------------------------
// File descriptor & dispatch
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_get_fd(
    ctx: *mut LibinputContext,
) -> RawFd {
    if ctx.is_null() { return -1; }
    (*ctx).event_fd
}

#[no_mangle]
pub unsafe extern "C" fn libinput_dispatch(
    ctx: *mut LibinputContext,
) -> libc::c_int {
    if ctx.is_null() { return -1; }
    (*ctx).drain_fd();
    populate_events(ctx);
    if !(*ctx).event_queue.is_empty() {
        (*ctx).signal_fd();
    }
    0
}

// ---------------------------------------------------------------------------
// Event retrieval & destruction
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_get_event(
    ctx: *mut LibinputContext,
) -> *mut LibinputEvent {
    if ctx.is_null() { return std::ptr::null_mut(); }
    match (*ctx).event_queue.pop_front() {
        Some(ev) => Box::into_raw(Box::new(ev)),
        None     => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_next_event_type(
    ctx: *mut LibinputContext,
) -> LibinputEventType {
    if ctx.is_null() { return LibinputEventType::LIBINPUT_EVENT_NONE; }
    (*ctx).event_queue.front()
        .map(|e| e.event_type)
        .unwrap_or(LibinputEventType::LIBINPUT_EVENT_NONE)
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_destroy(
    event: *mut LibinputEvent,
) {
    if !event.is_null() { drop(Box::from_raw(event)); }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_get_type(
    event: *const LibinputEvent,
) -> LibinputEventType {
    if event.is_null() { return LibinputEventType::LIBINPUT_EVENT_NONE; }
    (*event).event_type
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_get_context(
    event: *const LibinputEvent,
) -> *mut LibinputContext {
    if event.is_null() { return std::ptr::null_mut(); }
    (*event).context
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_get_device(
    event: *const LibinputEvent,
) -> *mut LibinputDevice {
    if event.is_null() { return std::ptr::null_mut(); }
    (*event).device
}

// ---------------------------------------------------------------------------
// Pointer event accessors
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_event_get_pointer_event(
    event: *mut LibinputEvent,
) -> *mut LibinputEvent {
    if event.is_null() { return std::ptr::null_mut(); }
    match (*event).event_type {
        LibinputEventType::LIBINPUT_EVENT_POINTER_MOTION
        | LibinputEventType::LIBINPUT_EVENT_POINTER_MOTION_ABSOLUTE
        | LibinputEventType::LIBINPUT_EVENT_POINTER_BUTTON
        | LibinputEventType::LIBINPUT_EVENT_POINTER_AXIS
        | LibinputEventType::LIBINPUT_EVENT_POINTER_SCROLL_WHEEL
        | LibinputEventType::LIBINPUT_EVENT_POINTER_SCROLL_FINGER
        | LibinputEventType::LIBINPUT_EVENT_POINTER_SCROLL_CONTINUOUS => event,
        _ => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_time_usec(
    event: *const LibinputEvent,
) -> u64 {
    if event.is_null() { return 0; }
    match &(*event).payload {
        EventPayload::PointerMotion(e)         => e.time_usec,
        EventPayload::PointerMotionAbsolute(e) => e.time_usec,
        EventPayload::PointerButton(e)         => e.time_usec,
        EventPayload::PointerAxis(e)           => e.time_usec,
        _ => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_dx(
    event: *const LibinputEvent,
) -> f64 {
    if event.is_null() { return 0.0; }
    if let EventPayload::PointerMotion(e) = &(*event).payload { e.dx } else { 0.0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_dy(
    event: *const LibinputEvent,
) -> f64 {
    if event.is_null() { return 0.0; }
    if let EventPayload::PointerMotion(e) = &(*event).payload { e.dy } else { 0.0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_dx_unaccelerated(
    event: *const LibinputEvent,
) -> f64 {
    if event.is_null() { return 0.0; }
    if let EventPayload::PointerMotion(e) = &(*event).payload { e.dx_unaccel } else { 0.0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_dy_unaccelerated(
    event: *const LibinputEvent,
) -> f64 {
    if event.is_null() { return 0.0; }
    if let EventPayload::PointerMotion(e) = &(*event).payload { e.dy_unaccel } else { 0.0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_absolute_x(
    event: *const LibinputEvent,
) -> f64 {
    if event.is_null() { return 0.0; }
    if let EventPayload::PointerMotionAbsolute(e) = &(*event).payload { e.abs_x } else { 0.0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_absolute_y(
    event: *const LibinputEvent,
) -> f64 {
    if event.is_null() { return 0.0; }
    if let EventPayload::PointerMotionAbsolute(e) = &(*event).payload { e.abs_y } else { 0.0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_button(
    event: *const LibinputEvent,
) -> u32 {
    if event.is_null() { return 0; }
    if let EventPayload::PointerButton(e) = &(*event).payload { e.button } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_button_state(
    event: *const LibinputEvent,
) -> u32 {
    if event.is_null() { return 0; }
    if let EventPayload::PointerButton(e) = &(*event).payload { e.state } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_axis_value(
    event: *const LibinputEvent,
    _axis: u32,
) -> f64 {
    if event.is_null() { return 0.0; }
    if let EventPayload::PointerAxis(e) = &(*event).payload { e.value } else { 0.0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_axis_value_discrete(
    event: *const LibinputEvent,
    _axis: u32,
) -> f64 {
    if event.is_null() { return 0.0; }
    if let EventPayload::PointerAxis(e) = &(*event).payload { e.value_discrete as f64 } else { 0.0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_axis_source(
    event: *const LibinputEvent,
) -> u32 {
    if event.is_null() { return 0; }
    if let EventPayload::PointerAxis(e) = &(*event).payload { e.source } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_has_axis(
    event: *const LibinputEvent,
    _axis: u32,
) -> libc::c_int {
    if event.is_null() { return 0; }
    matches!((*event).payload, EventPayload::PointerAxis(_)) as libc::c_int
}

// ---------------------------------------------------------------------------
// Keyboard event accessors
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_event_get_keyboard_event(
    event: *mut LibinputEvent,
) -> *mut LibinputEvent {
    if event.is_null() { return std::ptr::null_mut(); }
    if (*event).event_type == LibinputEventType::LIBINPUT_EVENT_KEYBOARD_KEY {
        event
    } else { std::ptr::null_mut() }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_keyboard_get_time_usec(
    event: *const LibinputEvent,
) -> u64 {
    if event.is_null() { return 0; }
    if let EventPayload::KeyboardKey(e) = &(*event).payload { e.time_usec } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_keyboard_get_key(
    event: *const LibinputEvent,
) -> u32 {
    if event.is_null() { return 0; }
    if let EventPayload::KeyboardKey(e) = &(*event).payload { e.key } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_keyboard_get_key_state(
    event: *const LibinputEvent,
) -> u32 {
    if event.is_null() { return 0; }
    if let EventPayload::KeyboardKey(e) = &(*event).payload { e.state } else { 0 }
}

// ---------------------------------------------------------------------------
// Touch event accessors
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_event_get_touch_event(
    event: *mut LibinputEvent,
) -> *mut LibinputEvent {
    if event.is_null() { return std::ptr::null_mut(); }
    match (*event).event_type {
        LibinputEventType::LIBINPUT_EVENT_TOUCH_DOWN
        | LibinputEventType::LIBINPUT_EVENT_TOUCH_UP
        | LibinputEventType::LIBINPUT_EVENT_TOUCH_MOTION
        | LibinputEventType::LIBINPUT_EVENT_TOUCH_CANCEL
        | LibinputEventType::LIBINPUT_EVENT_TOUCH_FRAME => event,
        _ => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_touch_get_time_usec(
    event: *const LibinputEvent,
) -> u64 {
    if event.is_null() { return 0; }
    match &(*event).payload {
        EventPayload::TouchDown(e)
        | EventPayload::TouchUp(e)
        | EventPayload::TouchMotion(e)
        | EventPayload::TouchCancel(e) => e.time_usec,
        EventPayload::TouchFrame { time_usec } => *time_usec,
        _ => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_touch_get_slot(
    event: *const LibinputEvent,
) -> i32 {
    if event.is_null() { return -1; }
    match &(*event).payload {
        EventPayload::TouchDown(e) | EventPayload::TouchMotion(e) => e.slot,
        _ => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_touch_get_x(
    event: *const LibinputEvent,
) -> f64 {
    if event.is_null() { return 0.0; }
    match &(*event).payload {
        EventPayload::TouchDown(e) | EventPayload::TouchMotion(e) => e.x,
        _ => 0.0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_touch_get_y(
    event: *const LibinputEvent,
) -> f64 {
    if event.is_null() { return 0.0; }
    match &(*event).payload {
        EventPayload::TouchDown(e) | EventPayload::TouchMotion(e) => e.y,
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Gesture event accessors
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_event_get_gesture_event(
    event: *mut LibinputEvent,
) -> *mut LibinputEvent {
    if event.is_null() { return std::ptr::null_mut(); }
    match (*event).event_type {
        LibinputEventType::LIBINPUT_EVENT_GESTURE_SWIPE_BEGIN
        | LibinputEventType::LIBINPUT_EVENT_GESTURE_SWIPE_UPDATE
        | LibinputEventType::LIBINPUT_EVENT_GESTURE_SWIPE_END
        | LibinputEventType::LIBINPUT_EVENT_GESTURE_PINCH_BEGIN
        | LibinputEventType::LIBINPUT_EVENT_GESTURE_PINCH_UPDATE
        | LibinputEventType::LIBINPUT_EVENT_GESTURE_PINCH_END => event,
        _ => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_gesture_get_finger_count(
    event: *const LibinputEvent,
) -> libc::c_int {
    if event.is_null() { return 0; }
    match &(*event).payload {
        EventPayload::GestureSwipeBegin(e)
        | EventPayload::GestureSwipeUpdate(e)
        | EventPayload::GestureSwipeEnd(e)
        | EventPayload::GesturePinchBegin(e)
        | EventPayload::GesturePinchUpdate(e)
        | EventPayload::GesturePinchEnd(e) => e.finger_count,
        _ => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_gesture_get_dx(
    event: *const LibinputEvent,
) -> f64 {
    if event.is_null() { return 0.0; }
    match &(*event).payload {
        EventPayload::GestureSwipeUpdate(e)
        | EventPayload::GesturePinchUpdate(e) => e.dx,
        _ => 0.0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_gesture_get_dy(
    event: *const LibinputEvent,
) -> f64 {
    if event.is_null() { return 0.0; }
    match &(*event).payload {
        EventPayload::GestureSwipeUpdate(e)
        | EventPayload::GesturePinchUpdate(e) => e.dy,
        _ => 0.0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_gesture_get_scale(
    event: *const LibinputEvent,
) -> f64 {
    if event.is_null() { return 1.0; }
    match &(*event).payload {
        EventPayload::GesturePinchUpdate(e)
        | EventPayload::GesturePinchEnd(e) => e.scale,
        _ => 1.0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_gesture_get_angle_delta(
    event: *const LibinputEvent,
) -> f64 {
    if event.is_null() { return 0.0; }
    match &(*event).payload {
        EventPayload::GesturePinchUpdate(e) => e.angle,
        _ => 0.0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_gesture_get_cancelled(
    event: *const LibinputEvent,
) -> libc::c_int {
    if event.is_null() { return 0; }
    match &(*event).payload {
        EventPayload::GestureSwipeEnd(e)
        | EventPayload::GesturePinchEnd(e) => e.cancelled as libc::c_int,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// Switch event accessors
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_event_get_switch_event(
    event: *mut LibinputEvent,
) -> *mut LibinputEvent {
    if event.is_null() { return std::ptr::null_mut(); }
    if (*event).event_type == LibinputEventType::LIBINPUT_EVENT_SWITCH_TOGGLE {
        event
    } else { std::ptr::null_mut() }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_switch_get_switch(
    event: *const LibinputEvent,
) -> u32 {
    if event.is_null() { return 0; }
    if let EventPayload::SwitchToggle(e) = &(*event).payload { e.switch } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_switch_get_switch_state(
    event: *const LibinputEvent,
) -> u32 {
    if event.is_null() { return 0; }
    if let EventPayload::SwitchToggle(e) = &(*event).payload { e.state } else { 0 }
}

// ---------------------------------------------------------------------------
// Device info
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_device_ref(
    dev: *mut LibinputDevice,
) -> *mut LibinputDevice {
    if dev.is_null() { return std::ptr::null_mut(); }
    (*dev).refcount.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    dev
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_unref(
    dev: *mut LibinputDevice,
) -> *mut LibinputDevice {
    if dev.is_null() { return std::ptr::null_mut(); }
    (*dev).refcount.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    std::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_get_name(
    dev: *const LibinputDevice,
) -> *const libc::c_char {
    if dev.is_null() { return std::ptr::null(); }
    (*dev).name.as_ptr() as *const libc::c_char
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_get_sysname(
    dev: *const LibinputDevice,
) -> *const libc::c_char {
    if dev.is_null() { return std::ptr::null(); }
    (*dev).sysname.as_ptr() as *const libc::c_char
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_get_output_name(
    _dev: *const LibinputDevice,
) -> *const libc::c_char { std::ptr::null() }

#[no_mangle]
pub unsafe extern "C" fn libinput_device_get_id_vendor(
    dev: *const LibinputDevice,
) -> libc::c_uint {
    if dev.is_null() { return 0; }
    (*dev).vendor_id
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_get_id_product(
    dev: *const LibinputDevice,
) -> libc::c_uint {
    if dev.is_null() { return 0; }
    (*dev).product_id
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_get_devnode(
    dev: *const LibinputDevice,
) -> *const libc::c_char {
    if dev.is_null() { return std::ptr::null(); }
    (*dev).devnode.as_ptr() as *const libc::c_char
}

// ---------------------------------------------------------------------------
// Device capability checks
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_device_has_capability(
    dev: *const LibinputDevice, capability: u32,
) -> libc::c_int {
    if dev.is_null() { return 0; }
    let has = match capability {
        1 => (*dev).has_keyboard,
        2 => (*dev).has_pointer,
        3 => (*dev).has_touch,
        4 => (*dev).has_gesture,
        5 => (*dev).has_switch,
        6 => (*dev).has_tablet,
        _ => false,
    };
    has as libc::c_int
}

// ---------------------------------------------------------------------------
// Device configuration — tap
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_tap_get_finger_count(
    dev: *const LibinputDevice,
) -> libc::c_int {
    if dev.is_null() { return 0; }
    if (*dev).has_touch || (*dev).has_pointer { 3 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_tap_set_enabled(
    dev: *mut LibinputDevice, enabled: u32,
) -> u32 {
    if dev.is_null() { return 1; }
    (*dev).tap_enabled = enabled != 0; 0
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_tap_get_enabled(
    dev: *const LibinputDevice,
) -> u32 {
    if dev.is_null() { return 0; } (*dev).tap_enabled as u32
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_tap_get_default_enabled(
    _dev: *const LibinputDevice,
) -> u32 { 1 }

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_tap_set_drag_enabled(
    dev: *mut LibinputDevice, _e: u32,
) -> u32 { if dev.is_null() { 1 } else { 0 } }

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_tap_get_drag_enabled(
    _dev: *const LibinputDevice,
) -> u32 { 1 }

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_tap_set_drag_lock_enabled(
    dev: *mut LibinputDevice, _e: u32,
) -> u32 { if dev.is_null() { 1 } else { 0 } }

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_tap_get_drag_lock_enabled(
    _dev: *const LibinputDevice,
) -> u32 { 0 }

// ---------------------------------------------------------------------------
// Device configuration — pointer acceleration
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_accel_is_available(
    dev: *const LibinputDevice,
) -> libc::c_int {
    if dev.is_null() { return 0; } (*dev).has_pointer as libc::c_int
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_accel_set_speed(
    dev: *mut LibinputDevice, speed: f64,
) -> u32 {
    if dev.is_null() { return 1; }
    (*dev).accel_speed = speed.clamp(-1.0, 1.0); 0
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_accel_get_speed(
    dev: *const LibinputDevice,
) -> f64 {
    if dev.is_null() { return 0.0; } (*dev).accel_speed
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_accel_get_default_speed(
    _dev: *const LibinputDevice,
) -> f64 { 0.0 }

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_accel_get_profiles(
    dev: *const LibinputDevice,
) -> u32 {
    if dev.is_null() { return 0; } if (*dev).has_pointer { 0b11 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_accel_set_profile(
    dev: *mut LibinputDevice, profile: u32,
) -> u32 {
    if dev.is_null() { return 1; } (*dev).accel_profile = profile; 0
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_accel_get_profile(
    dev: *const LibinputDevice,
) -> u32 {
    if dev.is_null() { return 0; } (*dev).accel_profile
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_accel_get_default_profile(
    _dev: *const LibinputDevice,
) -> u32 { 1 }

// ---------------------------------------------------------------------------
// Device configuration — natural scroll
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_scroll_has_natural_scroll(
    dev: *const LibinputDevice,
) -> libc::c_int {
    if dev.is_null() { return 0; } (*dev).has_pointer as libc::c_int
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_scroll_set_natural_scroll_enabled(
    dev: *mut LibinputDevice, enabled: libc::c_int,
) -> u32 {
    if dev.is_null() { return 1; } (*dev).natural_scroll = enabled != 0; 0
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_scroll_get_natural_scroll_enabled(
    dev: *const LibinputDevice,
) -> libc::c_int {
    if dev.is_null() { return 0; } (*dev).natural_scroll as libc::c_int
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_scroll_get_default_natural_scroll_enabled(
    _dev: *const LibinputDevice,
) -> libc::c_int { 0 }

// ---------------------------------------------------------------------------
// Device configuration — left-handed
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_left_handed_is_available(
    dev: *const LibinputDevice,
) -> libc::c_int {
    if dev.is_null() { return 0; } (*dev).has_pointer as libc::c_int
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_left_handed_set(
    dev: *mut LibinputDevice, enabled: libc::c_int,
) -> u32 {
    if dev.is_null() { return 1; } (*dev).left_handed = enabled != 0; 0
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_left_handed_get(
    dev: *const LibinputDevice,
) -> libc::c_int {
    if dev.is_null() { return 0; } (*dev).left_handed as libc::c_int
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_left_handed_get_default(
    _dev: *const LibinputDevice,
) -> libc::c_int { 0 }

// ---------------------------------------------------------------------------
// Device configuration — scroll method
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_scroll_get_methods(
    dev: *const LibinputDevice,
) -> u32 {
    if dev.is_null() { return 0; }
    if (*dev).has_touch || (*dev).has_pointer { 0b111 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_scroll_set_method(
    dev: *mut LibinputDevice, method: u32,
) -> u32 {
    if dev.is_null() { return 1; } (*dev).scroll_method = method; 0
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_scroll_get_method(
    dev: *const LibinputDevice,
) -> u32 {
    if dev.is_null() { return 0; } (*dev).scroll_method
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_scroll_get_default_method(
    _dev: *const LibinputDevice,
) -> u32 { 2 }

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_scroll_set_button(
    dev: *mut LibinputDevice, _button: u32,
) -> u32 { if dev.is_null() { 1 } else { 0 } }

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_scroll_get_button(
    _dev: *const LibinputDevice,
) -> u32 { 0 }

// ---------------------------------------------------------------------------
// Device configuration — click method
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_click_get_methods(
    dev: *const LibinputDevice,
) -> u32 {
    if dev.is_null() { return 0; } if (*dev).has_pointer { 0b11 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_click_set_method(
    dev: *mut LibinputDevice, method: u32,
) -> u32 {
    if dev.is_null() { return 1; } (*dev).click_method = method; 0
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_click_get_method(
    dev: *const LibinputDevice,
) -> u32 {
    if dev.is_null() { return 0; } (*dev).click_method
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_click_get_default_method(
    _dev: *const LibinputDevice,
) -> u32 { 1 }

// ---------------------------------------------------------------------------
// Device configuration — middle button emulation
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_middle_emulation_is_available(
    dev: *const LibinputDevice,
) -> libc::c_int {
    if dev.is_null() { return 0; } (*dev).has_pointer as libc::c_int
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_middle_emulation_set_enabled(
    dev: *mut LibinputDevice, enabled: u32,
) -> u32 {
    if dev.is_null() { return 1; } (*dev).middle_emulation = enabled != 0; 0
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_middle_emulation_get_enabled(
    dev: *const LibinputDevice,
) -> u32 {
    if dev.is_null() { return 0; } (*dev).middle_emulation as u32
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_middle_emulation_get_default_enabled(
    _dev: *const LibinputDevice,
) -> u32 { 0 }

// ---------------------------------------------------------------------------
// Device configuration — disable-while-typing
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_dwt_is_available(
    dev: *const LibinputDevice,
) -> libc::c_int {
    if dev.is_null() { return 0; }
    ((*dev).has_pointer || (*dev).has_touch) as libc::c_int
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_dwt_set_enabled(
    dev: *mut LibinputDevice, enabled: u32,
) -> u32 {
    if dev.is_null() { return 1; } (*dev).dwt_enabled = enabled != 0; 0
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_dwt_get_enabled(
    dev: *const LibinputDevice,
) -> u32 {
    if dev.is_null() { return 0; } (*dev).dwt_enabled as u32
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_dwt_get_default_enabled(
    _dev: *const LibinputDevice,
) -> u32 { 1 }

// ---------------------------------------------------------------------------
// Device configuration — calibration matrix
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_calibration_has_matrix(
    dev: *const LibinputDevice,
) -> libc::c_int {
    if dev.is_null() { return 0; } (*dev).has_touch as libc::c_int
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_calibration_set_matrix(
    dev: *mut LibinputDevice, matrix: *const f32,
) -> u32 {
    if dev.is_null() || matrix.is_null() { return 1; }
    (*dev).calibration.copy_from_slice(std::slice::from_raw_parts(matrix, 6));
    0
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_calibration_get_matrix(
    dev: *const LibinputDevice, matrix: *mut f32,
) -> libc::c_int {
    if dev.is_null() || matrix.is_null() { return 0; }
    std::slice::from_raw_parts_mut(matrix, 6).copy_from_slice(&(*dev).calibration);
    1
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_config_calibration_get_default_matrix(
    _dev: *const LibinputDevice, matrix: *mut f32,
) -> libc::c_int {
    if matrix.is_null() { return 0; }
    std::slice::from_raw_parts_mut(matrix, 6)
        .copy_from_slice(&[1.0_f32, 0.0, 0.0, 0.0, 1.0, 0.0]);
    1
}

// ---------------------------------------------------------------------------
// Seat
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_device_get_seat(
    _dev: *const LibinputDevice,
) -> *mut libc::c_void { std::ptr::null_mut() }

#[no_mangle]
pub unsafe extern "C" fn libinput_seat_get_physical_name(
    _seat: *const libc::c_void,
) -> *const libc::c_char {
    b"seat0\0".as_ptr() as *const libc::c_char
}

#[no_mangle]
pub unsafe extern "C" fn libinput_seat_get_logical_name(
    _seat: *const libc::c_void,
) -> *const libc::c_char {
    b"default\0".as_ptr() as *const libc::c_char
}

// ---------------------------------------------------------------------------
// Logging
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_log_set_priority(
    _ctx: *mut LibinputContext, _priority: u32,
) {}

#[no_mangle]
pub unsafe extern "C" fn libinput_log_get_priority(
    _ctx: *const LibinputContext,
) -> u32 { 3 }

#[no_mangle]
pub unsafe extern "C" fn libinput_log_set_handler(
    ctx: *mut LibinputContext,
    handler: Option<unsafe extern "C" fn(
        ctx: *mut LibinputContext, priority: u32, msg: *const libc::c_char,
    )>,
) {
    if ctx.is_null() { return; }
    (*ctx).log_handler = handler;
}

// ---------------------------------------------------------------------------
// User data
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_set_user_data(
    ctx: *mut LibinputContext, data: *mut libc::c_void,
) {
    if ctx.is_null() { return; } (*ctx).user_data = data;
}

#[no_mangle]
pub unsafe extern "C" fn libinput_get_user_data(
    ctx: *const LibinputContext,
) -> *mut libc::c_void {
    if ctx.is_null() { return std::ptr::null_mut(); } (*ctx).user_data
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_set_user_data(
    dev: *mut LibinputDevice, data: *mut libc::c_void,
) {
    if dev.is_null() { return; } (*dev).user_data = data;
}

#[no_mangle]
pub unsafe extern "C" fn libinput_device_get_user_data(
    dev: *const LibinputDevice,
) -> *mut libc::c_void {
    if dev.is_null() { return std::ptr::null_mut(); } (*dev).user_data
}

// ---------------------------------------------------------------------------
// Suspend / resume
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn libinput_suspend(
    _ctx: *mut LibinputContext,
) {}

#[no_mangle]
pub unsafe extern "C" fn libinput_resume(
    _ctx: *mut LibinputContext,
) -> libc::c_int { 0 }
