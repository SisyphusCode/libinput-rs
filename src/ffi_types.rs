//! Opaque C-compatible types exposed through the libinput ABI.

use std::collections::VecDeque;
use std::ffi::CString;
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;

use crate::backend::BackendState;

// ---------------------------------------------------------------------------
// libinput_interface
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct LibinputInterface {
    pub open_restricted: Option<
        unsafe extern "C" fn(
            path: *const libc::c_char,
            flags: libc::c_int,
            user_data: *mut libc::c_void,
        ) -> libc::c_int,
    >,
    pub close_restricted:
        Option<unsafe extern "C" fn(fd: libc::c_int, user_data: *mut libc::c_void)>,
}

// ---------------------------------------------------------------------------
// libinput_event_type
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types, dead_code)]
pub enum LibinputEventType {
    LIBINPUT_EVENT_NONE = 0,
    LIBINPUT_EVENT_DEVICE_ADDED = 1,
    LIBINPUT_EVENT_DEVICE_REMOVED = 2,
    LIBINPUT_EVENT_KEYBOARD_KEY = 300,
    LIBINPUT_EVENT_POINTER_MOTION = 400,
    LIBINPUT_EVENT_POINTER_MOTION_ABSOLUTE = 401,
    LIBINPUT_EVENT_POINTER_BUTTON = 402,
    LIBINPUT_EVENT_POINTER_AXIS = 403,
    LIBINPUT_EVENT_POINTER_SCROLL_WHEEL = 404,
    LIBINPUT_EVENT_POINTER_SCROLL_FINGER = 405,
    LIBINPUT_EVENT_POINTER_SCROLL_CONTINUOUS = 406,
    LIBINPUT_EVENT_TOUCH_DOWN = 500,
    LIBINPUT_EVENT_TOUCH_UP = 501,
    LIBINPUT_EVENT_TOUCH_MOTION = 502,
    LIBINPUT_EVENT_TOUCH_CANCEL = 503,
    LIBINPUT_EVENT_TOUCH_FRAME = 504,
    LIBINPUT_EVENT_GESTURE_SWIPE_BEGIN = 800,
    LIBINPUT_EVENT_GESTURE_SWIPE_UPDATE = 801,
    LIBINPUT_EVENT_GESTURE_SWIPE_END = 802,
    LIBINPUT_EVENT_GESTURE_PINCH_BEGIN = 803,
    LIBINPUT_EVENT_GESTURE_PINCH_UPDATE = 804,
    LIBINPUT_EVENT_GESTURE_PINCH_END = 805,
    LIBINPUT_EVENT_SWITCH_TOGGLE = 900,
    LIBINPUT_EVENT_TABLET_TOOL_AXIS = 600,
    LIBINPUT_EVENT_TABLET_TOOL_PROXIMITY = 601,
    LIBINPUT_EVENT_TABLET_TOOL_TIP = 602,
    LIBINPUT_EVENT_TABLET_TOOL_BUTTON = 603,
    LIBINPUT_EVENT_TABLET_PAD_BUTTON = 700,
    LIBINPUT_EVENT_TABLET_PAD_RING = 701,
    LIBINPUT_EVENT_TABLET_PAD_STRIP = 702,
}

// ---------------------------------------------------------------------------
// Event payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PointerMotionEvent {
    pub time_usec: u64,
    pub dx: f64,
    pub dy: f64,
    pub dx_unaccel: f64,
    pub dy_unaccel: f64,
}

#[derive(Debug, Clone)]
pub struct PointerMotionAbsoluteEvent {
    pub time_usec: u64,
    pub abs_x: f64,
    pub abs_y: f64,
}

#[derive(Debug, Clone)]
pub struct PointerButtonEvent {
    pub time_usec: u64,
    pub button: u32,
    pub state: u32,
}

#[derive(Debug, Clone)]
pub struct PointerAxisEvent {
    pub time_usec: u64,
    pub axis: u32,
    pub value: f64,
    pub value_discrete: i32,
    pub source: u32,
}

#[derive(Debug, Clone)]
pub struct KeyboardKeyEvent {
    pub time_usec: u64,
    pub key: u32,
    pub state: u32,
}

#[derive(Debug, Clone)]
pub struct TouchEvent {
    pub time_usec: u64,
    pub slot: i32,
    pub seat_slot: i32,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct GestureEvent {
    pub time_usec: u64,
    pub finger_count: i32,
    pub dx: f64,
    pub dy: f64,
    pub scale: f64,
    pub angle: f64,
    pub cancelled: bool,
}

#[derive(Debug, Clone)]
pub struct SwitchEvent {
    pub time_usec: u64,
    pub switch: u32,
    pub state: u32,
}

#[derive(Debug, Clone)]
pub enum EventPayload {
    PointerMotion(PointerMotionEvent),
    PointerMotionAbsolute(PointerMotionAbsoluteEvent),
    PointerButton(PointerButtonEvent),
    PointerAxis(PointerAxisEvent),
    KeyboardKey(KeyboardKeyEvent),
    TouchDown(TouchEvent),
    TouchUp(TouchEvent),
    TouchMotion(TouchEvent),
    TouchCancel(TouchEvent),
    TouchFrame { time_usec: u64 },
    GestureSwipeBegin(GestureEvent),
    GestureSwipeUpdate(GestureEvent),
    GestureSwipeEnd(GestureEvent),
    GesturePinchBegin(GestureEvent),
    GesturePinchUpdate(GestureEvent),
    GesturePinchEnd(GestureEvent),
    SwitchToggle(SwitchEvent),
    DeviceAdded,
    DeviceRemoved,
}

// ---------------------------------------------------------------------------
// LibinputEvent
// ---------------------------------------------------------------------------

pub struct LibinputEvent {
    pub event_type: LibinputEventType,
    pub payload: EventPayload,
    pub context: *mut LibinputContext,
    pub device: *mut LibinputDevice,
}

// ---------------------------------------------------------------------------
// LibinputDevice
// ---------------------------------------------------------------------------

fn make_cstring(s: &str) -> CString {
    CString::new(s).unwrap_or_else(|_| CString::new("").unwrap())
}

#[allow(dead_code)]
pub struct LibinputDevice {
    // CString fields so .as_ptr() returns valid null-terminated C strings.
    pub name: CString,
    pub sysname: CString,
    pub devnode: CString,
    pub vendor_id: u32,
    pub product_id: u32,
    pub has_keyboard: bool,
    pub has_pointer: bool,
    pub has_touch: bool,
    pub has_gesture: bool,
    pub has_switch: bool,
    pub has_tablet: bool,
    pub tap_enabled: bool,
    pub tap_button_map: u32,
    pub natural_scroll: bool,
    pub accel_speed: f64,
    pub accel_profile: u32,
    pub left_handed: bool,
    pub scroll_method: u32,
    pub click_method: u32,
    pub middle_emulation: bool,
    pub dwt_enabled: bool,
    pub calibration: [f32; 6],
    pub refcount: AtomicI32,
    pub user_data: *mut libc::c_void,
    // Back-pointer to owning seat so libinput_device_get_seat is non-null.
    pub seat: *mut LibinputSeat,
}

unsafe impl Send for LibinputDevice {}

impl LibinputDevice {
    pub fn new(name: &str, devnode: &str) -> Self {
        let sysname = std::path::Path::new(devnode)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        Self {
            name: make_cstring(name),
            sysname: make_cstring(sysname),
            devnode: make_cstring(devnode),
            vendor_id: 0,
            product_id: 0,
            has_keyboard: false,
            has_pointer: true,
            has_touch: false,
            has_gesture: false,
            has_switch: false,
            has_tablet: false,
            tap_enabled: true,
            tap_button_map: 0,
            natural_scroll: true,
            accel_speed: 0.0,
            accel_profile: 1,
            left_handed: false,
            scroll_method: 2,
            click_method: 1,
            middle_emulation: false,
            dwt_enabled: true,
            calibration: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            refcount: AtomicI32::new(1),
            user_data: std::ptr::null_mut(),
            seat: std::ptr::null_mut(),
        }
    }
}

// ---------------------------------------------------------------------------
// LibinputSeat
// ---------------------------------------------------------------------------

pub struct LibinputSeat {
    pub physical_name: CString,
    pub logical_name: CString,
}

impl LibinputSeat {
    pub fn new(physical: &str, logical: &str) -> Self {
        Self {
            physical_name: make_cstring(physical),
            logical_name: make_cstring(logical),
        }
    }
}

// ---------------------------------------------------------------------------
// LibinputContext
// ---------------------------------------------------------------------------

pub struct LibinputContext {
    pub interface: *const LibinputInterface,
    pub user_data: *mut libc::c_void,
    pub event_fd: RawFd,
    pub event_queue: VecDeque<LibinputEvent>,
    pub devices: Vec<*mut LibinputDevice>,
    pub seat: Box<LibinputSeat>,
    pub refcount: AtomicI32,
    pub log_handler: Option<
        unsafe extern "C" fn(ctx: *mut LibinputContext, priority: u32, msg: *const libc::c_char),
    >,
    pub backend: Mutex<BackendState>,
}

unsafe impl Send for LibinputContext {}
unsafe impl Sync for LibinputContext {}

impl LibinputContext {
    pub fn new(interface: *const LibinputInterface, user_data: *mut libc::c_void) -> Self {
        let efd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        Self {
            interface,
            user_data,
            event_fd: efd,
            event_queue: VecDeque::new(),
            devices: Vec::new(),
            seat: Box::new(LibinputSeat::new("seat0", "default")),
            refcount: AtomicI32::new(1),
            log_handler: None,
            backend: Mutex::new(BackendState::new()),
        }
    }

    pub fn signal_fd(&self) {
        let val: u64 = 1;
        unsafe {
            libc::write(self.event_fd, &val as *const u64 as *const libc::c_void, 8);
        }
    }

    pub fn drain_fd(&self) {
        let mut buf: u64 = 0;
        unsafe {
            libc::read(self.event_fd, &mut buf as *mut u64 as *mut libc::c_void, 8);
        }
    }

    pub fn inc_ref(&self) {
        self.refcount.fetch_add(1, Ordering::SeqCst);
    }
    pub fn dec_ref(&self) -> i32 {
        self.refcount.fetch_sub(1, Ordering::SeqCst) - 1
    }
}

impl Drop for LibinputContext {
    fn drop(&mut self) {
        if self.event_fd >= 0 {
            unsafe {
                libc::close(self.event_fd);
            }
        }
        for dev_ptr in self.devices.drain(..) {
            if !dev_ptr.is_null() {
                unsafe {
                    drop(Box::from_raw(dev_ptr));
                }
            }
        }
    }
}
