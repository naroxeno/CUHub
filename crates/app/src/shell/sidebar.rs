//! Sidebar: wide labeled tree with group headers, narrow icon rail, fold toggle and footer.

use nsh_core::prelude::*;
use nsh_core::{LayoutAnimation, LayoutTransition};

use super::nav::{self, NavEntry, NavKind};
use crate::palette::*;
use crate::ui::{PRESS_SHIFT, STATE_MS, Ui, pix, pix_c, scaled};

const WIDE_RAIL: f32 = 176.0;
const NARROW_RAIL: f32 = 44.0;
const FOLD_RESPONSE_MS: f32 = 220.0;
const FOLD_DAMPING: f32 = 0.8;

fn rail_width(folded: bool) -> f32 {
    if folded { NARROW_RAIL } else { WIDE_RAIL }
}

fn fold_animation() -> LayoutAnimation {
    LayoutAnimation {
        update: Some(LayoutTransition::spring(
            Spring::new(FOLD_RESPONSE_MS).damping_ratio(FOLD_DAMPING),
        )),
        ..LayoutAnimation::default()
    }
}

pub fn sidebar(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let width = ui.clone();
    let wide = ui.clone();
    let narrow = ui.clone();
    let wide_box = ui.clone();
    let narrow_box = ui.clone();
    view()
        .flex_col()
        .w(move || scaled(&z, rail_width(width.nav_collapsed.get()))())
        .h_full()
        .min_h(px(0.0))
        .overflow_hidden()
        .child(when(
            move || !wide_box.nav_collapsed.get(),
            move || search_box(wide_box.clone(), false),
        ))
        .child(when(
            move || narrow_box.nav_collapsed.get(),
            move || search_box(narrow_box.clone(), true),
        ))
        .child(when(
            move || !wide.nav_collapsed.get(),
            move || wide_rail(wide.clone()),
        ))
        .child(when(
            move || narrow.nav_collapsed.get(),
            move || narrow_rail(narrow.clone()),
        ))
}

fn search_box(ui: Ui, narrow: bool) -> impl IntoElement {
    let z = ui.zoom;
    let press = ui.clone();
    view()
        .flex_row()
        .items_center()
        .justify_center()
        .h(scaled(&z, 24.0))
        .mx(scaled(&z, 5.0))
        .my(scaled(&z, 6.0))
        .gap(scaled(&z, 6.0))
        .bg(rgb(SURFACE_RAISED))
        .border(scaled(&z, 1.0))
        .border_color(rgb(BORDER))
        .transition()
        .duration(STATE_MS)
        .hover(|s| s.border_color(rgb(ACCENT)))
        .active(|s| s.bg(rgb(BORDER_DIM)).translate_y(px(PRESS_SHIFT)))
        .cursor(Cursor::Pointer)
        .overflow_hidden()
        .on_press(move || press.notice.set(t!("notice.search_soon")))
        .child(pix("?", 10.0, TEXT_DIM, &z))
        .child(when(
            move || !narrow,
            move || pix(t!("nav.search"), 11.0, TEXT_MUTED, &z),
        ))
}

fn wide_rail(ui: Ui) -> impl IntoElement {
    let z = ui.zoom;
    let mut list = view()
        .flex_col()
        .w_full()
        .child(fold_toggle(ui.clone(), false));
    let mut previous: Option<&'static str> = None;
    for entry in nav::NAV.iter() {
        match entry.group {
            Some(group) => {
                if previous != Some(group) {
                    previous = Some(group);
                    list = list.child(group_header(ui.clone(), group));
                }
                let condition = ui.clone();
                let rows = ui.clone();
                list = list.child(when(
                    move || condition.nav_closed_group.get() != group,
                    move || nav_row(rows.clone(), entry, true),
                ));
            }
            None => {
                previous = None;
                list = list.child(nav_row(ui.clone(), entry, false));
            }
        }
    }
    view()
        .flex_col()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .bg(rgb(SURFACE))
        .border_r(scaled(&z, 1.0))
        .border_r_color(rgb(BORDER))
        .child(scroll().flex_1().min_h(px(0.0)).scrollbar(true).child(list))
        .child(footer(ui, false))
}

fn narrow_rail(ui: Ui) -> impl IntoElement {
    let z = ui.zoom;
    let mut list = view()
        .flex_col()
        .w_full()
        .child(fold_toggle(ui.clone(), true));
    let mut previous: Option<&'static str> = None;
    for entry in nav::NAV.iter() {
        if previous != entry.group {
            list = list.child(group_boundary(ui.clone(), previous, entry.group));
        }
        previous = entry.group;
        list = list.child(icon_row(ui.clone(), entry));
    }
    view()
        .flex_col()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .bg(rgb(SURFACE))
        .border_r(scaled(&z, 1.0))
        .border_r_color(rgb(BORDER))
        .child(scroll().flex_1().min_h(px(0.0)).scrollbar(true).child(list))
        .child(footer(ui, true))
}

fn fold_toggle(ui: Ui, narrow: bool) -> impl IntoElement {
    let z = ui.zoom;
    let press = ui.clone();
    view()
        .flex_row()
        .items_center()
        .justify_center()
        .h(scaled(&z, 24.0))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER_DIM))
        .hover(|s| s.bg(rgb(SURFACE_RAISED)))
        .active(|s| s.bg(rgb(BORDER_DIM)))
        .on_press(move || {
            animate_layout(fold_animation());
            let folded = press.nav_collapsed.get();
            press.nav_collapsed.set(!folded);
        })
        .child(pix(if narrow { ">>" } else { "<<" }, 10.0, TEXT_DIM, &z))
}

fn group_header(ui: Ui, group: &'static str) -> impl IntoElement {
    let z = ui.zoom;
    let press = ui.clone();
    let lit = ui.clone();
    let open = ui.clone();
    let key = nav::GROUPS
        .iter()
        .find(|candidate| candidate.id == group)
        .map(|candidate| candidate.key)
        .unwrap_or("nav.mods");
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .h(scaled(&z, 28.0))
        .px(scaled(&z, 10.0))
        .bg(rgb(SURFACE))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER_DIM))
        .border_l(scaled(&z, 2.0))
        .border_l_color(rgb(SURFACE))
        .hover(|s| s.bg(rgb(SURFACE_RAISED)))
        .active(|s| s.bg(rgb(BORDER_DIM)))
        .on_press(move || {
            let closed = press.nav_closed_group.get() == group;
            press.nav_closed_group.set(if closed { "" } else { group });
        })
        .child(pix_c(
            move || t!(key),
            12.0,
            move || {
                if nav::in_group(lit.page.get(), group) {
                    rgb(ACCENT)
                } else {
                    rgb(TEXT)
                }
            },
            &z,
        ))
        .child(pix_c(
            move || {
                if open.nav_closed_group.get() == group {
                    "▸"
                } else {
                    "▾"
                }
            },
            10.0,
            move || rgb(TEXT_MUTED),
            &z,
        ))
}

fn nav_row(ui: Ui, entry: &'static NavEntry, child: bool) -> impl IntoElement {
    let z = ui.zoom;
    let bg = ui.clone();
    let edge = ui.clone();
    let text = ui.clone();
    let press = ui.clone();
    let lit = ui.clone();
    let glow = motion();
    let is_page = matches!(&entry.kind, NavKind::Page(_));
    effect(move || {
        if is_page && lit.page.get() == entry.id {
            glow.play(MotionPreset::FadeIn);
        }
    });
    let key = entry.key;
    view()
        .flex_row()
        .items_center()
        .h(scaled(&z, if child { 26.0 } else { 28.0 }))
        .bg(move || {
            if is_page && bg.page.get() == entry.id {
                rgb(SURFACE_RAISED)
            } else {
                rgb(SURFACE)
            }
        })
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER_DIM))
        .border_l(scaled(&z, 2.0))
        .border_l_color(move || {
            if is_page && edge.page.get() == entry.id {
                rgb(ACCENT)
            } else {
                rgb(SURFACE)
            }
        })
        .transition()
        .duration(STATE_MS)
        .hover(|s| s.bg(rgb(SURFACE_RAISED)))
        .active(|s| s.bg(rgb(BORDER_DIM)).translate_y(px(PRESS_SHIFT)))
        .cursor(Cursor::Pointer)
        .on_press(move || run_entry(&press, entry))
        .child(view().w(scaled(&z, if child { 16.0 } else { 0.0 })))
        .child(
            view()
                .w(scaled(&z, 1.0))
                .h(scaled(&z, if child { 26.0 } else { 28.0 }))
                .bg(rgb(BORDER)),
        )
        .child(view().w(scaled(&z, 8.0)))
        .child(
            view().flex_1().px(scaled(&z, 2.0)).child(
                pix_c(
                    move || t!(key),
                    12.0,
                    move || {
                        if is_page && text.page.get() == entry.id {
                            rgb(ACCENT)
                        } else {
                            rgb(TEXT_MUTED)
                        }
                    },
                    &z,
                )
                .bind_motion(&glow),
            ),
        )
}

fn icon_row(ui: Ui, entry: &'static NavEntry) -> impl IntoElement {
    let z = ui.zoom;
    let bg = ui.clone();
    let edge = ui.clone();
    let text = ui.clone();
    let press = ui.clone();
    let is_page = matches!(&entry.kind, NavKind::Page(_));
    view()
        .flex_row()
        .items_center()
        .justify_center()
        .h(scaled(&z, 28.0))
        .bg(move || {
            if is_page && bg.page.get() == entry.id {
                rgb(SURFACE_RAISED)
            } else {
                rgb(SURFACE)
            }
        })
        .border_l(scaled(&z, 2.0))
        .border_l_color(move || {
            if is_page && edge.page.get() == entry.id {
                rgb(ACCENT)
            } else {
                rgb(SURFACE)
            }
        })
        .transition()
        .duration(STATE_MS)
        .hover(|s| s.bg(rgb(SURFACE_RAISED)))
        .active(|s| s.bg(rgb(BORDER_DIM)).translate_y(px(PRESS_SHIFT)))
        .cursor(Cursor::Pointer)
        .on_press(move || run_entry(&press, entry))
        .child(pix_c(
            move || entry.icon.to_owned(),
            10.0,
            move || {
                if is_page && text.page.get() == entry.id {
                    rgb(ACCENT)
                } else {
                    rgb(TEXT_MUTED)
                }
            },
            &z,
        ))
}

fn group_boundary(
    ui: Ui,
    previous: Option<&'static str>,
    next: Option<&'static str>,
) -> impl IntoElement {
    let z = ui.zoom;
    if previous.is_some() || next.is_some() {
        view()
            .h(scaled(&z, 1.0))
            .w_full()
            .bg(rgb(BORDER))
            .into_element()
    } else {
        view().h(px(0.0)).into_element()
    }
}

fn footer(ui: Ui, narrow: bool) -> impl IntoElement {
    let launch = ui.clone();
    let z = ui.zoom;
    let mut row = view()
        .flex_row()
        .items_center()
        .justify_center()
        .h(scaled(&z, 28.0))
        .border_t(scaled(&z, 1.0))
        .border_t_color(rgb(BORDER))
        .bg(rgb(SURFACE))
        .hover(|s| s.bg(rgb(SURFACE_RAISED)))
        .active(|s| s.bg(rgb(BORDER_DIM)))
        .gap(scaled(&z, 6.0))
        .on_press(move || run_entry(&launch, &nav::LAUNCH_ENTRY))
        .child(pix(nav::LAUNCH_ENTRY.icon, 10.0, ACCENT, &z));
    if !narrow {
        row = row.child(pix(move || t!("topbar.launch"), 12.0, TEXT, &z));
    }
    let mut footer = view().flex_col().child(row);
    if !narrow {
        footer = footer.child(version_row(ui));
    }
    footer
}

fn version_row(ui: Ui) -> impl IntoElement {
    let z = ui.zoom;
    let tap = ui.clone();
    view()
        .h(scaled(&z, 24.0))
        .items_center()
        .justify_center()
        .border_t(scaled(&z, 1.0))
        .border_t_color(rgb(BORDER_DIM))
        .on_press(move || tap_version(&tap))
        .child(pix(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            10.0,
            TEXT_DIM,
            &z,
        ))
}

fn tap_version(ui: &Ui) {
    let now = std::time::Instant::now();
    let chained = ui
        .last_tap
        .get()
        .map(|then| now.duration_since(then).as_secs() < 3)
        .unwrap_or(false);
    let taps = if chained { ui.taps.get() + 1 } else { 1 };
    if taps >= 5 {
        ui.taps.set(0);
        ui.last_tap.set(None);
        ui.dev.set(true);
    } else {
        ui.taps.set(taps);
        ui.last_tap.set(Some(now));
    }
}

fn run_entry(ui: &Ui, entry: &'static NavEntry) {
    match entry.kind {
        NavKind::Page(_) => ui.page.set(entry.id),
        NavKind::Action(run) => run(ui),
    }
}
