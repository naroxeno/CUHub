//! The shell around the pages: sidebar, title bar, status line and developer overlay.

mod dev;
mod frame_edges;
pub mod nav;
mod sidebar;
mod statusline;
mod task_cards;
mod task_center;
mod title_bar;

pub use dev::dev_overlay;
pub use frame_edges::frame_edges;
pub use sidebar::sidebar;
pub use statusline::statusline;
pub use task_center::{task_center, task_drawer};
pub use title_bar::title_bar;

use nsh_core::prelude::*;

use crate::palette::*;
use crate::ui::{Ui, scaled};

pub fn content(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let mut pages = view().flex_col().gap(scaled(&z, 12.0)).p(scaled(&z, 12.0));
    for entry in nav::NAV.iter() {
        if let nav::NavKind::Page(render) = &entry.kind {
            pages = pages.child(page_region(ui, entry.id, *render));
        }
    }
    view()
        .flex_col()
        .flex_1()
        .bg(rgb(BG))
        .child(scroll().flex_1().child(pages))
}

fn page_region(ui: &Ui, id: &'static str, render: fn(&Ui) -> AnyElement) -> impl IntoElement {
    let condition = ui.clone();
    let page = ui.clone();
    when(move || condition.page.get() == id, move || render(&page))
}
