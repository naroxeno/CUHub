//! Cuhub app crate: the nsh root component and the shell it renders.

mod i18n;
mod pages;
mod palette;
mod setup;
mod shell;
mod ui;

use nsh_core::desktop::{WindowAttributes, WindowFrameStyle};
use nsh_core::element::desktop::current_window;
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

    let mut attrs = WindowAttributes::default();
    attrs.frame = Some(WindowFrameStyle::None);
    current_window().set(attrs);

    let mut root = view()
        .flex_col()
        .size_full()
        .bg(rgb(BG))
        .child(shell::title_bar(&ui))
        .child(
            view()
                .flex_row()
                .flex_1()
                .child(shell::sidebar(&ui))
                .child(view().flex_col().flex_1().child(shell::content(&ui))),
        )
        .child(shell::statusline(&ui))
        .child(pages::delete_overlay(&ui))
        .child(pages::bepinex_overlay(&ui))
        .child(shell::dev_overlay(&ui))
        .child(setup::wizard(&ui));
    for edge in shell::frame_edges(&ui) {
        root = root.child(edge);
    }
    root
}
