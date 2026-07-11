use evdev::{uinput::VirtualDeviceBuilder, InputEvent, Key, RelativeAxisType};
use std::error::Error;
use log::info;

pub struct VirtualDevice {
    device: evdev::uinput::VirtualDevice,
}

impl Drop for VirtualDevice {
    fn drop(&mut self) {
        // The VirtualDevice will be automatically destroyed when dropped
        info!("Virtual input device destroyed");
    }
}

impl VirtualDevice {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let mut keys = evdev::AttributeSet::new();
        keys.insert(Key::BTN_LEFT);
        keys.insert(Key::BTN_RIGHT);
        keys.insert(Key::BTN_MIDDLE);

        let mut rel_axes = evdev::AttributeSet::new();
        rel_axes.insert(RelativeAxisType::REL_X);
        rel_axes.insert(RelativeAxisType::REL_Y);
        rel_axes.insert(RelativeAxisType::REL_WHEEL);

        // Stripping absolute axes ensures Wayland treats this strictly as a high-speed mouse
        let device = VirtualDeviceBuilder::new()?
            .name("libinput-rs Virtual Pointer")
            .with_keys(&keys)?
            .with_relative_axes(&rel_axes)?
            .build()?;

        Ok(Self { device })
    }

    pub fn emit_raw(&mut self, event: InputEvent) -> Result<(), Box<dyn Error>> {
        self.device.emit(&[event])?;
        Ok(())
    }
}
