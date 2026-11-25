//! Hub home: four reorderable summary tiles and the cached announcement.

use super::settings::game_folder_path;
use crate::app_state::Ui;
use crate::components::{fade_in, pix, pix_c, rise_in, scaled};
use crate::palette::*;
use crate::shell::nav;
use cuhub_core::install::Status as BepInExStatus;
use nsh_core::EllipsizeMode;
use nsh_core::prelude::*;

const TILE_IDS: [&str; 4] = ["game", "mods", "catalogue", "ready"];
const DRAG_THRESHOLD: f32 = 24.0;

#[derive(Clone, Copy)]
struct HomeDrag {
    order: Signal<Vec<String>>,
    dragging: Signal<Option<usize>>,
}

#[derive(Clone, Copy)]
struct DragOffset {
    dx: f32,
    dy: f32,
    threshold: f32,
}

struct CardParts {
    tag: &'static str,
    value: AnyElement,
    detail: AnyElement,
}

pub fn hub_home(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let order = signal(normalized_order(&ui.settings.get().home_order));
    let drag = HomeDrag {
        order,
        dragging: signal(None::<usize>),
    };
    let announcement = ui.announcement.get();
    let tiles = ui.clone();
    view()
        .flex_col()
        .flex_1()
        .bind_motion(&rise_in())
        .gap(scaled(&z, 12.0))
        .child(
            view()
                .flex_col()
                .gap(scaled(&z, 4.0))
                .child(pix(move || nav::title(nav::HOME), 14.0, TEXT, &z))
                .child(pix(move || t!("home.order_hint"), 10.0, TEXT_DIM, &z)),
        )
        .child(
            view()
                .grid()
                .grid_cols(2)
                .gap(scaled(&z, 12.0))
                .child(each(
                    order,
                    |id| id.clone(),
                    move |id, index| {
                        let parts = card_parts(&tiles, id.get_untracked().as_str());
                        tile(&tiles, drag, index, parts)
                    },
                ))
                .child(announcement_card(ui, announcement)),
        )
}

fn tile(ui: &Ui, drag: HomeDrag, index: ReadSignal<usize>, parts: CardParts) -> View {
    let z = ui.zoom;
    let offset_x = animated_value(0.0);
    let offset_y = animated_value(0.0);
    let spring_x = offset_x;
    let spring_y = offset_y;
    let save_ui = ui.clone();
    let HomeDrag { dragging, .. } = drag;
    let CardParts { tag, value, detail } = parts;
    let body = view()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .gap(scaled(&z, 8.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .child(pix(move || t!(tag), 10.0, TEXT_DIM, &z))
                .child(pix("::", 10.0, TEXT_DIM, &z)),
        )
        .child(value)
        .child(detail);
    view()
        .flex_col()
        .min_w(px(0.0))
        .min_h(scaled(&z, 96.0))
        .p(scaled(&z, 12.0))
        .bg(theme_color_with(move || {
            if dragging.get() == Some(index.get()) {
                SURFACE_RAISED
            } else {
                SURFACE
            }
        }))
        .border(scaled(&z, 1.0))
        .border_color(theme_color_with(move || {
            if dragging.get() == Some(index.get()) {
                ACCENT
            } else {
                BORDER_DIM
            }
        }))
        .hover(|s| {
            s.bg(theme_color(SURFACE_RAISED))
                .border_color(theme_color(ACCENT))
        })
        .z_index(move || {
            if dragging.get() == Some(index.get()) {
                10
            } else {
                0
            }
        })
        .bind_translate_x(&offset_x)
        .bind_translate_y(&offset_y)
        .pan_values(Some(&offset_x), Some(&offset_y))
        .on_pan_start(move |_event: &PanEvent| {
            dragging.set(Some(index.get_untracked()));
        })
        .on_pan_end(move |event: &PanEvent| {
            finish_drag(&save_ui, drag, index, event);
            spring_x.spring(0.0, Spring::default());
            spring_y.spring(0.0, Spring::default());
        })
        .bind_motion(&fade_in())
        .child(body)
}

fn finish_drag(ui: &Ui, drag: HomeDrag, index: ReadSignal<usize>, event: &PanEvent) {
    let HomeDrag { order, dragging } = drag;
    let from = index.get_untracked();
    dragging.set(None);
    let current = order.get_untracked();
    let target = if event.cancelled {
        from
    } else {
        reorder_target(
            &current,
            from,
            DragOffset {
                dx: event.translation.x.0,
                dy: event.translation.y.0,
                threshold: DRAG_THRESHOLD * ui.zoom.get_untracked() as f32,
            },
        )
    };
    if target != from {
        let next = moved(&current, from, target);
        order.set(next.clone());
        ui.save_home_order(next);
    }
}

fn reorder_target(order: &[String], from: usize, drag: DragOffset) -> usize {
    if order.is_empty() {
        return from;
    }
    let candidate = if drag.dx.abs() >= drag.dy.abs() {
        if drag.dx > drag.threshold {
            from + 1
        } else if drag.dx < -drag.threshold {
            from.saturating_sub(1)
        } else {
            from
        }
    } else if drag.dy > drag.threshold {
        from + 2
    } else if drag.dy < -drag.threshold {
        from.saturating_sub(2)
    } else {
        from
    };
    candidate.min(order.len() - 1)
}

fn normalized_order(order: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = Vec::with_capacity(TILE_IDS.len());
    for id in order {
        if TILE_IDS.contains(&id.as_str()) && !normalized.contains(id) {
            normalized.push(id.clone());
        }
    }
    for id in TILE_IDS {
        if !normalized.iter().any(|saved| saved.as_str() == id) {
            normalized.push(id.to_owned());
        }
    }
    normalized
}

fn moved(order: &[String], from: usize, to: usize) -> Vec<String> {
    if from == to || from >= order.len() || to >= order.len() {
        return order.to_vec();
    }
    let mut next = order.to_vec();
    let id = next.remove(from);
    next.insert(to, id);
    next
}

fn announcement_card(ui: &Ui, announcement: Option<String>) -> View {
    let z = ui.zoom;
    let color = if announcement.is_some() {
        TEXT
    } else {
        TEXT_DIM
    };
    let message = move || announcement.clone().unwrap_or_else(|| t!("hub.not_loaded"));
    view()
        .flex_col()
        .col_span_full()
        .gap(scaled(&z, 8.0))
        .p(scaled(&z, 12.0))
        .bg(theme_color(SURFACE))
        .border(scaled(&z, 1.0))
        .border_color(theme_color(BORDER_DIM))
        .bind_motion(&fade_in())
        .child(pix(move || t!("home.announcement.tag"), 10.0, TEXT_DIM, &z))
        .child(pix(message, 12.0, color, &z))
        .child(pix(
            move || t!("home.announcement.read_only"),
            10.0,
            TEXT_DIM,
            &z,
        ))
}

fn card_parts(ui: &Ui, id: &str) -> CardParts {
    let z = ui.zoom;
    match id {
        "game" => {
            let folder = ui.clone();
            let data = ui.clone();
            CardParts {
                tag: "home.game.tag",
                value: pix(
                    move || game_folder_path(&folder).unwrap_or_else(|| t!("common.not_set")),
                    11.0,
                    TEXT,
                    &z,
                )
                .number_of_lines(1)
                .text_overflow(EllipsizeMode::Middle)
                .into_element(),
                detail: view()
                    .flex_col()
                    .gap(scaled(&z, 2.0))
                    .child(pix(move || t!("home.game.detail"), 10.0, TEXT_DIM, &z))
                    .child(pix(
                        move || t!("home.local_data", path = data.app_data.clone()),
                        10.0,
                        TEXT_DIM,
                        &z,
                    ))
                    .into_element(),
            }
        }
        "mods" => {
            let mods = ui.mods;
            CardParts {
                tag: "home.mods.tag",
                value: pix(move || mods.get().len().to_string(), 22.0, ACCENT, &z).into_element(),
                detail: pix(move || t!("home.mods.detail"), 10.0, TEXT_DIM, &z).into_element(),
            }
        }
        "catalogue" => {
            let total = ui.nexus_total;
            CardParts {
                tag: "home.catalogue.tag",
                value: pix(move || total.get().to_string(), 22.0, ACCENT, &z).into_element(),
                detail: pix(move || t!("home.catalogue.detail"), 10.0, TEXT_DIM, &z).into_element(),
            }
        }
        _ => {
            let status = ui.bepinex;
            CardParts {
                tag: "home.ready.tag",
                value: pix_c(
                    move || ready_text(status.get()),
                    16.0,
                    theme_color_with(move || ready_color(status.get())),
                    &z,
                )
                .into_element(),
                detail: pix(move || t!("home.ready.detail"), 10.0, TEXT_DIM, &z).into_element(),
            }
        }
    }
}

fn ready_text(status: Option<BepInExStatus>) -> String {
    match status {
        Some(status) if status.complete => t!("home.ready.done"),
        Some(_) => t!("home.ready.incomplete"),
        None => t!("home.ready.unknown"),
    }
}

fn ready_color(status: Option<BepInExStatus>) -> ColorRole {
    match status {
        Some(status) if status.complete => ACCENT,
        Some(_) => WARN,
        None => TEXT_DIM,
    }
}
