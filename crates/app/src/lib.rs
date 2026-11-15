//! Cuhub app crate: the nsh root component and the shell it renders.

mod i18n;
mod pages;
mod palette;
mod shell;
mod ui;

use nsh_core::prelude::*;

use crate::palette::BG;
use crate::ui::Ui;

#[native_module("cuhub/native")]
fn native(registry: &mut Registry) {
    registry.assets(assets!());
    registry.register::<App>();
}

#[component]
pub fn App() -> impl IntoElement {
    let ui = Ui::new();
    ui.start_background_tasks();
    view()
        .flex_col()
        .size_full()
        .bg(rgb(BG))
        .child(shell::top_bar(&ui))
        .child(
            view()
                .flex_row()
                .flex_1()
                .child(shell::sidebar(&ui))
                .child(view().flex_col().flex_1().child(shell::content(&ui))),
        )
        .child(shell::statusline(&ui))
        .child(pages::delete_overlay(&ui))
        .child(shell::dev_overlay(&ui))
}
