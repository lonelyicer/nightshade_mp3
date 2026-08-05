use crate::{
    config::ConfigManager,
    error::{AppError, AppResult},
    model::AppConfig,
};

use eframe::egui;

use std::process::Command;

pub fn launch() -> AppResult<()> {
    Command::new(std::env::current_exe()?)
        .arg("--settings")
        .spawn()?;

    Ok(())
}

pub fn run() -> AppResult<()> {
    let config = ConfigManager::load()?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([300.0, 550.0])
            .with_min_inner_size([300.0, 550.0]),

        centered: true,

        renderer: eframe::Renderer::Glow,

        ..Default::default()
    };

    eframe::run_native(
        "Nightshade MP3 Settings",
        options,
        Box::new(move |_context| Ok(Box::new(SettingsApp::new(config)))),
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
        if let Err(error) = validate(&self.config) {
            self.status = error;
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

            ui.separator();

            ui.heading("OSC");

            ui.label("Host");

            ui.text_edit_singleline(&mut self.config.osc.host);

            ui.label("Port");

            ui.add(egui::DragValue::new(&mut self.config.osc.port).range(1..=u16::MAX));

            ui.checkbox(
                &mut self.config.osc.auto_discover,
                "Automatically discover VRChat",
            );

            ui.checkbox(&mut self.config.oscquery.enabled, "Enable OSCQuery");

            ui.separator();

            ui.heading("Avatar Parameters");

            ui.label("Pointer Parameter");

            ui.text_edit_singleline(&mut self.config.parameters.pointer);

            ui.label("Character Parameter");

            ui.text_edit_singleline(&mut self.config.parameters.character);

            ui.separator();

            ui.heading("Text");

            ui.label("Title and artist separator");

            ui.text_edit_singleline(&mut self.config.text.separator);

            ui.label("OSC write step in milliseconds");

            ui.add(egui::DragValue::new(&mut self.config.text.write_step_ms).range(20..=1_000));

            ui.label("Title scroll interval in milliseconds");

            ui.add(
                egui::DragValue::new(&mut self.config.text.scroll_interval_ms).range(100..=60_000),
            );

            ui.label("Title gap");

            ui.add(egui::DragValue::new(&mut self.config.text.title_gap).range(1..=32));

            ui.add_space(12.0);

            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    self.save();
                }

                if ui.button("Save and Close").clicked() && self.save() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }

                if ui.button("Restore Defaults").clicked() {
                    self.config = AppConfig::default();

                    self.status = "Defaults restored but not saved.".to_owned();
                }
            });

            if !self.status.is_empty() {
                ui.add_space(8.0);

                ui.label(&self.status);
            }
        });
    }
}

fn validate(config: &AppConfig) -> Result<(), String> {
    if config.osc.host.trim().is_empty() {
        return Err("OSC host cannot be empty.".to_owned());
    }

    if config.osc.port == 0 {
        return Err("OSC port cannot be zero.".to_owned());
    }

    if config.parameters.pointer.trim().is_empty() {
        return Err("Pointer parameter cannot be empty.".to_owned());
    }

    if config.parameters.character.trim().is_empty() {
        return Err("Character parameter cannot be empty.".to_owned());
    }

    if config.text.write_step_ms < 20 {
        return Err("OSC write step must be at least 20 milliseconds.".to_owned());
    }

    if config.text.scroll_interval_ms < 100 {
        return Err("Title scroll interval must be at least 100 milliseconds.".to_owned());
    }

    if config.text.title_gap == 0 {
        return Err("Title gap must be at least one character.".to_owned());
    }

    Ok(())
}
