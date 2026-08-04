use crate::{
    config::ConfigManager,
    error::{AppError, AppResult},
    model::AppConfig,
};

use eframe::egui;
use std::process::Command;

pub fn launch() -> AppResult<()> {
    let executable = std::env::current_exe()?;

    Command::new(executable).arg("--settings").spawn()?;

    Ok(())
}

pub fn run() -> AppResult<()> {
    let config = ConfigManager::load()?;

    eframe::run_native(
        "Nightshade MP3 Settings",
        eframe::NativeOptions::default(),
        Box::new(move |_creation_context| Ok(Box::new(SettingsApp::new(config)))),
    )
    .map_err(|error| AppError::Message(error.to_string()))
}

struct SettingsApp {
    config: AppConfig,
    status: String,
}

impl SettingsApp {
    fn new(config: AppConfig) -> Self {
        Self {
            config,
            status: String::new(),
        }
    }

    fn save(&mut self) -> bool {
        if let Err(message) = validate_config(&self.config) {
            self.status = message;
            return false;
        }

        match ConfigManager::save(&self.config) {
            Ok(()) => {
                self.status = "Configuration saved.".to_owned();
                true
            }

            Err(error) => {
                self.status = format!("Failed to save configuration: {error}");
                false
            }
        }
    }
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Nightshade MP3");

            ui.add_space(8.0);
            ui.separator();

            ui.heading("OSC");

            ui.label("Host");

            ui.text_edit_singleline(&mut self.config.osc.host);

            ui.label("Port");

            ui.add(egui::DragValue::new(&mut self.config.osc.port).range(1..=u16::MAX));

            ui.checkbox(
                &mut self.config.osc.auto_discover,
                "Automatically discover VRChat with OSCQuery",
            );

            ui.add_space(8.0);
            ui.separator();

            ui.heading("OSCQuery");

            ui.checkbox(&mut self.config.oscquery.enabled, "Enable OSCQuery");

            ui.add_space(8.0);
            ui.separator();

            ui.heading("Avatar Parameters");

            ui.label("Pointer Parameter");

            ui.text_edit_singleline(&mut self.config.parameters.pointer);

            ui.label("Character Parameter");

            ui.text_edit_singleline(&mut self.config.parameters.character);

            ui.add_space(8.0);
            ui.separator();

            ui.heading("Text");

            ui.horizontal(|ui| {
                ui.label("Characters per line");

                ui.label(self.config.text.width.to_string());
            });

            ui.label("Title and artist separator");

            ui.text_edit_singleline(&mut self.config.text.separator);

            ui.label("Update interval in milliseconds");

            ui.add(
                egui::DragValue::new(&mut self.config.text.update_interval_ms).range(50..=60_000),
            );

            ui.label("Scroll interval in milliseconds");

            ui.add(
                egui::DragValue::new(&mut self.config.text.scroll_interval_ms).range(100..=60_000),
            );

            ui.label("Full refresh interval in seconds");

            ui.add(
                egui::DragValue::new(&mut self.config.text.full_refresh_seconds).range(5..=3_600),
            );

            ui.add_space(12.0);
            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    self.save();
                }

                if ui.button("Save and Close").clicked() && self.save() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }

                if ui.button("Restore Defaults").clicked() {
                    self.config = AppConfig::default();
                    self.status = "Default values restored but not saved.".to_owned();
                }
            });

            if !self.status.is_empty() {
                ui.add_space(8.0);
                ui.label(self.status.as_str());
            }
        });
    }
}

fn validate_config(config: &AppConfig) -> Result<(), String> {
    if config.osc.host.trim().is_empty() {
        return Err("OSC host cannot be empty.".to_owned());
    }

    if config.osc.port == 0 {
        return Err("OSC port cannot be zero.".to_owned());
    }

    if config.oscquery.enabled && config.oscquery.host.trim().is_empty() {
        return Err("OSCQuery host cannot be empty.".to_owned());
    }

    if config.oscquery.enabled && config.oscquery.port == 0 {
        return Err("OSCQuery port cannot be zero.".to_owned());
    }

    if config.parameters.pointer.trim().is_empty() {
        return Err("Pointer parameter cannot be empty.".to_owned());
    }

    if config.parameters.character.trim().is_empty() {
        return Err("Character parameter cannot be empty.".to_owned());
    }

    if config.text.separator.chars().count() > 8 {
        return Err("The separator cannot contain more than eight characters.".to_owned());
    }

    Ok(())
}
