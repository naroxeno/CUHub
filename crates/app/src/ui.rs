//! Zoom-aware pixel primitives and the app's hub-backed state.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use cuhub_core::hub::Hub;
use cuhub_core::mods::InstalledMod;
use cuhub_core::settings::Settings;
use nsh_core::element::FontAsset;
use nsh_core::prelude::*;

use crate::palette::*;

pub const PIXEL_FONT: FontAsset = asset!("fonts/fusion-pixel-12px-proportional-zh_hans.otf");

pub const PAGES: [&str; 6] = [
    "Nexus Dashboard",
    "Local Mods",
    "Multiplayer",
    "Skins & Backups",
    "Hub Home",
    "Settings",
];

#[derive(Clone)]
pub struct Ui {
    pub page: Signal<usize>,
    pub zoom: Signal<u8>,
    pub dev: Signal<bool>,
    pub taps: Signal<u32>,
    pub last_tap: Signal<Option<Instant>>,
    pub notice: Signal<String>,
    pub hub: Arc<Mutex<Hub>>,
    pub settings: Signal<Settings>,
    pub mods: Signal<Vec<InstalledMod>>,
    pub metadata_count: Signal<usize>,
    pub metadata_synced: Signal<String>,
    pub app_data: String,
}

impl Ui {
    pub fn new() -> Self {
        let mut hub = Hub::open();
        if hub.settings().game_path.trim().is_empty() {
            hub.detect_game_folder();
        }
        let settings = hub.settings().clone();
        let zoom = settings.ui_scale;
        let mods = hub.scan_mods().unwrap_or_default();
        let metadata_count = hub.metadata().len();
        let metadata_synced = hub
            .metadata_last_synced_label()
            .unwrap_or_else(|| String::from("Never"));
        let app_data = hub.app_data().display().to_string();
        let notice = if settings.game_path.trim().is_empty() {
            String::from("No game folder set. Open Settings to detect it.")
        } else {
            format!("{} mod(s) found in Plugins.", mods.len())
        };
        Self {
            page: signal(0usize),
            zoom: signal(zoom),
            dev: signal(false),
            taps: signal(0u32),
            last_tap: signal(None),
            notice: signal(notice),
            hub: Arc::new(Mutex::new(hub)),
            settings: signal(settings),
            mods: signal(mods),
            metadata_count: signal(metadata_count),
            metadata_synced: signal(metadata_synced),
            app_data,
        }
    }

    fn hub(&self) -> MutexGuard<'_, Hub> {
        self.hub
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn refresh_mods(&self) {
        let result = {
            let hub = self.hub();
            hub.scan_mods()
        };
        match result {
            Ok(mods) => {
                let count = mods.len();
                self.mods.set(mods);
                self.notice
                    .set(format!("Scanned {count} mod(s) in Plugins."));
            }
            Err(error) => self.notice.set(error.to_string()),
        }
    }

    pub fn launch_game(&self) {
        let result = {
            let hub = self.hub();
            hub.launch_game()
        };
        match result {
            Ok(app_id) => self.notice.set(format!(
                "Asked Steam to launch Casualties Unknown (app {app_id})."
            )),
            Err(error) => self.notice.set(format!("Could not launch: {error}")),
        }
    }

    pub fn detect_game(&self) {
        let found = {
            let mut hub = self.hub();
            hub.detect_game_folder()
        };
        match found {
            Some(path) => {
                let settings = {
                    let hub = self.hub();
                    hub.settings().clone()
                };
                self.settings.set(settings);
                self.notice
                    .set(format!("Game folder set to {}.", path.display()));
                self.refresh_mods();
            }
            None => self.notice.set(String::from(
                "Casualties Unknown was not found in the Steam libraries.",
            )),
        }
    }

    pub fn set_ui_scale(&self, level: u8) {
        let result = {
            let mut hub = self.hub();
            let mut settings = hub.settings().clone();
            if settings.ui_scale == level {
                Ok(settings)
            } else {
                settings.ui_scale = level;
                match hub.save_settings(&settings) {
                    Ok(()) => Ok(hub.settings().clone()),
                    Err(error) => Err(error.to_string()),
                }
            }
        };
        match result {
            Ok(settings) => {
                self.zoom.set(settings.ui_scale);
                self.settings.set(settings);
                self.notice.set(format!("Interface scale set to {level}x."));
            }
            Err(error) => self.notice.set(format!("Could not save settings: {error}")),
        }
    }
}

pub fn scaled(zoom: &Signal<u8>, base: f32) -> impl Fn() -> Points + Clone + use<> {
    let zoom = *zoom;
    move || px(base * zoom.get() as f32)
}

pub fn pix(s: impl IntoReactive<SharedString>, base: f32, color: u32, zoom: &Signal<u8>) -> Text {
    pix_c(s, base, rgb(color), zoom)
}

pub fn pix_c(
    s: impl IntoReactive<SharedString>,
    base: f32,
    color: impl IntoReactive<AdaptiveColor>,
    zoom: &Signal<u8>,
) -> Text {
    text(s)
        .text_size(scaled(zoom, base))
        .line_height(scaled(zoom, base * 4.0 / 3.0))
        .font_family(PIXEL_FONT.family)
        .text_color(color)
}

pub fn panel(zoom: &Signal<u8>, title: &'static str, body: impl IntoElement) -> View {
    let z = *zoom;
    view()
        .flex_col()
        .bg(rgb(SURFACE))
        .border_l(scaled(&z, 1.0))
        .border_r(scaled(&z, 1.0))
        .border_b(scaled(&z, 1.0))
        .border_color(rgb(BORDER))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_center()
                .h(scaled(&z, 1.0))
                .child(view().flex_1().h(scaled(&z, 1.0)).bg(rgb(BORDER)))
                .child(
                    view()
                        .px(scaled(&z, 8.0))
                        .bg(rgb(SURFACE))
                        .child(pix(title, 12.0, TEXT, &z)),
                )
                .child(view().flex_1().h(scaled(&z, 1.0)).bg(rgb(BORDER))),
        )
        .child(
            view()
                .flex_col()
                .flex_1()
                .pt(scaled(&z, 9.0))
                .px(scaled(&z, 2.0))
                .pb(scaled(&z, 2.0))
                .child(
                    view()
                        .flex_col()
                        .flex_1()
                        .p(scaled(&z, 8.0))
                        .border(scaled(&z, 1.0))
                        .border_color(rgb(BORDER))
                        .child(body),
                ),
        )
}

pub fn button(
    zoom: &Signal<u8>,
    label: &'static str,
    enabled: bool,
    on_press: impl Fn() + 'static,
) -> Text {
    let z = *zoom;
    let mut button = pix(label, 12.0, if enabled { TEXT } else { TEXT_DIM }, &z)
        .px(scaled(&z, 10.0))
        .py(scaled(&z, 4.0))
        .bg(rgb(SURFACE_RAISED))
        .border(scaled(&z, 1.0))
        .border_color(if enabled {
            rgb(BORDER)
        } else {
            rgb(BORDER_DIM)
        });
    if enabled {
        button = button
            .hover(|s| s.bg(rgb(TEXT)).text_color(rgb(BG)))
            .active(|s| s.bg(rgb(ACCENT_PRESS)).text_color(rgb(TEXT)))
            .on_press(on_press);
    }
    button
}
