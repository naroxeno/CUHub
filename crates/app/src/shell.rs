//! The shell around the pages: top bar, sidebar, status line and developer overlay.

use nsh_core::prelude::*;

use crate::pages;
use crate::palette::*;
use crate::ui::{PAGES, Ui, button, fade_in, panel, pix, pix_c, rise_in, scaled};

pub fn top_bar(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let launch = ui.clone();
    let report = ui.clone();
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .h(scaled(&z, 32.0))
        .px(scaled(&z, 10.0))
        .bg(rgb(SURFACE))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 8.0))
                .child(pix("CUHUB", 14.0, TEXT, &z))
                .child(pix("0.1.0", 10.0, TEXT_DIM, &z)),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 8.0))
                .child(button(&z, "Launch Game", true, move || {
                    launch.launch_game();
                }))
                .child(button(&z, "Report issues", false, move || {
                    let _ = &report;
                })),
        )
}

pub fn sidebar(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let mut items = view().flex_col().flex_1();
    for (index, label) in PAGES.iter().enumerate() {
        items = items.child(nav_item(ui, index, label));
    }
    view()
        .flex_col()
        .w(scaled(&z, 176.0))
        .bg(rgb(SURFACE))
        .border_r(scaled(&z, 1.0))
        .border_r_color(rgb(BORDER))
        .child(items)
        .child(version_row(ui))
}

fn nav_item(ui: &Ui, index: usize, label: &'static str) -> impl IntoElement {
    let z = ui.zoom;
    let bg = ui.clone();
    let edge = ui.clone();
    let text = ui.clone();
    let press = ui.clone();
    let lit = ui.clone();
    let glow = motion();
    effect(move || {
        if lit.page.get() == index {
            glow.play(MotionPreset::FadeIn);
        }
    });
    view()
        .flex_row()
        .items_center()
        .h(scaled(&z, 28.0))
        .px(scaled(&z, 10.0))
        .bg(move || {
            if bg.page.get() == index {
                rgb(SURFACE_RAISED)
            } else {
                rgb(SURFACE)
            }
        })
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER_DIM))
        .border_l(scaled(&z, 2.0))
        .border_l_color(move || {
            if edge.page.get() == index {
                rgb(ACCENT)
            } else {
                rgb(SURFACE)
            }
        })
        .hover(|s| s.bg(rgb(SURFACE_RAISED)))
        .active(|s| s.bg(rgb(BORDER_DIM)))
        .on_press(move || press.page.set(index))
        .child(
            pix_c(
                label,
                12.0,
                move || {
                    if text.page.get() == index {
                        rgb(ACCENT)
                    } else {
                        rgb(TEXT_MUTED)
                    }
                },
                &z,
            )
            .bind_motion(&glow),
        )
}

fn version_row(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let tap = ui.clone();
    view()
        .h(scaled(&z, 24.0))
        .items_center()
        .justify_center()
        .border_t(scaled(&z, 1.0))
        .border_t_color(rgb(BORDER_DIM))
        .on_press(move || tap_version(&tap))
        .child(pix("v0.1.0", 10.0, TEXT_DIM, &z))
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

pub fn statusline(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let state = ui.clone();
    let watched = ui.clone();
    let toast = motion();
    effect(move || {
        if !watched.notice.get().is_empty() {
            toast.play(MotionPreset::FadeInLeft);
        }
    });
    view()
        .flex_row()
        .items_center()
        .h(scaled(&z, 24.0))
        .px(scaled(&z, 8.0))
        .bg(rgb(SURFACE))
        .border_t(scaled(&z, 1.0))
        .border_t_color(rgb(BORDER))
        .child(
            text(move || {
                let notice = state.notice.get();
                if notice.is_empty() {
                    format!(
                        "Ready · {} · {}x",
                        PAGES[state.page.get()],
                        state.zoom.get()
                    )
                } else {
                    notice
                }
            })
            .text_size(scaled(&z, 10.0))
            .font_family(crate::ui::PIXEL_FONT.family)
            .text_color(rgb(TEXT_MUTED))
            .bind_motion(&toast),
        )
}

pub fn content(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    view().flex_col().flex_1().bg(rgb(BG)).child(
        scroll().flex_1().child(
            view()
                .flex_col()
                .gap(scaled(&z, 12.0))
                .p(scaled(&z, 12.0))
                .child(page_region(ui, 0))
                .child(page_region(ui, 1))
                .child(page_region(ui, 2))
                .child(page_region(ui, 3))
                .child(page_region(ui, 4))
                .child(page_region(ui, 5)),
        ),
    )
}

fn page_region(ui: &Ui, index: usize) -> If {
    let condition = ui.clone();
    let page = ui.clone();
    when(
        move || condition.page.get() == index,
        move || page_for(index, &page),
    )
}

fn page_for(index: usize, ui: &Ui) -> AnyElement {
    match index {
        0 => pages::nexus(ui).into_element(),
        1 => pages::local_mods(ui).into_element(),
        2 => pages::multiplayer(ui).into_element(),
        3 => pages::skins_backups(ui).into_element(),
        4 => pages::hub_home(ui).into_element(),
        _ => pages::settings(ui).into_element(),
    }
}

pub fn dev_overlay(ui: &Ui) -> impl IntoElement {
    let show = ui.clone();
    let close = ui.clone();
    let done = ui.clone();
    when(
        move || show.dev.get(),
        move || {
            let z = close.zoom;
            let page = PAGES[close.page.get()];
            let zoom = close.zoom.get();
            let veil = fade_in();
            let card = rise_in();
            view()
                .absolute()
                .inset(px(0.0))
                .w_full()
                .h_full()
                .bg(rgba(0x000000CC))
                .bind_motion(&veil)
                .items_center()
                .justify_center()
                .child(
                    panel(
                        &z,
                        "DEVELOPER CONSOLE",
                        view()
                            .flex_col()
                            .gap(scaled(&z, 8.0))
                            .child(pix("nsh 0.5.1 · prototype shell", 12.0, TEXT_MUTED, &z))
                            .child(pix(format!("page = {page}"), 12.0, TEXT, &z))
                            .child(pix(format!("zoom = {zoom}x"), 12.0, TEXT, &z))
                            .child(button(&z, "Close", true, move || done.dev.set(false))),
                    )
                    .w(scaled(&z, 320.0))
                    .bind_motion(&card),
                )
        },
    )
}
