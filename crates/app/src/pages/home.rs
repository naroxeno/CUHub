//! Hub home: game entry point, reorderable details and cached announcements.

use super::settings::game_folder_path;
use crate::app_state::Ui;
use crate::components::{button, fade_in, pix, pix_c, pixel_icon, rise_in, scaled};
use crate::palette::*;
use crate::shell::nav;
use cuprum_core::install::Status as BepInExStatus;
use nsh_base::tooltip;
use nsh_core::EllipsizeMode;
use nsh_core::prelude::*;

const TILE_IDS: [&str; 4] = ["game", "mods", "catalogue", "ready"];
const DRAG_THRESHOLD: f32 = 24.0;
const TWO_COLUMN_WIDTH: f32 = 352.0;
const GRIP: [u8; 8] = [0x00, 0x24, 0x24, 0x00, 0x24, 0x24, 0x00, 0x00];

#[derive(Clone, Copy)]
struct HomeDrag {
    order: Signal<Vec<String>>,
    dragging: Signal<Option<usize>>,
    columns: Memo<u16>,
}

#[derive(Clone, Copy)]
struct DragOffset {
    dx: f32,
    dy: f32,
    threshold: f32,
    columns: usize,
}

struct CardParts {
    tag: &'static str,
    value: AnyElement,
    page: &'static str,
    action: &'static str,
}

pub fn hub_home(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let order = signal(normalized_order(&ui.settings.get().home_order));
    let width = signal(0.0_f32);
    let columns = computed(move || {
        if width.get() >= scaled(&z, TWO_COLUMN_WIDTH)().0 {
            2
        } else {
            1
        }
    });
    let drag = HomeDrag {
        order,
        dragging: signal(None::<usize>),
        columns,
    };
    let announcement = ui.announcement;
    let tiles = ui.clone();
    view()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .bind_motion(&rise_in())
        .gap(scaled(&z, 12.0))
        .child(pix(move || nav::title(nav::HOME), 18.0, TEXT, &z))
        .child(game_panel(ui))
        .child(
            view()
                .grid()
                .w_full()
                .grid_cols(move || columns.get())
                .gap(scaled(&z, 8.0))
                .on_layout(move |bounds: &Bounds<Points>| width.set(bounds.size.width.0))
                .child(each(
                    order,
                    |id| id.clone(),
                    move |id, index| {
                        let parts = card_parts(&tiles, id.get_untracked().as_str());
                        tile(&tiles, drag, index, parts)
                    },
                )),
        )
        .child(when(
            move || {
                announcement
                    .get()
                    .is_some_and(|text| !text.trim().is_empty())
            },
            move || announcement_panel(z, announcement),
        ))
}

fn game_panel(ui: &Ui) -> View {
    let z = ui.zoom;
    let settings = ui.settings;
    let bridge = ui.bridge;
    let launch = ui.clone();
    let configure = ui.clone();
    let configured = move || !settings.get().game_path.trim().is_empty();
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(scaled(&z, 12.0))
        .p(scaled(&z, 16.0))
        .bg(theme_color(SURFACE_RAISED))
        .border(scaled(&z, 1.0))
        .border_color(theme_color(BORDER_DIM))
        .border_t(scaled(&z, 2.0))
        .border_t_color(theme_color(ACCENT))
        .child(
            view()
                .flex_col()
                .min_w(px(0.0))
                .gap(scaled(&z, 6.0))
                .child(pix("Casualties Unknown", 20.0, TEXT, &z))
                .child(pix_c(
                    move || {
                        if bridge.get().game_running {
                            t!("home.game_running")
                        } else if configured() {
                            t!("home.game_configured")
                        } else {
                            t!("notice.no_game_folder")
                        }
                    },
                    11.0,
                    theme_color_with(move || {
                        if bridge.get().game_running {
                            ACCENT
                        } else if configured() {
                            TEXT_MUTED
                        } else {
                            WARN
                        }
                    }),
                    &z,
                )),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap(scaled(&z, 8.0))
                .child(when(
                    move || configured() && !bridge.get().game_running,
                    move || {
                        let launch = launch.clone();
                        button(
                            &z,
                            move || t!("topbar.launch"),
                            true,
                            move || launch.launch_game(),
                        )
                        .bg(theme_color(ACCENT_SURFACE))
                        .border_color(theme_color(ACCENT))
                    },
                ))
                .child(when(
                    move || !configured() && !bridge.get().game_running,
                    move || {
                        page_button(&configure, nav::SETTINGS, "home.configure_game")
                            .bg(theme_color(ACCENT_SURFACE))
                            .border_color(theme_color(ACCENT))
                    },
                ))
                .child(page_button(ui, nav::MULTIPLAYER, "nav.multiplayer")),
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
    let CardParts {
        tag,
        value,
        page,
        action,
    } = parts;
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
                .child(pix(move || t!(tag), 11.0, TEXT_MUTED, &z))
                .child(
                    pixel_icon(GRIP, theme_color(TEXT_DIM), &z)
                        .cursor(Cursor::Grab)
                        .tooltip(tooltip().child(pix(
                            move || t!("home.order_hint"),
                            11.0,
                            TEXT,
                            &z,
                        ))),
                ),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap(scaled(&z, 6.0))
                .child(view().flex_1().min_w(px(0.0)).child(value))
                .child(page_button(ui, page, action)),
        );
    view()
        .flex_col()
        .min_w(px(0.0))
        .min_h(scaled(&z, 64.0))
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
    let HomeDrag {
        order,
        dragging,
        columns,
    } = drag;
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
                columns: usize::from(columns.get_untracked()),
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
    let candidate = if drag.columns > 1 && drag.dx.abs() >= drag.dy.abs() {
        if drag.dx > drag.threshold {
            from + 1
        } else if drag.dx < -drag.threshold {
            from.saturating_sub(1)
        } else {
            from
        }
    } else if drag.dy > drag.threshold {
        from + drag.columns
    } else if drag.dy < -drag.threshold {
        from.saturating_sub(drag.columns)
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

fn announcement_panel(z: Signal<u8>, announcement: Signal<Option<String>>) -> View {
    view()
        .flex_col()
        .gap(scaled(&z, 6.0))
        .p(scaled(&z, 12.0))
        .border_t(scaled(&z, 1.0))
        .border_t_color(theme_color(BORDER_DIM))
        .bind_motion(&fade_in())
        .child(pix(
            move || t!("home.announcement.tag"),
            11.0,
            TEXT_MUTED,
            &z,
        ))
        .child(pix(
            move || announcement.get().unwrap_or_default(),
            12.0,
            TEXT,
            &z,
        ))
}

fn page_button(ui: &Ui, target: &'static str, label: &'static str) -> Text {
    let page = ui.page;
    button(&ui.zoom, move || t!(label), true, move || page.set(target))
}

fn card_parts(ui: &Ui, id: &str) -> CardParts {
    let z = ui.zoom;
    match id {
        "game" => {
            let ui = ui.clone();
            let path = move || game_folder_path(&ui).unwrap_or_else(|| t!("common.not_set"));
            CardParts {
                tag: "settings.game_folder",
                value: pix(path.clone(), 12.0, TEXT, &z)
                    .number_of_lines(1)
                    .text_overflow(EllipsizeMode::Middle)
                    .tooltip(tooltip().child(pix(path, 11.0, TEXT, &z)))
                    .into_element(),
                page: nav::SETTINGS,
                action: "home.game_settings",
            }
        }
        "mods" => {
            let mods = ui.mods;
            CardParts {
                tag: "nav.local_mods",
                value: pix(move || mods.get().len().to_string(), 20.0, TEXT, &z).into_element(),
                page: nav::LOCAL,
                action: "home.manage_mods",
            }
        }
        "catalogue" => {
            let total = ui.nexus_total;
            CardParts {
                tag: "home.catalogue.tag",
                value: pix(move || total.get().to_string(), 20.0, TEXT, &z).into_element(),
                page: nav::NEXUS,
                action: "home.browse_catalogue",
            }
        }
        _ => {
            let status = ui.bepinex;
            CardParts {
                tag: "home.ready.tag",
                value: pix_c(
                    move || ready_text(status.get()),
                    14.0,
                    theme_color_with(move || ready_color(status.get())),
                    &z,
                )
                .into_element(),
                page: nav::SETTINGS,
                action: "home.component_settings",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_drag_follows_the_displayed_column_count() {
        let order = normalized_order(&[]);
        let drag = DragOffset {
            dx: 0.0,
            dy: 50.0,
            threshold: DRAG_THRESHOLD,
            columns: 1,
        };
        assert_eq!(reorder_target(&order, 0, drag), 1);
        assert_eq!(
            reorder_target(&order, 0, DragOffset { columns: 2, ..drag }),
            2
        );
        assert_eq!(
            reorder_target(
                &order,
                0,
                DragOffset {
                    dx: 50.0,
                    dy: 0.0,
                    ..drag
                }
            ),
            0
        );
    }
}
