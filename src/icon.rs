use crate::error::{AppError, AppResult};

use eframe::egui;

use std::sync::Arc;

const APP_ICON_PNG: &[u8] = include_bytes!("../assets/app.png");

struct RgbaIcon {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

pub fn tray_icon() -> AppResult<tray_icon::Icon> {
    let icon = decode_icon()?;

    tray_icon::Icon::from_rgba(icon.pixels, icon.width, icon.height)
        .map_err(|error| AppError::Message(format!("Could not create the tray icon: {error}")))
}

pub fn window_icon() -> AppResult<Arc<egui::IconData>> {
    let icon = decode_icon()?;

    Ok(Arc::new(egui::IconData {
        rgba: icon.pixels,
        width: icon.width,
        height: icon.height,
    }))
}

fn decode_icon() -> AppResult<RgbaIcon> {
    let image = image::load_from_memory(APP_ICON_PNG)
        .map_err(|error| {
            AppError::Message(format!("Could not decode the application icon: {error}"))
        })?
        .into_rgba8();

    let width = image.width();

    let height = image.height();

    Ok(RgbaIcon {
        pixels: image.into_raw(),

        width,

        height,
    })
}
