//! Right-edge task entry and task drawer.

use super::keyboard::{Area, Keyboard};
use super::{icon_masks, task_cards::card_list};
use crate::app_state::{Ui, attention_count};
use crate::components::{STATE_MS, button, focus_ring, pix, pix_c, pixel_icon, scaled};
use crate::palette::*;
use nsh_base::{SheetEvent, button as semantic_button, sheet};
use nsh_core::prelude::*;

pub fn task_center(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let keyboard = Keyboard::get();
    let hover = signal(false);
    let focused = signal(false);
    let trigger = keyboard.task_entry;
    keyboard.track_area(trigger, Area::TaskEntry);
    trigger.on_focus_in(move || focused.set(true));
    trigger.on_focus_out(move || focused.set(false));
    let count = computed({
        let ui = ui.clone();
        move || attention_count(&ui)
    });
    let compact = move || if count.get() == 0 { 26.0 } else { 40.0 };
    semantic_button()
        .element_ref(&trigger)
        .accessibility_label(move || {
            Some(if count.get() == 0 {
                t!("task.title").into()
            } else {
                t!("task.entry_accessible", count = count.get()).into()
            })
        })
        .absolute()
        .top(scaled(&z, 50.0))
        .right(scaled(&z, 15.0))
        .w(move || {
            let label_width = if hover.get() || focused.get() {
                38.0
            } else {
                0.0
            };
            scaled(&z, compact() + label_width)()
        })
        .h(scaled(&z, 22.0))
        .flex_row()
        .items_center()
        .justify_end()
        .overflow_hidden()
        .bg(theme_color(SURFACE_RAISED))
        .border(scaled(&z, 1.0))
        .border_color(theme_color(BORDER))
        .hover(|style| style.bg(theme_color(ACCENT_SURFACE)))
        .focus_visible(focus_ring(&z))
        .active(|style| style.bg(theme_color(ACCENT_PRESS)))
        .transition()
        .duration(STATE_MS)
        .cursor(Cursor::Pointer)
        .on_mouse_enter(move |_: &MouseEvent| hover.set(true))
        .on_mouse_leave(move |_: &MouseEvent| hover.set(false))
        .on_press(move || {
            keyboard.open_tasks(keyboard.active(Area::TaskEntry).then_some(Area::TaskEntry))
        })
        .child(
            view()
                .flex_row()
                .flex_shrink_0()
                .items_center()
                .child(pix(move || t!("task.button"), 11.0, TEXT, &z))
                .child(
                    view()
                        .flex_row()
                        .w(move || scaled(&z, compact())())
                        .justify_center()
                        .items_center()
                        .gap(scaled(&z, 4.0))
                        .child(pixel_icon(icon_masks::TASKS, theme_color(TEXT_MUTED), &z))
                        .child(when(
                            move || count.get() > 0,
                            move || {
                                pix_c(
                                    move || {
                                        if count.get() > 99 {
                                            "99+".to_owned()
                                        } else {
                                            count.get().to_string()
                                        }
                                    },
                                    10.0,
                                    theme_color(WARN),
                                    &z,
                                )
                            },
                        )),
                ),
        )
}

pub fn task_drawer(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let open_ui = ui.clone();
    let change_ui = ui.clone();
    let header_ui = ui.clone();
    let body_ui = ui.clone();
    let z_body = z;
    let keyboard = Keyboard::get();
    let area = element_ref();
    keyboard.track_area(area, Area::Tasks);
    sheet()
        .side(Placement::Right)
        .accessibility_label(move || Some(t!("task.title").into()))
        .popup_style(|style| style.w(relative(0.55)))
        .open(move || Some(open_ui.task_open.get()))
        .on_open({
            let open = ui.task_open;
            move || {
                if open.get_untracked() && !keyboard.tasks.has_focus() {
                    keyboard.tasks.focus_first();
                }
            }
        })
        .on_close(move || keyboard.restore_after_tasks())
        .on_open_change(move |event: &SheetEvent| {
            if let SheetEvent::OpenChange(next, _) = event {
                change_ui.task_open.set(*next);
            }
        })
        .backdrop_style(|style| style.bg(rgba(0x00000066)))
        .child(
            view()
                .element_ref(&area)
                .flex_col()
                .w_full()
                .h_full()
                .bg(theme_color(SURFACE))
                .border_l(scaled(&z_body, 1.0))
                .border_l_color(theme_color(BORDER))
                .child(drawer_header(&z_body, &header_ui))
                .child(
                    scroll()
                        .element_ref(&keyboard.tasks.scroll)
                        .on_scroll(move |event: &ScrollEvent| keyboard.tasks.on_scroll(event))
                        .w_full()
                        .flex_1()
                        .min_h(px(0.0))
                        .child(card_list(&z_body, &body_ui)),
                )
                .child(keyboard.hint(z_body, true)),
        )
}

fn drawer_header(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let close = ui.clone();
    view()
        .flex_row()
        .w_full()
        .items_center()
        .justify_between()
        .gap(scaled(&z, 8.0))
        .px(scaled(&z, 12.0))
        .py(scaled(&z, 10.0))
        .bg(theme_color(SURFACE_RAISED))
        .border_b(scaled(&z, 1.0))
        .border_b_color(theme_color(BORDER))
        .child(pix(move || t!("task.title"), 12.0, TEXT, &z))
        .child(
            button(
                &z,
                move || t!("task.close"),
                true,
                move || close.task_open.set(false),
            )
            .element_ref(&Keyboard::get().tasks.fallback),
        )
}
