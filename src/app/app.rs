use std::collections::HashMap;
use std::thread::sleep;

use super::config::*;
use crate::game::Game::HandlerRef;
use crate::game::*;
use crate::input::*;
use crate::instance::*;
use crate::launch::launch_game;
use crate::paths::*;
use crate::util::*;

use eframe::egui::{self, Key};

#[derive(Eq, PartialEq)]
pub enum MenuPage {
    Home,
    Settings,
    Profiles,
    Game,
    Instances,
}

#[derive(Eq, PartialEq)]
pub enum SettingsPage {
    General,
    Gamescope,
}

pub struct PartyApp {
    pub needs_update: bool,
    pub options: PartyConfig,
    pub cur_page: MenuPage,
    pub settings_page: SettingsPage,
    pub infotext: String,

    pub input_devices: Vec<InputDevice>,
    pub instances: Vec<Instance>,
    pub instance_add_dev: Option<usize>,
    pub games: Vec<Game>,
    pub selected_game: usize,
    pub profiles: Vec<String>,
    pub proton_versions: Vec<ProtonInstall>,

    pub loading_msg: Option<String>,
    pub loading_since: Option<std::time::Instant>,
    #[allow(dead_code)]
    pub task: Option<std::thread::JoinHandle<()>>,
    /// Target interval between egui repaints so Steam Deck builds can dial in
    /// smoother menus when docked without sacrificing handheld battery life.
    pub repaint_interval: std::time::Duration,
    /// Tracks when the input list was last synchronized so new controllers can
    /// be discovered automatically without hammering the kernel every frame.
    pub last_input_scan: std::time::Instant,
}

macro_rules! cur_game {
    ($self:expr) => {
        &$self.games[$self.selected_game]
    };
}

impl Default for PartyApp {
    fn default() -> Self {
        Self::with_repaint_interval(std::time::Duration::from_millis(33))
    }
}

impl PartyApp {
    /// Builds the full PartyDeck UI with a specific repaint interval so the
    /// main application can align frame pacing with the detected display.
    pub fn with_repaint_interval(repaint_interval: std::time::Duration) -> Self {
        let options = load_cfg();
        let input_devices = scan_input_devices(&options.pad_filter_type);
        Self {
            needs_update: check_for_partydeck_update(),
            options,
            cur_page: MenuPage::Home,
            settings_page: SettingsPage::General,
            infotext: String::new(),
            input_devices,
            instances: Vec::new(),
            instance_add_dev: None,
            games: scan_all_games(),
            selected_game: 0,
            profiles: Vec::new(),
            proton_versions: discover_proton_versions(),
            loading_msg: None,
            loading_since: None,
            task: None,
            repaint_interval,
            last_input_scan: std::time::Instant::now(),
        }
    }
}

impl eframe::App for PartyApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if !raw_input.focused || self.task.is_some() {
            return;
        }
        match self.cur_page {
            MenuPage::Instances => self.handle_devices_instance_menu(),
            _ => self.handle_gamepad_gui(raw_input),
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Opportunistically refresh the device cache so Bluetooth pads appear
        // without requiring the user to mash the manual rescan button.
        self.maybe_refresh_input_devices();

        egui::TopBottomPanel::top("menu_nav_panel").show(ctx, |ui| {
            if self.task.is_some() {
                ui.disable();
            }
            self.display_panel_top(ui);
        });

        if self.cur_page != MenuPage::Home {
            // Keep the traditional left navigation only when the user is in a
            // detail view so the new tile-based home screen can stretch across
            // the full window.
            egui::SidePanel::left("games_panel")
                .resizable(false)
                .exact_width(200.0)
                .show(ctx, |ui| {
                    if self.task.is_some() {
                        ui.disable();
                    }
                    self.display_panel_left(ui);
                });
        }

        if self.cur_page == MenuPage::Instances {
            egui::SidePanel::right("devices_panel")
                .resizable(false)
                .exact_width(180.0)
                .show(ctx, |ui| {
                    if self.task.is_some() {
                        ui.disable();
                    }
                    self.display_panel_right(ui, ctx);
                });
        }

        if (self.cur_page != MenuPage::Home) && (self.cur_page != MenuPage::Instances) {
            self.display_panel_bottom(ctx);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.task.is_some() {
                ui.disable();
            }
            match self.cur_page {
                MenuPage::Home => self.display_page_main(ui),
                MenuPage::Settings => self.display_page_settings(ui),
                MenuPage::Profiles => self.display_page_profiles(ui),
                MenuPage::Game => self.display_page_game(ui),
                MenuPage::Instances => self.display_page_instances(ui),
            }
        });

        if let Some(handle) = self.task.take() {
            if handle.is_finished() {
                let _ = handle.join();
                self.loading_since = None;
                self.loading_msg = None;
            } else {
                self.task = Some(handle);
            }
        }
        if let Some(start) = self.loading_since {
            if start.elapsed() > std::time::Duration::from_secs(60) {
                // Give up waiting after one minute
                self.loading_msg = Some("Operation timed out".to_string());
            }
        }
        if let Some(msg) = &self.loading_msg {
            egui::Area::new("loading".into())
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgba_premultiplied(0, 0, 0, 192))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::symmetric(16, 12))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add(egui::widgets::Spinner::new().size(40.0));
                                ui.add_space(8.0);
                                ui.label(msg);
                            });
                        });
                });
        }
        if ctx.input(|input| input.focused) {
            ctx.request_repaint_after(self.repaint_interval);
        }
    }
}

impl PartyApp {
    pub fn spawn_task<F>(&mut self, msg: &str, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.loading_msg = Some(msg.to_string());
        self.loading_since = Some(std::time::Instant::now());
        self.task = Some(std::thread::spawn(f));
    }

    fn handle_gamepad_gui(&mut self, raw_input: &mut egui::RawInput) {
        let mut key: Option<egui::Key> = None;
        let mut trigger_instances = false;
        for pad in &mut self.input_devices {
            if !pad.enabled() {
                continue;
            }
            match pad.poll() {
                Some(PadButton::ABtn) => key = Some(Key::Enter),
                Some(PadButton::BBtn) => self.cur_page = MenuPage::Home,
                Some(PadButton::XBtn) => {
                    self.profiles = scan_profiles(false);
                    self.cur_page = MenuPage::Profiles;
                }
                Some(PadButton::YBtn) => self.cur_page = MenuPage::Settings,
                Some(PadButton::SelectBtn) => key = Some(Key::Tab),
                Some(PadButton::StartBtn) => {
                    if self.cur_page == MenuPage::Game {
                        trigger_instances = true;
                    }
                }
                Some(PadButton::Up) => key = Some(Key::ArrowUp),
                Some(PadButton::Down) => key = Some(Key::ArrowDown),
                Some(PadButton::Left) => key = Some(Key::ArrowLeft),
                Some(PadButton::Right) => key = Some(Key::ArrowRight),
                Some(_) => {}
                None => {}
            }
        }

        if trigger_instances {
            self.open_instances_for(self.selected_game);
        }

        if let Some(key) = key {
            raw_input.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            });
        }
    }

    /// Refreshes the cached Proton installation list so users can discover new
    /// compatibility tools without restarting PartyDeck.
    pub fn refresh_proton_versions(&mut self) {
        self.proton_versions = discover_proton_versions();
    }

    /// Opens the handler/executable picker and refreshes the library so newly
    /// installed entries immediately appear in the UI.
    pub fn prompt_add_game(&mut self) {
        if let Err(err) = add_game() {
            println!("Couldn't add game: {err}");
            msg("Error", &format!("Couldn't add game: {err}"));
        }

        let dir_tmp = PATH_PARTY.join("tmp");
        if dir_tmp.exists() {
            if let Err(err) = std::fs::remove_dir_all(&dir_tmp) {
                eprintln!("Failed to remove temporary handler files: {err}");
            }
        }

        self.reload_games();
    }

    /// Rebuilds the game list while preserving the previously selected entry
    /// whenever possible so the UI does not jump unexpectedly.
    pub fn reload_games(&mut self) {
        let previous_id = self
            .games
            .get(self.selected_game)
            .map(|game| game.persistent_id());

        let refreshed = scan_all_games();

        if refreshed.is_empty() {
            self.selected_game = 0;
        } else if let Some(prev) = previous_id {
            if let Some(idx) = refreshed
                .iter()
                .position(|game| game.persistent_id() == prev)
            {
                self.selected_game = idx;
            } else {
                self.selected_game = 0;
            }
        } else {
            self.selected_game = 0;
        }

        if !refreshed.is_empty() && self.selected_game >= refreshed.len() {
            self.selected_game = 0;
        }

        self.games = refreshed;
    }

    /// Routes the user to the instance assignment screen for the selected tile
    /// so profiles can be linked with a single tap from the home grid.
    pub fn open_instances_for(&mut self, game_index: usize) {
        if game_index >= self.games.len() {
            return;
        }

        self.selected_game = game_index;
        self.instances.clear();
        self.profiles = scan_profiles(true);
        self.instance_add_dev = None;
        self.cur_page = MenuPage::Instances;
    }

    /// Returns the Proton installation that matches the current settings
    /// value, accounting for the implicit GE-Proton default.
    pub fn selected_proton_install(&self) -> Option<&ProtonInstall> {
        let trimmed = self.options.proton_version.trim();
        if trimmed.is_empty() {
            return self
                .proton_versions
                .iter()
                .find(|install| install.matches("GE-Proton"));
        }

        self.proton_versions
            .iter()
            .find(|install| install.matches(trimmed))
    }

    /// Generates the label shown in the Proton selection combo box so users can
    /// easily identify whether a custom path or a discovered build is active.
    pub fn proton_dropdown_label(&self) -> String {
        if let Some(install) = self.selected_proton_install() {
            return install.display_label();
        }

        let trimmed = self.options.proton_version.trim();
        if trimmed.is_empty() {
            if self
                .proton_versions
                .iter()
                .any(|install| install.matches("GE-Proton"))
            {
                "Auto (GE-Proton)".to_string()
            } else {
                "Auto (GE-Proton missing)".to_string()
            }
        } else {
            format!("Custom: {trimmed}")
        }
    }

    fn handle_devices_instance_menu(&mut self) {
        let mut i = 0;
        while i < self.input_devices.len() {
            if !self.input_devices[i].enabled() {
                i += 1;
                continue;
            }
            match self.input_devices[i].poll() {
                Some(PadButton::ABtn) | Some(PadButton::ZKey) | Some(PadButton::RightClick) => {
                    if self.input_devices[i].device_type() != DeviceType::Gamepad
                        && !self.options.kbm_support
                    {
                        continue;
                    }

                    match self.instance_add_dev {
                        Some(inst) => {
                            self.instance_add_dev = None;
                            if !self.instances[inst].devices.contains(&i) {
                                self.instances[inst].devices.push(i);
                            }
                        }
                        None => {
                            // Restore the last-used profile for this slot when starting a
                            // fresh instance so the join screen remembers previous
                            // assignments per game.
                            let slot_index = self.instances.len();
                            let default_profile = self.default_profile_index_for_slot(slot_index);
                            self.instances.push(Instance {
                                devices: vec![i],
                                profname: String::new(),
                                profselection: default_profile,
                                width: 0,
                                height: 0,
                            });
                        }
                    }
                }
                Some(PadButton::BBtn) | Some(PadButton::XKey) => {
                    if self.instance_add_dev != None {
                        self.instance_add_dev = None;
                    } else if self.is_device_in_any_instance(i) {
                        self.remove_device(i);
                    } else if self.instances.len() < 1 {
                        self.cur_page = MenuPage::Game;
                    }
                }
                Some(PadButton::YBtn) | Some(PadButton::AKey) => {
                    if self.instance_add_dev == None {
                        if let Some((instance, _)) = self.find_device_in_instance(i) {
                            self.instance_add_dev = Some(instance);
                        }
                    }
                }
                Some(PadButton::StartBtn) => {
                    if self.instances.len() > 0 && self.is_device_in_any_instance(i) {
                        self.prepare_game_launch();
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn is_device_in_any_instance(&mut self, dev: usize) -> bool {
        for instance in &self.instances {
            if instance.devices.contains(&dev) {
                return true;
            }
        }
        false
    }

    fn find_device_in_instance(&mut self, dev: usize) -> Option<(usize, usize)> {
        for (i, instance) in self.instances.iter().enumerate() {
            for (d, device) in instance.devices.iter().enumerate() {
                if device == &dev {
                    return Some((i, d));
                }
            }
        }
        None
    }

    /// Resolves the preferred profile index for a newly created instance slot so
    /// returning to the join screen preserves each player's last selection.
    fn default_profile_index_for_slot(&self, slot_index: usize) -> usize {
        if let HandlerRef(_) = cur_game!(self) {
            let game_id = cur_game!(self).persistent_id();
            if let Some(assignments) = self.options.last_profile_assignments.get(&game_id) {
                if let Some(saved_name) = assignments.get(slot_index) {
                    if let Some(idx) = self
                        .profiles
                        .iter()
                        .position(|profile| profile == saved_name)
                    {
                        return idx;
                    }
                }
            }
        }
        0
    }

    pub fn remove_device(&mut self, dev: usize) {
        if let Some((instance_index, device_index)) = self.find_device_in_instance(dev) {
            self.remove_device_at(instance_index, device_index);
        }
    }

    /// Removes a device from a specific instance slot so duplicate controller
    /// assignments can be cleaned up without touching other players.
    pub fn remove_device_at(&mut self, instance_index: usize, device_index: usize) {
        if let Some(instance) = self.instances.get_mut(instance_index) {
            if device_index < instance.devices.len() {
                instance.devices.remove(device_index);
            }
        }
        self.prune_empty_instances();
    }

    /// Prunes stale instance assignments and remaps surviving devices after a
    /// background rescan so controller indices stay consistent.
    fn sync_input_devices(&mut self) {
        let old_paths: Vec<String> = self
            .input_devices
            .iter()
            .map(|device| device.path().to_string())
            .collect();
        let new_devices = scan_input_devices(&self.options.pad_filter_type);
        let new_paths: Vec<String> = new_devices
            .iter()
            .map(|device| device.path().to_string())
            .collect();

        if new_paths == old_paths {
            return;
        }

        let mut path_to_index: HashMap<String, usize> = HashMap::new();
        for (idx, path) in new_paths.iter().enumerate() {
            path_to_index.insert(path.clone(), idx);
        }

        for instance in &mut self.instances {
            let mut remapped: Vec<usize> = Vec::with_capacity(instance.devices.len());
            for &old_index in &instance.devices {
                if let Some(old_path) = old_paths.get(old_index) {
                    if let Some(&new_index) = path_to_index.get(old_path) {
                        if !remapped.contains(&new_index) {
                            remapped.push(new_index);
                        }
                    }
                }
            }
            instance.devices = remapped;
        }

        self.prune_empty_instances();
        self.input_devices = new_devices;
    }

    /// Drops any join slots that lost all devices after a rescan so the UI
    /// never renders empty placeholders.
    fn prune_empty_instances(&mut self) {
        self.instances
            .retain(|instance| !instance.devices.is_empty());
    }

    /// Periodically rescans for controllers to surface new Bluetooth devices as
    /// soon as they connect.
    fn maybe_refresh_input_devices(&mut self) {
        if self.last_input_scan.elapsed() < std::time::Duration::from_secs(2) {
            return;
        }
        self.last_input_scan = std::time::Instant::now();
        self.sync_input_devices();
    }

    pub fn prepare_game_launch(&mut self) {
        set_instance_resolutions(&mut self.instances, &self.options);

        if let HandlerRef(_) = cur_game!(self) {
            // Remember the raw profile selections for this game before translating
            // guest placeholders so the next launch can restore the same layout.
            let game_id = cur_game!(self).persistent_id();
            let mut assignments: Vec<String> = Vec::new();
            for instance in &self.instances {
                let selection = self
                    .profiles
                    .get(instance.profselection)
                    .cloned()
                    .unwrap_or_else(|| "Guest".to_string());
                assignments.push(selection);
            }
            self.options
                .last_profile_assignments
                .insert(game_id, assignments);
        }

        set_instance_names(&mut self.instances, &self.profiles);

        let game = cur_game!(self).to_owned();
        let instances = self.instances.clone();
        let dev_infos: Vec<DeviceInfo> = self.input_devices.iter().map(|p| p.info()).collect();

        let cfg = self.options.clone();
        let _ = save_cfg(&cfg);

        self.cur_page = MenuPage::Home;
        self.spawn_task(
            "Launching...\n\nDon't press any buttons or move any analog sticks or mice.",
            move || {
                sleep(std::time::Duration::from_secs(2));
                if let Err(err) = launch_game(&game, &dev_infos, &instances, &cfg) {
                    println!("{}", err);
                    msg("Launch Error", &format!("{err}"));
                }
            },
        );
    }
}
