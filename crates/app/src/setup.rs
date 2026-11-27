//! First-run setup wizard: six steps over hub settings, mounted over the shell.

use crate::app_state::Ui;
use crate::components::{button, fade_in, panel, pix, pix_c, rise_in, scaled, text_input};
use crate::i18n;
use crate::palette::{
    ACCENT, BG, BORDER, DANGER, TEXT, TEXT_DIM, TEXT_MUTED, WARN, theme_color, theme_color_with,
};
use cuprum_core::settings::Settings;
use nsh_core::file_dialog::OpenDialog;
use nsh_core::prelude::*;

const LAST_STEP: usize = 5;
const STEPS: usize = LAST_STEP + 1;

#[derive(Clone)]
pub struct Setup {
    pub open: Signal<bool>,
    pub step: Signal<usize>,
    pub game_draft: Signal<String>,
    pub download_draft: Signal<String>,
    pub store_draft: Signal<String>,
    pub store_error: Signal<String>,
    pub error: Signal<String>,
}

impl Setup {
    pub fn new(settings: &Settings, open: bool) -> Self {
        Self {
            open: signal(open),
            step: signal(0usize),
            game_draft: signal(settings.game_path.clone()),
            download_draft: signal(settings.download_path.clone()),
            store_draft: signal(settings.store_path.clone()),
            store_error: signal(String::new()),
            error: signal(String::new()),
        }
    }
}

pub fn open(ui: &Ui) {
    let settings = ui.settings.get();
    ui.setup.game_draft.set(settings.game_path);
    ui.setup.download_draft.set(settings.download_path);
    ui.setup.store_draft.set(settings.store_path);
    ui.setup.store_error.set(String::new());
    ui.setup.error.set(String::new());
    ui.setup.step.set(0);
    ui.setup.open.set(true);
}

pub fn wizard(ui: &Ui) -> impl IntoElement {
    let ui = ui.clone();
    let open = ui.setup.open;
    when(
        move || open.get(),
        move || {
            let dismiss = ui.clone();
            on_key("escape", move |_| skip(&dismiss));
            let ui = ui.clone();
            let z = ui.zoom;
            let step = ui.setup.step;
            let error = ui.setup.error;
            let veil = fade_in();
            let card = rise_in();
            let body = view()
                .flex_col()
                .gap(scaled(&z, 10.0))
                .child(pix(
                    move || t!("wizard.step_label", step = step.get() + 1, total = STEPS),
                    11.0,
                    TEXT_MUTED,
                    &z,
                ))
                .child(dynamic({
                    let ui = ui.clone();
                    move || {
                        let fade = step_fade();
                        view()
                            .flex_col()
                            .w_full()
                            .bind_motion(&fade)
                            .child(step_content(&ui))
                    }
                }))
                .child(when(
                    move || !error.get().is_empty(),
                    move || pix(move || error.get(), 11.0, DANGER, &z),
                ))
                .child(footer(&ui));
            view()
                .absolute()
                .inset(px(0.0))
                .w_full()
                .h_full()
                .bg(theme_color(BG))
                .items_center()
                .justify_center()
                .bind_motion(&veil)
                .child(
                    panel(&z, move || t!("wizard.title"), body)
                        .w(scaled(&z, 560.0))
                        .max_w(percentage(0.9))
                        .bind_motion(&card),
                )
        },
    )
}

fn step_fade() -> Motion {
    motion().entering(
        MotionPreset::FadeIn
            .spec()
            .timing(|_| KeyframeTiming::new(200.0)),
    )
}

fn step_content(ui: &Ui) -> View {
    match ui.setup.step.get() {
        0 => welcome(ui),
        1 => game(ui),
        2 => bepinex(ui),
        3 => downloads(ui),
        4 => appearance(ui),
        _ => done(ui),
    }
}

fn welcome(ui: &Ui) -> View {
    let z = ui.zoom;
    view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .child(pix(move || t!("wizard.welcome.title"), 14.0, ACCENT, &z))
        .child(pix(move || t!("wizard.welcome.body"), 12.0, TEXT, &z))
}

fn game(ui: &Ui) -> View {
    let z = ui.zoom;
    let draft = ui.setup.game_draft;
    let settings = ui.settings;
    let browse_ui = ui.clone();
    let detect_ui = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .child(pix(move || t!("wizard.game.title"), 14.0, ACCENT, &z))
        .child(pix(move || t!("wizard.game.hint"), 11.0, TEXT_MUTED, &z))
        .child(folder_input(&z, draft, move || {
            t!("wizard.game.placeholder")
        }))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 8.0))
                .child(button(
                    &z,
                    move || t!("wizard.game.browse"),
                    true,
                    move || browse(&browse_ui),
                ))
                .child(button(
                    &z,
                    move || t!("wizard.game.detect"),
                    true,
                    move || detect(&detect_ui),
                )),
        )
        .child(pix(
            move || {
                let path = settings.get().game_path;
                if path.trim().is_empty() {
                    t!("wizard.game.not_set")
                } else {
                    t!("wizard.game.detected", path = path)
                }
            },
            11.0,
            TEXT_DIM,
            &z,
        ))
}

fn bepinex(ui: &Ui) -> View {
    let z = ui.zoom;
    let label_ui = ui.clone();
    let tone_ui = ui.clone();
    let version_ui = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .child(pix(move || t!("wizard.bepinex.title"), 14.0, ACCENT, &z))
        .child(pix_c(
            move || {
                let Some(status) = label_ui.bepinex.get() else {
                    return t!("wizard.bepinex.unavailable");
                };
                if status.il2cpp {
                    return t!("wizard.bepinex.il2cpp");
                }
                if status.complete {
                    return t!("wizard.bepinex.installed");
                }
                if status.installed {
                    return t!("wizard.bepinex.incomplete");
                }
                t!("wizard.bepinex.not_installed")
            },
            12.0,
            theme_color_with(move || {
                let Some(status) = tone_ui.bepinex.get() else {
                    return TEXT_DIM;
                };
                if status.il2cpp {
                    DANGER
                } else if status.complete {
                    ACCENT
                } else if status.installed {
                    WARN
                } else {
                    TEXT_MUTED
                }
            }),
            &z,
        ))
        .child(pix(
            move || {
                let Some(status) = version_ui.bepinex.get() else {
                    return String::new();
                };
                match (&status.version, status.major) {
                    (Some(version), _) => t!("wizard.bepinex.version", version = version),
                    (None, Some(major)) => t!("wizard.bepinex.major", major = major),
                    _ => String::new(),
                }
            },
            11.0,
            TEXT_DIM,
            &z,
        ))
        .child(pix(move || t!("wizard.bepinex.note"), 11.0, TEXT_MUTED, &z))
}

fn downloads(ui: &Ui) -> View {
    let z = ui.zoom;
    let draft = ui.setup.download_draft;
    let store = ui.setup.store_draft;
    let store_error = ui.setup.store_error;
    let store_default = {
        let ui = ui.clone();
        move || ui.store_folder_path()
    };
    let browse_ui = ui.clone();
    let reset_ui = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .child(pix(move || t!("wizard.downloads.title"), 14.0, ACCENT, &z))
        .child(pix(
            move || t!("wizard.downloads.hint"),
            11.0,
            TEXT_MUTED,
            &z,
        ))
        .child(folder_input(&z, draft, move || {
            t!("wizard.downloads.placeholder")
        }))
        .child(pix(
            move || t!("settings.store_folder"),
            12.0,
            TEXT_MUTED,
            &z,
        ))
        .child(folder_input(&z, store, store_default))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(scaled(&z, 8.0))
                .child(button(
                    &z,
                    move || t!("settings.store_browse"),
                    true,
                    move || browse_store(&browse_ui),
                ))
                .child(button(
                    &z,
                    move || t!("settings.store_reset"),
                    true,
                    move || reset_store(&reset_ui),
                )),
        )
        .child(pix(move || t!("settings.store_hint"), 11.0, TEXT_DIM, &z))
        .child(when(
            move || !store_error.get().is_empty(),
            move || pix(move || store_error.get(), 11.0, DANGER, &z),
        ))
}

fn appearance(ui: &Ui) -> View {
    let z = ui.zoom;
    let mut scales = view().flex_row().items_center().gap(scaled(&z, 8.0));
    for level in 1..=3 {
        scales = scales.child(crate::pages::zoom_button(ui, level));
    }
    let mut languages = view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(scaled(&z, 8.0));
    for index in 0..i18n::LANGUAGES.len() {
        languages = languages.child(crate::pages::language_button(ui, index, &z));
    }
    view()
        .flex_col()
        .gap(scaled(&z, 10.0))
        .child(pix(move || t!("wizard.appearance.title"), 14.0, ACCENT, &z))
        .child(pix(
            move || t!("settings.interface_scale"),
            12.0,
            TEXT_MUTED,
            &z,
        ))
        .child(scales)
        .child(pix(move || t!("settings.language"), 12.0, TEXT_MUTED, &z))
        .child(languages)
}

fn done(ui: &Ui) -> View {
    let z = ui.zoom;
    let settings = ui.settings;
    let game = move || {
        let path = settings.get().game_path;
        if path.trim().is_empty() {
            t!("wizard.game.not_set")
        } else {
            path
        }
    };
    let downloads = move || {
        let path = settings.get().download_path;
        if path.trim().is_empty() {
            t!("wizard.downloads.placeholder")
        } else {
            path
        }
    };
    let scale = move || t!("wizard.done.scale", scale = settings.get().ui_scale);
    let language = move || i18n::language_label(&settings.get().language);
    view()
        .flex_col()
        .gap(scaled(&z, 6.0))
        .child(pix(move || t!("wizard.done.title"), 14.0, ACCENT, &z))
        .child(pix(move || t!("wizard.done.body"), 12.0, TEXT, &z))
        .child(summary_row(&z, move || t!("wizard.done.game"), game))
        .child(summary_row(
            &z,
            move || t!("wizard.done.downloads"),
            downloads,
        ))
        .child(summary_row(&z, move || t!("wizard.done.scale"), scale))
        .child(summary_row(
            &z,
            move || t!("wizard.done.language"),
            language,
        ))
}

fn summary_row(
    z: &Signal<u8>,
    label: impl IntoReactive<SharedString>,
    value: impl IntoReactive<SharedString>,
) -> View {
    let z = *z;
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .h(scaled(&z, 24.0))
        .px(scaled(&z, 6.0))
        .border_b(scaled(&z, 1.0))
        .border_b_color(theme_color(BORDER))
        .gap(scaled(&z, 8.0))
        .child(pix(label, 11.0, TEXT_MUTED, &z))
        .child(pix(value, 11.0, TEXT, &z))
}

fn folder_input(
    z: &Signal<u8>,
    value: Signal<String>,
    placeholder: impl IntoReactive<String>,
) -> impl IntoElement {
    let z = *z;
    text_input(&z)
        .value(value)
        .placeholder(placeholder)
        .h(scaled(&z, 26.0))
        .w_full()
        .on_change(move |text: &str| value.set(text.to_owned()))
}

fn footer(ui: &Ui) -> View {
    let z = ui.zoom;
    let step = ui.setup.step;
    let back_ui = ui.clone();
    let skip_ui = ui.clone();
    let finish_ui = ui.clone();
    let next_ui = ui.clone();
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .w_full()
        .child(when(
            move || step.get() > 0,
            move || {
                let ui = back_ui.clone();
                button(&z, move || t!("wizard.back"), true, move || back(&ui))
            },
        ))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 8.0))
                .child(when(
                    move || step.get() < LAST_STEP,
                    move || {
                        let ui = skip_ui.clone();
                        button(&z, move || t!("wizard.skip"), true, move || skip(&ui))
                    },
                ))
                .child(
                    when(
                        move || step.get() == LAST_STEP,
                        move || {
                            let ui = finish_ui.clone();
                            button(&z, move || t!("wizard.finish"), true, move || finish(&ui))
                        },
                    )
                    .otherwise(move || {
                        let ui = next_ui.clone();
                        button(&z, move || t!("wizard.next"), true, move || next(&ui))
                    }),
                ),
        )
}

fn back(ui: &Ui) {
    let step = ui.setup.step.get();
    ui.setup.step.set(step.saturating_sub(1));
    ui.setup.error.set(String::new());
}

fn next(ui: &Ui) {
    let step = ui.setup.step.get();
    let saved = match step {
        1 => ui.save_setup_game(&ui.setup.game_draft.get()),
        3 => {
            if let Err(error) = ui.save_store(&ui.setup.store_draft.get()) {
                ui.setup.store_error.set(error);
                return;
            }
            ui.save_setup_download(&ui.setup.download_draft.get())
        }
        _ => Ok(()),
    };
    match saved {
        Ok(()) => {
            if step == 1 {
                ui.setup.game_draft.set(ui.settings.get().game_path.clone());
            }
            if step == 3 {
                ui.setup
                    .store_draft
                    .set(ui.settings.get().store_path.clone());
            }
            ui.setup.error.set(String::new());
            ui.setup.store_error.set(String::new());
            ui.setup.step.set((step + 1).min(LAST_STEP));
        }
        Err(error) => ui.setup.error.set(error),
    }
}

fn skip(ui: &Ui) {
    if let Err(error) = ui.mark_setup_completed() {
        ui.setup.error.set(error);
        return;
    }
    ui.setup.open.set(false);
    ui.setup.step.set(0);
    ui.setup.error.set(String::new());
    ui.notice.set(t!("notice.setup_finished"));
}

fn finish(ui: &Ui) {
    skip(ui);
}

fn browse(ui: &Ui) {
    let ui = ui.clone();
    let picked = pick_paths(OpenDialog::folders());
    ui.spawn_worker(async move {
        match picked.await {
            Ok(Some(paths)) => {
                if let Some(path) = paths.into_iter().next() {
                    ui.setup.game_draft.set(path.display().to_string());
                    ui.setup.error.set(String::new());
                }
            }
            Ok(None) => {}
            Err(error) => ui
                .setup
                .error
                .set(t!("wizard.game.dialog_unavailable", error = error)),
        }
    });
}

fn browse_store(ui: &Ui) {
    let ui = ui.clone();
    let picked = pick_paths(OpenDialog::folders());
    ui.spawn_worker(async move {
        match picked.await {
            Ok(Some(paths)) => {
                if let Some(path) = paths.into_iter().next() {
                    ui.setup.store_draft.set(path.display().to_string());
                    ui.setup.store_error.set(String::new());
                }
            }
            Ok(None) => {}
            Err(error) => ui
                .setup
                .store_error
                .set(t!("settings.store_picker_failed", error = error)),
        }
    });
}

fn reset_store(ui: &Ui) {
    ui.setup.store_draft.set(String::new());
    ui.setup.store_error.set(String::new());
}

fn detect(ui: &Ui) {
    match ui.detect_setup_game() {
        Some(path) => {
            ui.setup.game_draft.set(path);
            ui.setup.error.set(String::new());
        }
        None => ui.setup.error.set(t!("wizard.game.detect_failed")),
    }
}
