//! The floating task button, its badge and the right-side task drawer.

use super::task_cards::card_list;
use crate::palette::*;
use crate::ui::{Ui, button, pending_count, pix, scaled, states};
use nsh_base::{SheetEvent, sheet};
use nsh_core::prelude::*;

pub fn task_center(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let press = ui.clone();
    let label_z = z;
    let badge_z = z;
    let badge_ui = ui.clone();
    states(
        view()
            .absolute()
            .right(scaled(&z, 16.0))
            .bottom(scaled(&z, 40.0))
            .flex_row()
            .items_center()
            .gap(scaled(&z, 6.0))
            .px(scaled(&z, 10.0))
            .py(scaled(&z, 6.0))
            .bg(rgb(SURFACE_RAISED))
            .border(scaled(&z, 1.0))
            .border_color(rgb(BORDER))
            .cursor(Cursor::Pointer),
    )
    .hover(|style| style.bg(rgb(ACCENT_PRESS)))
    .on_press(move || press.task_open.set(true))
    .child(pix(move || t!("task.button"), 11.0, TEXT, &label_z))
    .child(badge(&badge_z, &badge_ui))
}

fn badge(z: &Signal<u8>, ui: &Ui) -> impl IntoElement {
    let z = *z;
    let gate = ui.clone();
    let count_ui = ui.clone();
    let count_z = z;
    when(
        move || !gate.task_open.get() && pending_count(&gate) > 0,
        move || {
            let count_ui = count_ui.clone();
            view()
                .items_center()
                .justify_center()
                .w(scaled(&z, 16.0))
                .h(scaled(&z, 16.0))
                .bg(rgb(DANGER))
                .animation(
                    KeyframeAnimation::new(Keyframes::Pulse)
                        .duration(900.0)
                        .frame_rate(20.0),
                )
                .child(pix(
                    move || pending_count(&count_ui).to_string(),
                    9.0,
                    BG,
                    &count_z,
                ))
        },
    )
}

pub fn task_drawer(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let open_ui = ui.clone();
    let change_ui = ui.clone();
    let header_ui = ui.clone();
    let body_ui = ui.clone();
    let z_body = z;
    sheet()
        .side(Placement::Right)
        .open(move || Some(open_ui.task_open.get()))
        .on_open_change(move |event: &SheetEvent| {
            if let SheetEvent::OpenChange(next, _) = event {
                change_ui.task_open.set(*next);
            }
        })
        .backdrop_style(|style| style.bg(rgba(0x00000066)))
        .child(
            view()
                .flex_col()
                .w(relative(0.42))
                .h_full()
                .bg(rgb(SURFACE))
                .border_l(scaled(&z_body, 1.0))
                .border_l_color(rgb(BORDER))
                .child(drawer_header(&z_body, &header_ui))
                .child(
                    scroll()
                        .flex_1()
                        .min_h(px(0.0))
                        .child(card_list(&z_body, &body_ui)),
                ),
        )
}

fn drawer_header(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let close = ui.clone();
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(scaled(&z, 8.0))
        .px(scaled(&z, 12.0))
        .py(scaled(&z, 10.0))
        .bg(rgb(SURFACE_RAISED))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER))
        .child(pix(move || t!("task.title"), 12.0, TEXT, &z))
        .child(button(
            &z,
            move || t!("task.close"),
            true,
            move || close.task_open.set(false),
        ))
}
