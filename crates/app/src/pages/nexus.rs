//! Nexus catalogue layout and page-local browsing state.

mod browse;
mod card;
mod markup;

use crate::palette::*;
use crate::ui::{Ui, button, nexus_page_count, pix, rise_in, scaled};
use browse::{Browse, toolbar};
use card::{catalogue_card, detail_sheet};
use nsh_core::prelude::*;

pub fn nexus(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let browse = Browse::new(ui);
    let width = signal(0.0_f32);
    let error = ui.nexus_error;
    let results = browse.results;
    let rows = browse.rows;
    let grid_ui = ui.clone();
    let list_ui = ui.clone();
    let settings = ui.settings;
    let columns = move || ((width.get() / (185.0 * f32::from(z.get()))) as u16).clamp(1, 6);
    let covers = ui.clone();
    let page = ui.page;
    effect(move || {
        let urls = if page.get() == crate::shell::nav::NEXUS {
            rows.get()
                .iter()
                .filter_map(|row| row.cover.clone())
                .collect()
        } else {
            Vec::new()
        };
        covers.request_nexus_covers(urls);
    });
    view()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .min_w(px(0.0))
        .p(scaled(&z, 12.0))
        .gap(scaled(&z, 8.0))
        .bind_motion(&rise_in())
        .child(toolbar(ui, browse))
        .child(when(
            move || !error.get().is_empty(),
            move || pix(move || error.get(), 11.0, WARN, &z),
        ))
        .child(
            scroll()
                .flex_1()
                .min_h(px(0.0))
                .scrollbar(true)
                .element_ref(&browse.scroll)
                .child(
                    view()
                        .flex_col()
                        .min_w(px(0.0))
                        .on_layout(move |bounds: &Bounds<Points>| {
                            if width.get_untracked() != bounds.size.width.0 {
                                width.set(bounds.size.width.0);
                            }
                        })
                        .child(when(move || results.get().is_empty(), {
                            let total = ui.nexus_total;
                            move || {
                                pix(
                                    move || {
                                        if total.get() == 0 {
                                            t!("nexus.empty")
                                        } else {
                                            t!("nexus.no_results")
                                        }
                                    },
                                    12.0,
                                    TEXT_MUTED,
                                    &z,
                                )
                                .py(scaled(&z, 12.0))
                            }
                        }))
                        .child(
                            when(
                                move || settings.get().dashboard_list_view,
                                move || {
                                    let ui = list_ui.clone();
                                    view().flex_col().gap(scaled(&z, 5.0)).child(each(
                                        move || rows.get(),
                                        |row| row.id.clone(),
                                        move |row, _| catalogue_card(&ui, browse, row, true),
                                    ))
                                },
                            )
                            .otherwise(move || {
                                let ui = grid_ui.clone();
                                view()
                                    .grid()
                                    .grid_cols(columns)
                                    .gap(scaled(&z, 8.0))
                                    .child(each(
                                        move || rows.get(),
                                        |row| row.id.clone(),
                                        move |row, _| catalogue_card(&ui, browse, row, false),
                                    ))
                            }),
                        ),
                ),
        )
        .child(when(
            move || !results.get().is_empty(),
            move || pager(&z, browse),
        ))
        .child(detail_sheet(ui, browse))
}

fn pager(z: &Signal<u8>, browse: Browse) -> View {
    let z = *z;
    let current = browse.current;
    let results = browse.results;
    let mut bar = view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(scaled(&z, 6.0))
        .p(scaled(&z, 6.0))
        .pr(scaled(&z, 102.0))
        .bg(rgb(SURFACE_RAISED))
        .border_t(scaled(&z, 1.0))
        .border_t_color(rgb(BORDER_DIM));
    bar = bar.child(pager_button(&z, browse, false)).child(pix(
        move || {
            let total = results.get().len();
            t!(
                "nexus.pager",
                page = current.get() + 1,
                pages = nexus_page_count(total),
                total = total
            )
        },
        10.0,
        TEXT_MUTED,
        &z,
    ));
    bar.child(pager_button(&z, browse, true))
}

fn pager_button(z: &Signal<u8>, browse: Browse, next: bool) -> impl IntoElement {
    let z = *z;
    let key = if next {
        "common.next"
    } else {
        "common.previous"
    };
    when(
        move || {
            if next {
                browse.current.get() + 1 < nexus_page_count(browse.results.get().len())
            } else {
                browse.current.get() > 0
            }
        },
        move || {
            button(
                &z,
                move || t!(key),
                true,
                move || {
                    let page = browse.current.get();
                    browse.go_to(if next {
                        page + 1
                    } else {
                        page.saturating_sub(1)
                    });
                },
            )
        },
    )
    .otherwise(move || button(&z, move || t!(key), false, || {}))
}
