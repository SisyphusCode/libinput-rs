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

unsafe fn populate_events(ctx: *mut LibinputContext) {
    if ctx.is_null() { return; }
    let ctx_ref = &mut *ctx;
    let mut tmp: std::collections::VecDeque<LibinputEvent> =
        std::collections::VecDeque::new();
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
    (*ctx).inc_ref(); ctx
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

#[no_mangle]
pub unsafe extern "C" fn libinput_udev_assign_seat(
    ctx: *mut LibinputContext,
    seat_name: *const libc::c_char,
) -> libc::c_int {
    if ctx.is_null() || seat_name.is_null() { return -1; }
    (*ctx).seat.logical_name = CStr::from_ptr(seat_name)
        .to_string_lossy().into_owned();
    let mut tmp: Vec<LibinputEvent> = Vec::new();
    if let Ok(mut backend) = (*ctx).backend.lock() {
        backend.scan_and_open(ctx, &mut tmp);
    }
    for ev in tmp { (*ctx).event_queue.push_back(ev); }
    if !(*ctx).event_queue.is_empty() { (*ctx).signal_fd(); }
    0
}

#[no_mangle]
pub unsafe extern "C" fn libinput_path_add_device(
    ctx: *mut LibinputContext,
    path: *const libc::c_char,
) -> *mut LibinputDevice {
    if ctx.is_null() || path.is_null() { return std::ptr::null_mut(); }
    let devnode = CStr::from_ptr(path).to_string_lossy().into_owned();
    let p = std::path::PathBuf::from(&devnode);
    let mut tmp: Vec<LibinputEvent> = Vec::new();
    if let Ok(mut backend) = (*ctx).backend.lock() {
        backend.try_open(ctx, &p, &mut tmp);
    }
    for ev in tmp { (*ctx).event_queue.push_back(ev); }
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
// FD & dispatch
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
    if !(*ctx).event_queue.is_empty() { (*ctx).signal_fd(); }
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
pub unsafe extern "C" fn libinput_event_pointer_get_time(
    event: *const LibinputEvent,
) -> u32 {
    if event.is_null() { return 0; }
    match &(*event).payload {
        EventPayload::PointerMotion(e)         => (e.time_usec / 1000) as u32,
        EventPayload::PointerMotionAbsolute(e) => (e.time_usec / 1000) as u32,
        EventPayload::PointerButton(e)         => (e.time_usec / 1000) as u32,
        EventPayload::PointerAxis(e)           => (e.time_usec / 1000) as u32,
        _ => 0,
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
pub unsafe extern "C" fn libinput_event_pointer_get_seat_button_count(
    event: *const LibinputEvent,
) -> u32 {
    if event.is_null() { return 0; }
    if let EventPayload::PointerButton(e) = &(*event).payload {
        if e.state == 1 { 1 } else { 0 }
    } else { 0 }
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
pub unsafe extern "C" fn libinput_event_keyboard_get_time(
    event: *const LibinputEvent,
) -> u32 {
    if event.is_null() { return 0; }
    if let EventPayload::KeyboardKey(e) = &(*event).payload {
        (e.time_usec / 1000) as u32
    } else { 0 }
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

#[no_mangle]
pub unsafe extern "C" fn libinput_event_keyboard_get_seat_key_count(
    event: *const LibinputEvent,
) -> u32 {
    if event.is_null() { return 0; }
    if let EventPayload::KeyboardKey(e) = &(*event).payload {
        if e.state >= 1 { 1 } else { 0 }
    } else { 0 }
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
pub unsafe extern "C" fn libinput_event_touch_get_time(
    event: *const LibinputEvent,
) -> u32 {
    if event.is_null() { return 0; }
    match &(*event).payload {
        EventPayload::TouchDown(e)
        | EventPayload::TouchUp(e)
        | EventPayload::TouchMotion(e)
        | EventPayload::TouchCancel(e) => (e.time_usec / 1000) as u32,
        EventPayload::TouchFrame { time_usec } => (*time_usec / 1000) as u32,
        _ => 0,
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
    if event.is_null() { re