use cuprum_core::thunderstore::Package;
use nsh_core::prelude::*;

use crate::app_state::{RemoteSource, Ui};
use crate::components::{button, pix, scaled};
use crate::palette::*;

pub(super) fn refresh(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let busy = ui.thunderstore_refreshing;
    let ui = ui.clone();
    view().child(
        when(
            move || busy.get(),
            move || button(&z, move || t!("remote.refreshing"), false, || {}),
        )
        .otherwise(move || {
            let ui = ui.clone();
            button(
                &z,
                move || t!("remote.refresh_thunderstore"),
                true,
                move || ui.refresh_thunderstore(),
            )
        }),
    )
}

pub(super) fn refresh_error(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let error = ui.thunderstore_error;
    let source = ui.remote_source;
    view().child(when(
        move || {
            matches!(source.get(), RemoteSource::All | RemoteSource::Thunderstore)
                && error.get().is_some()
        },
        move || {
            pix(
                move || {
                    t!(
                        "remote.thunderstore_refresh_failed",
                        error = error.get().unwrap_or_default()
                    )
                },
                10.0,
                WARN,
                &z,
            )
        },
    ))
}

pub(super) fn row(z: &Signal<u8>, package: &Package, installed: Option<String>, ui: &Ui) -> View {
    let z = *z;
    let newest = package.versions.first();
    let mut content = view()
        .flex_col()
        .gap(scaled(&z, 4.0))
        .child(pix(package.name.clone(), 12.0, TEXT, &z))
        .child(pix(
            format!(
                "Thunderstore · {} · {}",
                package.owner,
                newest.map_or("", |v| &v.version_number)
            ),
            10.0,
            TEXT_MUTED,
            &z,
        ));
    if let Some(version) = newest.filter(|version| !version.description.is_empty()) {
        content = content.child(pix(version.description.clone(), 10.0, TEXT_DIM, &z));
    }
    let border = if installed.is_some() { ACCENT } else { SURFACE };
    if let Some(version) = installed {
        content = content.child(pix(
            move || t!("nexus.installed", version = version.clone()),
            10.0,
            TEXT_DIM,
            &z,
        ));
    } else {
        content = content.child(version_choices(&z, package, ui));
    }
    content
        .px(scaled(&z, 6.0))
        .py(scaled(&z, 6.0))
        .border_b(scaled(&z, 1.0))
        .border_b_color(theme_color(BORDER_DIM))
        .border_l(scaled(&z, 2.0))
        .border_l_color(theme_color(border))
}

fn version_choices(z: &Signal<u8>, package: &Package, ui: &Ui) -> View {
    let z = *z;
    let expanded = signal(false);
    let package = package.clone();
    let ui = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 4.0))
        .child(button(
            &z,
            move || t!("remote.thunderstore_versions"),
            true,
            move || expanded.set(!expanded.get()),
        ))
        .child(when(
            move || expanded.get(),
            move || {
                let mut choices = view().flex_row().flex_wrap().gap(scaled(&z, 4.0));
                for version in &package.versions {
                    choices =
                        choices.child(version_button(&z, &ui, &package, &version.version_number));
                }
                choices
            },
        ))
}

fn version_button(z: &Signal<u8>, ui: &Ui, package: &Package, version: &str) -> View {
    let z = *z;
    let ui = ui.clone();
    let version = version.to_owned();
    let label = version.clone();
    let package_id = package.full_name.clone();
    let name = package.name.clone();
    let busy = ui.remote_busy;
    let pending = ui.pending_install;
    view().child(
        when(
            move || busy.get().is_some() || pending.get().is_some(),
            move || {
                let label = label.clone();
                button(
                    &z,
                    move || {
                        t!(
                            "remote.thunderstore_install_version",
                            version = label.clone()
                        )
                    },
                    false,
                    || {},
                )
            },
        )
        .otherwise(move || {
            let label = version.clone();
            let version = version.clone();
            let id = package_id.clone();
            let name = name.clone();
            let ui = ui.clone();
            button(
                &z,
                move || {
                    t!(
                        "remote.thunderstore_install_version",
                        version = label.clone()
                    )
                },
                true,
                move || ui.install_thunderstore(id.clone(), name.clone(), version.clone()),
            )
        }),
    )
}
