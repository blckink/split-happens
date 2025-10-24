use crate::app::PadFilterType;

use evdev::*;

/// Minimum absolute axis magnitude required before an analog stick registers a
/// directional navigation event. This keeps small stick drift from spamming the
/// UI with unintended moves while still remaining responsive.
const ANALOG_DEADZONE: i32 = 12_000;

#[derive(Clone, PartialEq, Copy)]
pub enum DeviceType {
    Gamepad,
    Keyboard,
    Mouse,
    Other,
}

pub enum PadButton {
    Left,
    Right,
    Up,
    Down,
    ABtn,
    BBtn,
    XBtn,
    YBtn,
    StartBtn,
    SelectBtn,

    AKey,
    RKey,
    XKey,
    ZKey,

    RightClick,
}

#[derive(Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub enabled: bool,
    pub device_type: DeviceType,
}

pub struct InputDevice {
    path: String,
    dev: Device,
    enabled: bool,
    device_type: DeviceType,
    has_button_held: bool,
    /// Remembers the last normalized horizontal stick direction so we only
    /// emit navigation events when the player actually changes direction.
    last_axis_x: i32,
    /// Remembers the last normalized vertical stick direction for the same
    /// reason as `last_axis_x` and avoids repeated events while the stick stays
    /// held in one direction.
    last_axis_y: i32,
}
impl InputDevice {
    pub fn name(&self) -> &str {
        self.dev.name().unwrap_or_else(|| "")
    }
    pub fn emoji(&self) -> &str {
        match self.device_type() {
            DeviceType::Gamepad => "🎮",
            DeviceType::Keyboard => "🖮",
            DeviceType::Mouse => "🖱",
            DeviceType::Other => "",
        }
    }
    pub fn fancyname(&self) -> &str {
        match self.dev.input_id().vendor() {
            0x045e => "Xbox Controller",
            0x054c => "PS Controller",
            0x057e => "NT Pro Controller",
            0x28de => "Steam Input",
            _ => self.name(),
        }
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn device_type(&self) -> DeviceType {
        self.device_type
    }
    pub fn has_button_held(&self) -> bool {
        self.has_button_held
    }
    pub fn info(&self) -> DeviceInfo {
        DeviceInfo {
            path: self.path().to_string(),
            enabled: self.enabled(),
            device_type: self.device_type(),
        }
    }
    pub fn poll(&mut self) -> Option<PadButton> {
        let mut btn: Option<PadButton> = None;
        if let Ok(events) = self.dev.fetch_events() {
            for event in events {
                let summary = event.destructure();

                match summary {
                    EventSummary::Key(_, _, 1) => {
                        self.has_button_held = true;
                    }
                    EventSummary::Key(_, _, 0) => {
                        self.has_button_held = false;
                    }
                    _ => {}
                }

                btn = match summary {
                    EventSummary::Key(_, KeyCode::BTN_SOUTH, 1) => Some(PadButton::ABtn),
                    EventSummary::Key(_, KeyCode::BTN_EAST, 1) => Some(PadButton::BBtn),
                    EventSummary::Key(_, KeyCode::BTN_NORTH, 1) => Some(PadButton::XBtn),
                    EventSummary::Key(_, KeyCode::BTN_WEST, 1) => Some(PadButton::YBtn),
                    EventSummary::Key(_, KeyCode::BTN_START, 1) => Some(PadButton::StartBtn),
                    EventSummary::Key(_, KeyCode::BTN_SELECT, 1) => Some(PadButton::SelectBtn),
                    EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_HAT0X, -1) => {
                        Some(PadButton::Left)
                    }
                    EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_HAT0X, 1) => {
                        Some(PadButton::Right)
                    }
                    EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_HAT0Y, -1) => {
                        Some(PadButton::Up)
                    }
                    EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_HAT0Y, 1) => {
                        Some(PadButton::Down)
                    }
                    EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_X, value) => {
                        self.map_horizontal_axis(value).or(btn)
                    }
                    EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_Y, value) => {
                        self.map_vertical_axis(value).or(btn)
                    }
                    //keyboard
                    EventSummary::Key(_, KeyCode::KEY_A, 1) => Some(PadButton::AKey),
                    EventSummary::Key(_, KeyCode::KEY_R, 1) => Some(PadButton::RKey),
                    EventSummary::Key(_, KeyCode::KEY_X, 1) => Some(PadButton::XKey),
                    EventSummary::Key(_, KeyCode::KEY_Z, 1) => Some(PadButton::ZKey),
                    //mouse
                    EventSummary::Key(_, KeyCode::BTN_RIGHT, 1) => Some(PadButton::RightClick),
                    _ => btn,
                };
            }
        }
        btn
    }

    /// Normalizes raw analog stick values into -1, 0, 1 so we can reason about
    /// direction while respecting the configured deadzone.
    fn normalize_axis(value: i32) -> i32 {
        if value <= -ANALOG_DEADZONE {
            -1
        } else if value >= ANALOG_DEADZONE {
            1
        } else {
            0
        }
    }

    /// Converts horizontal stick motion into one-shot left/right navigation
    /// events so the UI can treat the analog stick just like the D-pad.
    fn map_horizontal_axis(&mut self, value: i32) -> Option<PadButton> {
        let direction = Self::normalize_axis(value);
        if direction == self.last_axis_x {
            return None;
        }

        self.last_axis_x = direction;
        match direction {
            -1 => Some(PadButton::Left),
            1 => Some(PadButton::Right),
            _ => None,
        }
    }

    /// Converts vertical stick motion into one-shot up/down navigation events
    /// so analog navigation mirrors the existing D-pad behavior.
    fn map_vertical_axis(&mut self, value: i32) -> Option<PadButton> {
        let direction = Self::normalize_axis(value);
        if direction == self.last_axis_y {
            return None;
        }

        self.last_axis_y = direction;
        match direction {
            -1 => Some(PadButton::Up),
            1 => Some(PadButton::Down),
            _ => None,
        }
    }
}

pub fn scan_input_devices(filter: &PadFilterType) -> Vec<InputDevice> {
    let mut pads: Vec<InputDevice> = Vec::new();
    for dev in evdev::enumerate() {
        let enabled = match filter {
            PadFilterType::All => true,
            PadFilterType::NoSteamInput => dev.1.input_id().vendor() != 0x28de,
            PadFilterType::OnlySteamInput => dev.1.input_id().vendor() == 0x28de,
        };

        let device_type = if dev
            .1
            .supported_keys()
            .map_or(false, |keys| keys.contains(KeyCode::BTN_SOUTH))
        {
            DeviceType::Gamepad
        } else if dev
            .1
            .supported_keys()
            .map_or(false, |keys| keys.contains(KeyCode::BTN_LEFT))
        {
            DeviceType::Mouse
        } else if dev
            .1
            .supported_keys()
            .map_or(false, |keys| keys.contains(KeyCode::KEY_SPACE))
        {
            DeviceType::Keyboard
        } else {
            DeviceType::Other
        };

        if device_type != DeviceType::Other {
            if dev.1.set_nonblocking(true).is_err() {
                println!("Failed to set non-blocking mode for {}", dev.0.display());
                continue;
            }
            pads.push(InputDevice {
                path: dev.0.to_str().unwrap().to_string(),
                dev: dev.1,
                enabled,
                device_type,
                has_button_held: false,
                last_axis_x: 0,
                last_axis_y: 0,
            });
        }
    }
    pads.sort_by_key(|pad| pad.path().to_string());
    pads
}
