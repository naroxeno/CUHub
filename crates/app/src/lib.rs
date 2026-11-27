//! Cuprum app crate: the nsh root component and the shell it renders.

mod app_state;
mod components;
mod i18n;
mod pages;
mod palette;
mod setup;
mod shell;

use nsh_core::desktop::{WindowAttributes, WindowFrameStyle};
use nsh_core::element::desktop::window;
use nsh_core::prelude::*;

use crate::app_state::Ui;
use crate::palette::{BG, theme_color};

#[native_module("cuprum/native")]
fn native(registry: &mut Registry) {
    registry.assets(assets!());
    registry.register::<App>();
}

#[component]
pub fn App() -> impl IntoElement {
    let mut attrs = WindowAttributes::default();
    attrs.frame = Some(WindowFrameStyle::None);
    window()
        .id("main")
        .title("Cuprum")
        .default_size(size(px(1100.0), px(720.0)))
        .attributes(attrs)
        .content(app_content)
}

fn app_content() -> impl IntoElement {
    let ui = Ui::new();
    ui.start_background_tasks();
    let keyboard = shell::keyboard::Keyboard::new(&ui);
    provide_context(keyboard);

    let mut root = view()
        .flex_col()
        .size_full()
        .bg(theme_color(BG))
        .child(shell::title_bar(&ui))
        .child(
            view()
                .flex_row()
                .flex_1()
                .child(shell::sidebar(&ui))
                .child(view().flex_col().flex_1().child(shell::content(&ui))),
        )
        .child(shell::notice_toast(&ui))
        .child(keyboard.hint(ui.zoom, false))
        .child(shell::task_center(&ui))
        .child(shell::task_drawer(&ui))
        .child(pages::delete_overlay(&ui))
        .child(pages::bepinex_overlay(&ui))
        .child(shell::dev_overlay(&ui))
        .child(setup::wizard(&ui));
    for edge in shell::frame_edges(&ui) {
        root = root.child(edge);
    }
    root
}
