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

use eframe::egui::{self, Key, StrokeKind};

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum MenuPage {
    Home,
    Settings,
    Profiles,
    Game,
    Instances,
}

pub struct PartyApp {
    pub needs_update: bool,
    pub options: PartyConfig,
    pub cur_page: MenuPage,
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
    /// Remembers how many columns the home grid used during the last frame so
    /// D-pad navigation can move predictably between rows.
    pub home_grid_columns: usize,
    /// Signals that the home grid should request focus for the selected tile so
    /// controller presses immediately trigger the highlighted entry.
    pub pending_home_focus: bool,
    /// Signals that the game list sidebar should scroll the selected entry into
    /// view to keep navigation fluid when using a controller.
    pub pending_game_list_focus: bool,
    /// Marks that the viewport still needs an initial focus pulse so Steam Deck
    /// controllers send events without the user clicking first.
    pub needs_viewport_focus: bool,
    /// Flags that the controller currently highlights the top navigation bar so
    /// horizontal D-pad input can hop between the main menu buttons.
    pub nav_in_focus: bool,
    /// Requests a focus pulse for the active navigation button so pressing
    /// Cross/Enter immediately triggers it after moving focus with the D-pad.
    pub pending_nav_focus: bool,
    /// Tracks which navigation entry is currently highlighted so changing focus
    /// with the controller no longer flips pages until the user confirms.
    pub nav_selection: MenuPage,
    /// Defers a focus pulse for the first interactive control on newly opened
    /// pages so controller navigation immediately highlights actionable widgets.
    pub pending_content_focus: bool,
    /// Requests a scroll adjustment after focus changes so the highlighted
    /// element remains visible when navigating large forms with the D-pad.
    pub pending_scroll_to_focus: bool,
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
    /// Builds the full Split Happens UI with a specific repaint interval so the
    /// main application can align frame pacing with the detected display.
    pub fn with_repaint_interval(repaint_interval: std::time::Duration) -> Self {
        let options = load_cfg();
        let input_devices = scan_input_devices(&options.pad_filter_type);
        Self {
            needs_update: check_for_split_happens_update(),
            options,
            cur_page: MenuPage::Home,
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
            home_grid_columns: 1,
            pending_home_focus: true,
            pending_game_list_focus: false,
            needs_viewport_focus: true,
            nav_in_focus: false,
            pending_nav_focus: false,
            nav_selection: MenuPage::Home,
            pending_content_focus: false,
            pending_scroll_to_focus: false,
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

        if self.needs_viewport_focus {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.needs_viewport_focus = false;
        }

        egui::TopBottomPanel::top("menu_nav_panel").show(ctx, |ui| {
            if self.task.is_some() {
                ui.disable();
            }
            self.display_panel_top(ui);
        });

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
    /// Highlights the active widget and manages focus/scroll bookkeeping so
    /// controller navigation remains visible across scrollable layouts.
    pub fn decorate_focus(&mut self, ui: &mut egui::Ui, response: &egui::Response) {
        if !response.enabled() {
            return;
        }

        if self.pending_content_focus {
            response.request_focus();
            response.scroll_to_me(Some(egui::Align::Center));
            self.pending_content_focus = false;
            self.pending_scroll_to_focus = false;
        } else if self.pending_scroll_to_focus && response.has_focus() {
            response.scroll_to_me(Some(egui::Align::Center));
            self.pending_scroll_to_focus = false;
        }

        if response.has_focus() {
            let visuals = ui.visuals();
            let stroke = egui::Stroke::new(2.0, visuals.selection.bg_fill);
            ui.painter()
                // Draw the focus ring just outside the widget so active elements
                // gain a subtle glow without shrinking their layout footprint.
                .rect_stroke(response.rect.expand(4.0), 8.0, stroke, StrokeKind::Outside);
        }
    }

    /// Cycles between the Home, Settings, and Profiles buttons in the header so
    /// the controller can open different sections without touching a mouse.
    fn cycle_nav_focus(&mut self, horizontal: i32) {
        if horizontal == 0 {
            return;
        }

        let nav_order = [MenuPage::Home, MenuPage::Settings, MenuPage::Profiles];
        let source = if self.nav_in_focus {
            self.nav_selection
        } else {
            self.cur_page
        };
        let current_index = nav_order
            .iter()
            .position(|page| *page == source)
            .unwrap_or(0) as i32;
        let next_index = (current_index + horizontal).clamp(0, nav_order.len() as i32 - 1);
        let target = nav_order[next_index as usize];

        self.nav_selection = target;
        self.pending_nav_focus = true;
    }

    /// Applies the currently highlighted navigation selection and prepares the
    /// destination page so controller focus begins at the first actionable
    /// element instead of auto-activating headers.
    fn activate_nav_selection(&mut self) {
        let target = self.nav_selection;
        match target {
            MenuPage::Home => {
                self.cur_page = MenuPage::Home;
                self.pending_home_focus = true;
                self.pending_content_focus = false;
                self.pending_scroll_to_focus = false;
            }
            MenuPage::Settings => {
                self.cur_page = MenuPage::Settings;
                self.pending_content_focus = true;
                self.pending_scroll_to_focus = true;
            }
            MenuPage::Profiles => {
                self.profiles = scan_profiles(false);
                self.cur_page = MenuPage::Profiles;
                self.pending_content_focus = true;
                self.pending_scroll_to_focus = true;
            }
            MenuPage::Game | MenuPage::Instances => {
                self.cur_page = target;
                self.pending_content_focus = true;
                self.pending_scroll_to_focus = true;
            }
        }

        self.nav_selection = self.cur_page;
        self.nav_in_focus = false;
        self.pending_nav_focus = false;
    }

    pub fn spawn_task<F>(&mut self, msg: &str, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.loading_msg = Some(msg.to_string());
        self.loading_since = Some(std::time::Instant::now());
        self.task = Some(std::thread::spawn(f));
    }

    fn handle_gamepad_gui(&mut self, raw_input: &mut egui::RawInput) {
        let mut keypress: Option<egui::Key> = None;
        let mut trigger_instances = false;
        let mut open_selected_from_home = false;
        let mut horizontal = 0i32;
        let mut vertical = 0i32;
        // Defer activating the navigation selection until the input iteration
        // finishes so the borrow checker can release the mutable slice borrow
        // from `self.input_devices` before we mutate other fields.
        let mut activate_nav_after_poll = false;

        for pad_index in 0..self.input_devices.len() {
            if !self.input_devices[pad_index].enabled() {
                continue;
            }

            let event = self.input_devices[pad_index].poll();
            match event {
                Some(PadButton::ABtn) => {
                    if self.nav_in_focus {
                        activate_nav_after_poll = true;
                    } else {
                        match self.cur_page {
                            MenuPage::Home => open_selected_from_home = true,
                            _ => keypress = Some(Key::Enter),
                        }
                    }
                }
                Some(PadButton::BBtn) => {
                    self.cur_page = MenuPage::Home;
                    self.nav_selection = MenuPage::Home;
                    self.pending_home_focus = true;
                    self.nav_in_focus = false;
                    self.pending_nav_focus = false;
                    self.pending_content_focus = false;
                    self.pending_scroll_to_focus = false;
                }
                Some(PadButton::XBtn) => {
                    self.profiles = scan_profiles(false);
                    self.cur_page = MenuPage::Profiles;
                    self.nav_selection = MenuPage::Profiles;
                    self.nav_in_focus = false;
                    self.pending_nav_focus = false;
                    self.pending_content_focus = true;
                    self.pending_scroll_to_focus = true;
                }
                Some(PadButton::YBtn) => {
                    self.cur_page = MenuPage::Settings;
                    self.nav_selection = MenuPage::Settings;
                    self.nav_in_focus = false;
                    self.pending_nav_focus = false;
                    self.pending_content_focus = true;
                    self.pending_scroll_to_focus = true;
                }
                Some(PadButton::SelectBtn) => keypress = Some(Key::Tab),
                Some(PadButton::StartBtn) => {
                    if self.cur_page == MenuPage::Game {
                        trigger_instances = true;
                    }
                }
                Some(PadButton::Up) => vertical -= 1,
                Some(PadButton::Down) => vertical += 1,
                Some(PadButton::Left) => horizontal -= 1,
                Some(PadButton::Right) => horizontal += 1,
                Some(_) => {}
                None => {}
            }
        }

        if activate_nav_after_poll {
            self.activate_nav_selection();
        }

        let mut tab_forward = 0i32;
        let mut tab_backward = 0i32;

        if self.cur_page == MenuPage::Home {
            if self.nav_in_focus {
                if horizontal != 0 {
                    // Rotate through the primary navigation buttons while the
                    // controller highlight sits on the header.
                    self.cycle_nav_focus(horizontal);
                }

                if vertical > 0 {
                    // Drop focus back to the tile grid when the user presses
                    // down from the navigation bar.
                    self.nav_in_focus = false;
                    self.pending_nav_focus = false;
                    self.nav_selection = self.cur_page;
                    self.pending_home_focus = true;
                }
            } else {
                let mut routed_to_nav = false;

                if vertical < 0 && self.selected_game < self.home_grid_columns {
                    // Jump into the navigation bar when pressing up from the
                    // top-most row of tiles.
                    self.nav_in_focus = true;
                    self.pending_nav_focus = true;
                    self.nav_selection = self.cur_page;
                    routed_to_nav = true;
                }

                if !routed_to_nav && (horizontal != 0 || vertical != 0) {
                    // Translate D-pad movement into focused selection changes
                    // inside the home grid.
                    self.navigate_home_grid(horizontal, vertical);
                }
            }
        } else {
            // Clear any lingering header focus when switching away from the
            // home grid so other pages can own the navigation flow.
            self.nav_in_focus = false;
            self.pending_nav_focus = false;
            self.nav_selection = self.cur_page;

            if vertical > 0 {
                // Step forward through interactive widgets with Tab so
                // controller users can reach every toggle and combo box.
                tab_forward += vertical;
            } else if vertical < 0 {
                // Walk backwards across the form with Shift+Tab when pressing
                // up on the D-pad.
                tab_backward += -vertical;
            }

            if vertical != 0 {
                // Ensure the form scrolls along with the newly focused widget so
                // controller navigation never leaves the highlight off-screen.
                self.pending_scroll_to_focus = true;
            }
        }

        if open_selected_from_home {
            self.open_instances_for(self.selected_game);
        }

        if trigger_instances {
            self.open_instances_for(self.selected_game);
        }

        for _ in 0..tab_forward {
            raw_input.events.push(egui::Event::Key {
                key: Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            });
        }

        for _ in 0..tab_backward {
            let mut modifiers = egui::Modifiers::default();
            modifiers.shift = true;
            raw_input.events.push(egui::Event::Key {
                key: Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            });
        }

        if let Some(key) = keypress {
            raw_input.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            });
        }
    }

    /// Updates the selected game index based on D-pad input and flags the
    /// corresponding UI region to request focus.
    #[allow(dead_code)]
    fn navigate_selection(&mut self, horizontal: i32, vertical: i32) {
        if self.games.is_empty() {
            return;
        }

        match self.cur_page {
            MenuPage::Home => self.navigate_home_grid(horizontal, vertical),
            _ => self.navigate_game_list(vertical),
        }
    }

    /// Handles horizontal and vertical travel within the home screen grid so
    /// controller navigation mirrors tile-based consoles.
    fn navigate_home_grid(&mut self, horizontal: i32, vertical: i32) {
        let columns = self.home_grid_columns.max(1);
        let total_rows = (self.games.len() + columns - 1) / columns;
        if total_rows == 0 {
            return;
        }

        let mut row = self.selected_game / columns;
        let mut col = self.selected_game % columns;

        if vertical != 0 {
            let mut new_row = row as i32 + vertical;
            new_row = new_row.clamp(0, (total_rows.saturating_sub(1)) as i32);
            row = new_row as usize;
            let row_start = row * columns;
            let row_len = (self.games.len().saturating_sub(row_start)).min(columns);
            if row_len > 0 {
                col = col.min(row_len - 1);
            }
        }

        if horizontal != 0 {
            let row_start = row * columns;
            let row_len = (self.games.len().saturating_sub(row_start)).min(columns);
            if row_len > 0 {
                let mut new_col = col as i32 + horizontal;
                new_col = new_col.clamp(0, (row_len.saturating_sub(1)) as i32);
                col = new_col as usize;
            }
        }

        let new_index = row * columns + col;
        if new_index < self.games.len() && new_index != self.selected_game {
            self.selected_game = new_index;
            self.pending_home_focus = true;
        }
    }

    /// Steps through the vertical game list while keeping the selection within
    /// bounds so the sidebar scrolls naturally with controller input.
    #[allow(dead_code)]
    fn navigate_game_list(&mut self, vertical: i32) {
        if vertical == 0 {
            return;
        }

        let len = self.games.len();
        if len == 0 {
            return;
        }

        let current = self.selected_game as i32;
        let max_index = len.saturating_sub(1) as i32;
        let mut next = current + vertical;
        next = next.clamp(0, max_index);

        if next != current {
            self.selected_game = next as usize;
            self.pending_game_list_focus = true;
        }
    }

    /// Synchronizes in-memory profile assignments when the user renames a
    /// profile so running sessions keep referencing the updated identifier.
    pub fn apply_local_profile_rename(&mut self, old_name: &str, new_name: &str) {
        for assignments in self.options.last_profile_assignments.values_mut() {
            for slot in assignments.iter_mut() {
                if slot == old_name {
                    *slot = new_name.to_string();
                }
            }
        }

        for instance in &mut self.instances {
            if instance.profname == old_name {
                instance.profname = new_name.to_string();
            }
        }
    }

    /// Refreshes the cached Proton installation list so users can discover new
    /// compatibility tools without restarting Split Happens.
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

        let dir_tmp = PATH_APP.join("tmp");
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
        self.pending_game_list_focus = true;
        self.cur_page = MenuPage::Instances;
        self.nav_selection = MenuPage::Home;
        self.nav_in_focus = false;
        self.pending_nav_focus = false;
        self.pending_content_focus = true;
        self.pending_scroll_to_focus = true;
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
                        self.nav_selection = MenuPage::Home;
                        self.pending_content_focus = true;
                        self.pending_scroll_to_focus = true;
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
        self.nav_selection = MenuPage::Home;
        self.pending_home_focus = true;
        self.nav_in_focus = false;
        self.pending_nav_focus = false;
        self.pending_content_focus = false;
        self.pending_scroll_to_focus = false;
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
