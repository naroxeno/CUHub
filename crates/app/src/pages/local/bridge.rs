//! Presentation for the dedicated bridge package on the local mods page.

use std::path::{Path, PathBuf};

use cuhub_core::bridge::install::{CORE_FILE, PLUGIN_FILE};
use cuhub_core::hub::BridgeInstallStatus;
use nsh_core::prelude::*;

use crate::palette::*;
use crate::ui::{LocalModRow, Ui, bridge_color, bridge_text, button, pix, pix_c, scaled};

pub(super) fn is_bridge_row(row: &LocalModRow, status: Option<&BridgeInstallStatus>) -> bool {
    status.is_some_and(|status| {
        status.installed
            && !row.managed
            && row.item.source_path == status.target
            && has_bridge_pair(&status.target, &row.item.dll_paths, row.item.disabled)
    })
}

fn has_bridge_pair(target: &Path, files: &[PathBuf], disabled: bool) -> bool {
    let suffix = if disabled { ".disabled" } else { "" };
    files.len() == 2
        && files.contains(&target.join(format!("{PLUGIN_FILE}{suffix}")))
        && files.contains(&target.join(format!("{CORE_FILE}{suffix}")))
}

pub(super) fn bridge_section(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let snapshot = ui.bridge;
    let feedback = ui.bridge_install_text;
    let state = ui.clone();
    let details = signal(false);
    view()
        .flex_col()
        .gap(scaled(&z, 6.0))
        .px(scaled(&z, 9.0))
        .py(scaled(&z, 7.0))
        .bg(rgb(SURFACE_RAISED))
        .border(scaled(&z, 1.0))
        .border_color(rgb(BORDER_DIM))
        .border_l(scaled(&z, 2.0))
        .border_l_color(rgb(LINK))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(scaled(&z, 8.0))
                .child(pix(move || t!("local.bridge_title"), 12.0, TEXT, &z))
                .child(pix(move || t!("local.bridge_component"), 10.0, LINK, &z)),
        )
        .child(pix(move || install_state(&state), 11.0, TEXT, &z))
        .child(pix_c(
            move || bridge_text(snapshot.get()),
            10.0,
            move || rgb(bridge_color(&snapshot.get())),
            &z,
        ))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap(scaled(&z, 6.0))
                .child(install_button(&z, ui))
                .child(button(
                    &z,
                    move || {
                        if details.get() {
                            t!("local.hide_details")
                        } else {
                            t!("common.details")
                        }
                    },
                    true,
                    move || details.set(!details.get()),
                )),
        )
        .child(when(move || details.get(), {
            let status = ui.bridge_install;
            move || bridge_details(&z, status)
        }))
        .child(when(
            move || !feedback.get().is_empty(),
            move || pix(move || feedback.get(), 10.0, TEXT_MUTED, &z),
        ))
}

fn install_state(ui: &Ui) -> String {
    let status = ui.bridge_install.get();
    let disabled = ui
        .local_rows
        .get()
        .iter()
        .any(|row| row.item.disabled && is_bridge_row(row, status.as_ref()));
    let label = ui.bridge_install_label();
    if disabled {
        t!("local.bridge_installed_disabled", installed = label)
    } else {
        label
    }
}

fn install_button(z: &Signal<u8>, ui: &Ui) -> impl IntoElement {
    let z = *z;
    let busy = ui.bridge_install_busy;
    let status = ui.bridge_install;
    let action = ui.clone();
    when(
        move || busy.get(),
        move || button(&z, move || t!("local.bridge_installing"), false, || {}),
    )
    .otherwise(move || {
        let action = action.clone();
        button(
            &z,
            move || {
                if status.get().is_some_and(|status| status.installed) {
                    t!("local.bridge_reinstall")
                } else {
                    t!("local.bridge_install")
                }
            },
            true,
            move || action.start_bridge_install(),
        )
    })
}

fn bridge_details(z: &Signal<u8>, status: Signal<Option<BridgeInstallStatus>>) -> View {
    let z = *z;
    view()
        .flex_col()
        .gap(scaled(&z, 3.0))
        .child(pix(
            move || t!("local.bridge_description"),
            10.0,
            TEXT_MUTED,
            &z,
        ))
        .child(pix(
            move || t!("local.bridge_bulk_scope"),
            10.0,
            TEXT_MUTED,
            &z,
        ))
        .child(when(
            move || status.get().is_some(),
            move || {
                pix(
                    move || {
                        status
                            .get()
                            .map_or_else(String::new, |status| status.target.display().to_string())
                    },
                    10.0,
                    TEXT_MUTED,
                    &z,
                )
            },
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_only_the_complete_pair_under_the_install_target() {
        let target = Path::new("BepInEx/plugins/CuhubBridge");
        let plugin = target.join(PLUGIN_FILE);
        let core = target.join(CORE_FILE);

        assert!(has_bridge_pair(
            target,
            &[plugin.clone(), core.clone()],
            false
        ));
        assert!(!has_bridge_pair(
            target,
            std::slice::from_ref(&plugin),
            false
        ));
        assert!(!has_bridge_pair(
            target,
            &[plugin.clone(), core, target.join("Other.dll")],
            false
        ));
        assert!(!has_bridge_pair(
            target,
            &[plugin, PathBuf::from(CORE_FILE)],
            false
        ));
        assert!(has_bridge_pair(
            target,
            &[
                target.join(format!("{PLUGIN_FILE}.disabled")),
                target.join(format!("{CORE_FILE}.disabled"))
            ],
            true
        ));
    }
}
