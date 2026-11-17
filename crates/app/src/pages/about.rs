//! About page: version, project links and license.

use nsh_core::prelude::*;

use crate::palette::*;
use crate::ui::{Ui, button, panel, pix, rise_in, scaled};

const REPOSITORY: &str = "https://github.com/naroxeno/CUHub";
const ISSUES: &str = "https://github.com/naroxeno/CUHub/issues";
const LICENSE: &str = "https://www.gnu.org/licenses/agpl-3.0.html";

pub fn about(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let version = concat!("v", env!("CARGO_PKG_VERSION"));
    view()
        .flex_col()
        .flex_1()
        .bind_motion(&rise_in())
        .child(panel(
            &z,
            move || t!("panel.about"),
            view()
                .flex_col()
                .gap(scaled(&z, 12.0))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 10.0))
                        .child(pix(move || t!("app.name"), 18.0, ACCENT, &z))
                        .child(pix(version, 11.0, TEXT_DIM, &z)),
                )
                .child(pix(move || t!("about.tagline"), 12.0, TEXT_MUTED, &z))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .gap(scaled(&z, 8.0))
                        .child(link_button(ui, "about.repository", REPOSITORY, &z))
                        .child(link_button(ui, "about.report_issues", ISSUES, &z))
                        .child(link_button(ui, "about.license_link", LICENSE, &z)),
                )
                .child(pix(move || t!("about.license_note"), 10.0, TEXT_DIM, &z))
                .child(pix(move || t!("about.disclaimer"), 10.0, TEXT_DIM, &z)),
        ))
}

fn link_button(ui: &Ui, key: &'static str, url: &'static str, z: &Signal<u8>) -> impl IntoElement {
    let ui = ui.clone();
    button(z, move || t!(key), true, move || ui.open_link(url))
}
