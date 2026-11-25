use nsh_core::file_dialog::OpenDialog;
use nsh_core::prelude::*;

use super::{actions, path_row, section, settings_input};
use crate::app_state::Ui;
use crate::components::{button, panel, pix, scaled};
use crate::palette::*;

pub(super) fn storage(ui: &Ui) -> View {
    let z = ui.zoom;
    let app_data = ui.app_data.clone();
    let current = ui.clone();
    let placeholder = ui.clone();
    let browse = ui.clone();
    let reset = ui.clone();
    let save = ui.clone();
    let draft = ui.store_draft;
    let error = ui.store_error;
    view()
        .flex_col()
        .gap(scaled(&z, 16.0))
        .child(panel(
            &z,
            move || t!("settings.store_folder"),
            section(&z)
                .child(pix(
                    move || t!("settings.current_store"),
                    11.0,
                    TEXT_MUTED,
                    &z,
                ))
                .child(path_row(&z, move || current.store_folder_path()))
                .child(
                    settings_input(&z, draft, false)
                        .label(move || t!("settings.store_folder"))
                        .placeholder(move || placeholder.store_folder_path()),
                )
                .child(when(
                    move || !error.get().is_empty(),
                    move || pix(move || error.get(), 11.0, DANGER, &z),
                ))
                .child(
                    actions(&z)
                        .child(button(
                            &z,
                            move || t!("settings.store_browse"),
                            true,
                            move || browse_store(&browse),
                        ))
                        .child(button(
                            &z,
                            move || t!("settings.store_reset"),
                            true,
                            move || reset_store(&reset),
                        ))
                        .child(button(
                            &z,
                            move || t!("settings.store_save"),
                            true,
                            move || commit_store(&save),
                        )),
                )
                .child(pix(move || t!("settings.store_hint"), 11.0, TEXT_MUTED, &z)),
        ))
        .child(panel(
            &z,
            move || t!("settings.data_folder"),
            path_row(&z, move || app_data.clone()),
        ))
}

fn browse_store(ui: &Ui) {
    let ui = ui.clone();
    let picked = pick_paths(OpenDialog::folders());
    ui.spawn_worker(async move {
        match picked.await {
            Ok(Some(paths)) => {
                if let Some(path) = paths.into_iter().next() {
                    ui.store_draft.set(path.display().to_string());
                    ui.store_error.set(String::new());
                }
            }
            Ok(None) => {}
            Err(error) => ui
                .store_error
                .set(t!("settings.store_picker_failed", error = error)),
        }
    });
}

fn reset_store(ui: &Ui) {
    ui.store_draft.set(String::new());
    ui.store_error.set(String::new());
}

fn commit_store(ui: &Ui) {
    match ui.save_store(&ui.store_draft.get()) {
        Ok(()) => ui.store_error.set(String::new()),
        Err(error) => ui.store_error.set(error),
    }
}
