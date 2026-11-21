//! Sidebar: wide labeled tree with group headers, narrow icon rail, fold toggle and footer.

use nsh_base::tooltip;
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
            move || pix(move || t!("nav.search"), 11.0, TEXT_MUTED, &z),
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
    let active_child = ui.clone();
    let open = ui.clone();
    let expanded = ui.clone();
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
        .hover(|s| s.bg(rgb(SURFACE_RAISED)))
        .focus_visible(|s| s.bg(rgb(BORDER_DIM)))
        .active(|s| s.bg(rgb(BORDER_DIM)))
        .cursor(Cursor::Pointer)
        .accessibility_state(move || AccessibilityState {
            expanded: Some(expanded.nav_closed_group.get() != group),
            ..AccessibilityState::default()
        })
        .on_press(move || {
            let closed = press.nav_closed_group.get() == group;
            animate_layout(fold_animation());
            press.nav_closed_group.set(if closed { "" } else { group });
        })
        .child(pix(move || t!(key), 12.0, TEXT, &z))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 7.0))
                .child(when(
                    move || {
                        active_child.nav_closed_group.get() == group
                            && nav::in_group(active_child.page.get(), group)
                    },
                    move || view().w(scaled(&z, 4.0)).h(scaled(&z, 4.0)).bg(rgb(ACCENT)),
                ))
                .child(pix(
                    move || {
                        if open.nav_closed_group.get() == group {
                            "▸"
                        } else {
                            "▾"
                        }
                    },
                    10.0,
                    TEXT_MUTED,
                    &z,
                )),
        )
}

fn nav_row(ui: Ui, entry: &'static NavEntry, child: bool) -> View {
    let z = ui.zoom;
    let page = ui.page;
    let is_page = matches!(&entry.kind, NavKind::Page(_));
    let selected = move || is_page && page.get() == entry.id;
    let glow = motion();
    effect(move || {
        if selected() {
            glow.play(MotionPreset::FadeIn);
        }
    });
    let key = entry.key;
    let row = view()
        .flex_row()
        .flex_1()
        .min_w(px(0.0))
        .items_center()
        .h(scaled(&z, if child { 24.0 } else { 28.0 }))
        .bg(move || {
            if selected() {
                rgb(SURFACE_RAISED)
            } else {
                rgb(SURFACE)
            }
        })
        .border_l(scaled(&z, 2.0))
        .border_l_color(move || {
            if selected() {
                rgb(ACCENT)
            } else {
                rgb(SURFACE)
            }
        })
        .px(scaled(&z, 9.0))
        .transition()
        .duration(STATE_MS)
        .hover(|s| s.bg(rgb(SURFACE_RAISED)))
        .focus_visible(|s| s.bg(rgb(BORDER_DIM)))
        .active(|s| s.bg(rgb(BORDER_DIM)).translate_y(px(PRESS_SHIFT)))
        .cursor(Cursor::Pointer)
        .accessibility_state(move || AccessibilityState {
            selected: selected(),
            ..AccessibilityState::default()
        })
        .on_press(move || run_entry(&ui, entry))
        .child(
            pix_c(
                move || t!(key),
                12.0,
                move || match (selected(), child) {
                    (true, false) => rgb(ACCENT),
                    (true, true) => rgb(TEXT),
                    (false, _) => rgb(TEXT_MUTED),
                },
                &z,
            )
            .min_w(px(0.0))
            .truncate()
            .bind_motion(&glow),
        );
    if child {
        view()
            .flex_row()
            .pl(scaled(&z, 17.0))
            .pr(scaled(&z, 6.0))
            .child(branch_guide(&z))
            .child(row)
    } else {
        view()
            .flex_row()
            .border_b(scaled(&z, 1.0))
            .border_b_color(rgb(BORDER_DIM))
            .child(row)
    }
}

fn branch_guide(z: &Signal<u8>) -> View {
    view()
        .flex_row()
        .items_center()
        .border_l(scaled(z, 1.0))
        .border_l_color(rgb(BORDER_DIM))
        .child(
            view()
                .w(scaled(z, 5.0))
                .h(scaled(z, 1.0))
                .bg(rgb(BORDER_DIM)),
        )
}

fn icon_row(ui: Ui, entry: &'static NavEntry) -> impl IntoElement {
    let z = ui.zoom;
    let bg = ui.clone();
    let edge = ui.clone();
    let text = ui.clone();
    let press = ui.clone();
    let key = entry.key;
    let page = ui.page;
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
        .focus_visible(|s| s.bg(rgb(BORDER_DIM)))
        .label(move || t!(key))
        .accessibility_state(move || AccessibilityState {
            selected: is_page && page.get() == entry.id,
            ..AccessibilityState::default()
        })
        .tooltip(sidebar_hint(&z, key))
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

fn sidebar_hint(z: &Signal<u8>, key: &'static str) -> impl IntoTooltip {
    tooltip().placement(Placement::Right).child(
        view()
            .px(scaled(z, 7.0))
            .py(scaled(z, 4.0))
            .bg(rgb(SURFACE_RAISED))
            .border(scaled(z, 1.0))
            .border_color(rgb(BORDER_DIM))
            .child(pix(move || t!(key), 11.0, TEXT, z)),
    )
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
            .bg(rgb(BORDER_DIM))
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
