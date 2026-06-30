use evdev::{Device, EventType, InputEvent, AbsoluteAxisType, RelativeAxisType, Key};
use std::error::Error;
use std::time::{Instant, Duration};
use std::path::{Path, PathBuf};
use log::{info, warn};

pub struct DeviceWrapper {
    pub device: Device,
    pub path: PathBuf,
    pub is_absolute: bool,
    pub is_keyboard: bool,
    
    // Pointer state
    pub touch_active: bool,
    pub touch_fingers: u32,
    pub last_x: Option<i32>,
    pub last_y: Option<i32>,
    pub current_dx: i32,
    pub current_dy: i32,
    pub remainder_x: f32,
    pub remainder_y: f32,
    
    // Tap-to-click state
    pub touch_start_time: Option<Instant>,
    pub tap_emitted: bool,

    // DWT state
    pub last_typing_time: Option<Instant>,
}

impl DeviceWrapper {
    pub fn new(device: Device, path: PathBuf) -> Self {
        let is_absolute = device.supported_events().contains(EventType::ABSOLUTE);
        let is_keyboard = device.supported_events().contains(EventType::KEY) && 
                          device.supported_keys().map_or(false, |keys| keys.contains(Key::KEY_A));
        
        Self {
            device,
            path,
            is_absolute,
            is_keyboard,
            touch_active: false,
            touch_fingers: 0,
            last_x: None,
            last_y: None,
            current_dx: 0,
            current_dy: 0,
            remainder_x: 0.0,
            remainder_y: 0.0,
            touch_start_time: None,
            tap_emitted: false,
            last_typing_time: None,
        }
    }

    pub fn process_event(
        &mut self,
        ev: InputEvent,
        v_device: &mut crate::virtual_device::VirtualDevice,
        config: &crate::config::InputConfig,
        last_global_typing_time: Option<Instant>,
    ) -> Result<(), Box<dyn Error>> {
        if self.is_keyboard {
            // Track typing for DWT
            if ev.event_type() == EventType::KEY && ev.value() != 0 {
                // value 1 is press, 2 is repeat
                self.last_typing_time = Some(Instant::now());
            }
            return Ok(());
        }

        if !self.is_absolute {
            // For relative devices (like trackpoints), just forward the event directly.
            v_device.emit_raw(ev)?;
            return Ok(());
        }

        // Disable-While-Typing (DWT) check
        let mut dwt_active = false;
        if config.disable_while_typing {
            if let Some(typing_time) = last_global_typing_time {
                if typing_time.elapsed() < Duration::from_millis(500) {
                    dwt_active = true;
                }
            }
        }

        // For absolute devices (touchpads), convert coordinate events into relative movements
        match ev.event_type() {
            EventType::KEY => {
                if ev.code() == Key::BTN_TOUCH.code() {
                    self.touch_active = ev.value() != 0;
                    if self.touch_active {
                        self.touch_start_time = Some(Instant::now());
                        self.tap_emitted = false;
                        self.touch_fingers = 1;
                        self.last_x = None;
                        self.last_y = None;
                    } else {
                        // Reset tracking state when the finger is lifted
                        
                        // Tap-to-click logic
                        if config.tap_to_click && !self.tap_emitted && !dwt_active {
                            if let Some(start) = self.touch_start_time {
                                if start.elapsed() < Duration::from_millis(250) {
                                    // Emit click
                                    v_device.emit_raw(InputEvent::new(EventType::KEY, Key::BTN_LEFT.code(), 1)).unwrap_or(());
                                    v_device.emit_raw(InputEvent::new(EventType::SYNCHRONIZATION, 0, 0)).unwrap_or(());
                                    v_device.emit_raw(InputEvent::new(EventType::KEY, Key::BTN_LEFT.code(), 0)).unwrap_or(());
                                }
                            }
                        }

                        self.last_x = None;
                        self.last_y = None;
                        self.current_dx = 0;
                        self.current_dy = 0;
                        self.touch_start_time = None;
                        self.touch_fingers = 0;
                    }
                } else if ev.code() == Key::BTN_TOOL_DOUBLETAP.code() {
                    if ev.value() != 0 {
                        self.touch_fingers = 2;
                    } else if self.touch_active {
                        self.touch_fingers = 1;
                    }
                } else if ev.code() == Key::BTN_TOOL_TRIPLETAP.code() {
                    if ev.value() != 0 {
                        self.touch_fingers = 3;
                    } else if self.touch_active {
                        self.touch_fingers = 2;
                    }
                }
                
                // Only emit standard buttons (left, right, middle) directly
                if ev.code() == Key::BTN_LEFT.code() || ev.code() == Key::BTN_RIGHT.code() || ev.code() == Key::BTN_MIDDLE.code() {
                    v_device.emit_raw(ev)?;
                }
            }
            EventType::ABSOLUTE => {
                let code = ev.code();
                if code == AbsoluteAxisType::ABS_X.0 {
                    let val = ev.value();
                    if self.touch_active {
                        if let Some(prev_x) = self.last_x {
                            self.current_dx += val - prev_x;
                        }
                    }
                    self.last_x = Some(val);
                } else if code == AbsoluteAxisType::ABS_Y.0 {
                    let val = ev.value();
                    if self.touch_active {
                        if let Some(prev_y) = self.last_y {
                            self.current_dy += val - prev_y;
                        }
                    }
                    self.last_y = Some(val);
                }
            }
            EventType::SYNCHRONIZATION => {
                if ev.code() == 0 { // SYN_REPORT code is 0
                    log::info!("SYN_REPORT: touch_active={}, touch_fingers={}, dx={}, dy={}, dwt_active={}", self.touch_active, self.touch_fingers, self.current_dx, self.current_dy, dwt_active);
                    if dwt_active {
                        // Throw away movement completely
                        self.current_dx = 0;
                        self.current_dy = 0;
                        self.remainder_x = 0.0;
                        self.remainder_y = 0.0;
                        self.tap_emitted = true; // prevent taps from happening
                    } else if self.current_dx != 0 || self.current_dy != 0 {
                        self.tap_emitted = true; // Moved enough to cancel tap

                        if self.touch_fingers == 1 {
                            // Touchpads emit high-resolution absolute coordinates. We must scale these down 
                            // so they feel like a standard relative mouse to the compositor.
                            let hardware_scale = 0.18; // Increased from 0.12 for faster speed
                            
                            let total_x = (self.current_dx as f32 * hardware_scale) * config.pointer_acceleration + self.remainder_x;
                            let total_y = (self.current_dy as f32 * hardware_scale) * config.pointer_acceleration + self.remainder_y;

                            let emit_x = total_x.round() as i32;
                            let emit_y = total_y.round() as i32;

                            self.remainder_x = total_x - emit_x as f32;
                            self.remainder_y = total_y - emit_y as f32;

                            if emit_x != 0 {
                                v_device.emit_raw(InputEvent::new(
                                    EventType::RELATIVE,
                                    RelativeAxisType::REL_X.0,
                                    emit_x,
                                ))?;
                            }
                            if emit_y != 0 {
                                v_device.emit_raw(InputEvent::new(
                                    EventType::RELATIVE,
                                    RelativeAxisType::REL_Y.0,
                                    emit_y,
                                ))?;
                            }
                        } else if self.touch_fingers == 2 {
                            // Scroll scaling
                            let scroll_scale = 0.02; // Wheel ticks are integers, scale heavily down
                            let total_y = (self.current_dy as f32 * scroll_scale) + self.remainder_y;
                            let emit_wheel = total_y.round() as i32;
                            self.remainder_y = total_y - emit_wheel as f32;

                            if emit_wheel != 0 {
                                let mut final_wheel = emit_wheel;
                                // REL_WHEEL typically uses 1 for up, -1 for down. 
                                // Moving fingers down the touchpad increases Y.
                                if config.natural_scrolling {
                                    final_wheel = -final_wheel;
                                }
                                v_device.emit_raw(InputEvent::new(
                                    EventType::RELATIVE,
                                    RelativeAxisType::REL_WHEEL.0,
                                    final_wheel,
                                ))?;
                            }
                            // Also clear dx so it doesn't build up during scroll
                            self.remainder_x = 0.0;
                        }

                        self.current_dx = 0;
                        self.current_dy = 0;
                    }
                    v_device.emit_raw(ev)?;
                } else {
                    v_device.emit_raw(ev)?;
                }
            }
            _ => {
                // Do not emit unknown events from absolute devices to the relative virtual device
            }
        }

        Ok(())
    }
}

fn disable_autosuspend(dev_path: &std::path::Path) {
    if let Some(file_name) = dev_path.file_name() {
        let mut sys_path = std::path::PathBuf::from("/sys/class/input");
        sys_path.push(file_name);
        sys_path.push("device");
        
        for _ in 0..4 {
            let power_control = sys_path.join("power/control");
            if power_control.exists() {
                if let Err(e) = std::fs::write(&power_control, "on") {
                    log::warn!("Failed to disable autosuspend at {:?}: {}", power_control, e);
                } else {
                    log::info!("Automatically disabled hardware autosuspend for {:?}", dev_path);
                }
            }
            sys_path.push("..");
        }
    }
}

pub fn try_open_device(path: &std::path::Path) -> Option<DeviceWrapper> {
    if let Ok(mut device) = evdev::Device::open(path) {
        let name = device.name().unwrap_or("Unknown").to_lowercase();
        
        // Ignore virtual devices we created to prevent infinite loops
        if name.contains("virtual pointer") {
            return None;
        }
        
        let is_pointer = name.contains("touchpad") || name.contains("trackpoint") || name.contains("elan") || name.contains("synaptics") || name.contains("mouse");
        let is_keyboard = device.supported_events().contains(EventType::KEY) && 
                          device.supported_keys().map_or(false, |keys| keys.contains(Key::KEY_A));
        
        if is_pointer {
            log::info!("Found target pointer hardware: {} at {:?}", name, path);
            if let Ok(_) = device.grab() {
                // Attempt to aggressively disable autosuspend for the hardware to prevent firmware freezing
                disable_autosuspend(path);
                return Some(DeviceWrapper::new(device, path.to_path_buf()));
            } else {
                log::warn!("Failed to grab pointer device: {:?}", path);
            }
        } else if is_keyboard {
            log::info!("Found keyboard for DWT monitoring: {} at {:?}", name, path);
            return Some(DeviceWrapper::new(device, path.to_path_buf()));
        }
    }
    None
}

pub fn scan_input_devices() -> Result<Vec<DeviceWrapper>, Box<dyn Error>> {
    let mut tracked = Vec::new();
    
    for (path, _) in evdev::enumerate() {
        if let Some(wrapper) = try_open_device(&path) {
            tracked.push(wrapper);
        }
    }
    
    Ok(tracked)
}
