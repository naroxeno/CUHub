//! Nexus browse page: cards, pager and stats.

use super::local::status_color;
use crate::i18n;
use crate::palette::*;
use crate::ui::{NexusModRow, Ui, button, nexus_page_count, panel, pix, rise_in, scaled, spinner};
use cuhub_core::mods::ModStatus;
use nsh_core::prelude::*;

pub fn nexus(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let tracked = ui.clone();
    let updates = ui.clone();
    let synced = ui.clone();
    let api_key = ui.clone();
    let rescan = ui.clone();
    let refresh = ui.clone();
    let refreshing = ui.nexus_refreshing;
    let error = ui.nexus_error;
    let total = ui.nexus_total;
    let empty = ui.clone();
    let empty_z = z;
    let rows_ui = ui.clone();
    let list = ui.nexus_list;
    let rows = view().flex_col().element_ref(&list).child(each(
        ui.nexus_page_rows,
        |row| row.row_key(),
        move |row, _| {
            let row = row.get();
            nexus_row(&z, &row, &rows_ui)
        },
    ));
    let pager_ui = ui.clone();
    let pager_z = z;
    view()
        .flex_col()
        .flex_1()
        .bind_motion(&rise_in())
        .gap(scaled(&z, 12.0))
        .child(panel(
            &z,
            move || i18n::page_label(i18n::NEXUS_PAGE),
            view()
                .flex_col()
                .gap(scaled(&z, 10.0))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .gap(scaled(&z, 20.0))
                        .child(stat(
                            &z,
                            "nexus.stat.tracked",
                            move || tracked.nexus_total.get().to_string(),
                            TEXT,
                        ))
                        .child(stat(
                            &z,
                            "nexus.stat.updates",
                            move || {
                                updates
                                    .mods
                                    .get()
                                    .iter()
                                    .filter(|item| item.status == ModStatus::OutOfDate)
                                    .count()
                                    .to_string()
                            },
                            WARN,
                        ))
                        .child(stat(
                            &z,
                            "nexus.stat.last_sync",
                            move || synced.metadata_synced.get(),
                            TEXT_MUTED,
                        ))
                        .child(stat(
                            &z,
                            "nexus.stat.api_key",
                            move || {
                                if api_key.nexus_key_saved.get() {
                                    t!("common.saved")
                                } else {
                                    t!("common.not_set")
                                }
                            },
                            TEXT,
                        )),
                )
                .child(pix(move || t!("nexus.intro"), 12.0, TEXT_MUTED, &z))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap(scaled(&z, 8.0))
                        .child(pix(
                            move || {
                                let total = total.get();
                                if total == 1 {
                                    t!("nexus.cached_one", count = total)
                                } else {
                                    t!("nexus.cached_other", count = total)
                                }
                            },
                            11.0,
                            TEXT_DIM,
                            &z,
                        ))
                        .child(
                            when(
                                move || refreshing.get(),
                                move || {
                                    view()
                                        .flex_row()
                                        .items_center()
                                        .gap(scaled(&z, 6.0))
                                        .child(spinner(&z))
                                        .child(pix(move || t!("nexus.refreshing"), 11.0, WARN, &z))
                                },
                            )
                            .otherwise(move || {
                                let refresh = refresh.clone();
                                button(
                                    &z,
                                    move || t!("nexus.refresh_catalogue"),
                                    true,
                                    move || {
                                        refresh.refresh_metadata(true);
                                    },
                                )
                            }),
                        ),
                )
                .child(when(
                    move || !error.get().is_empty(),
                    move || pix(move || error.get(), 11.0, WARN, &z),
                ))
                .child(when(
                    move || total.get() == 0,
                    move || empty_nexus(&empty_z, &empty),
                ))
                .child(rows)
                .child(when(
                    move || total.get() > 0,
                    move || nexus_pager(&pager_ui, &pager_z),
                ))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 8.0))
                        .child(button(
                            &z,
                            move || t!("common.rescan_mods"),
                            true,
                            move || {
                                rescan.refresh_mods();
                            },
                        )),
                ),
        ))
}

fn empty_nexus(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let refresh = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .py(scaled(&z, 12.0))
        .child(pix(move || t!("nexus.empty"), 11.0, TEXT_MUTED, &z))
        .child(button(
            &z,
            move || t!("nexus.refresh_catalogue"),
            true,
            move || {
                refresh.refresh_metadata(true);
            },
        ))
}

fn nexus_pager(ui: &Ui, z: &Signal<u8>) -> View {
    let z = *z;
    let page = ui.nexus_page;
    let total = ui.nexus_total;
    let prev = ui.clone();
    let next = ui.clone();
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .pt(scaled(&z, 4.0))
        .child(pix(
            move || {
                let total = total.get();
                t!(
                    "nexus.pager",
                    page = page.get() + 1,
                    pages = nexus_page_count(total),
                    total = total
                )
            },
            10.0,
            TEXT_MUTED,
            &z,
        ))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 6.0))
                .child(
                    when(
                        move || page.get() > 0,
                        move || {
                            let prev = prev.clone();
                            button(
                                &z,
                                move || t!("common.previous"),
                                true,
                                move || {
                                    prev.set_nexus_page(page.get().saturating_sub(1));
                                },
                            )
                        },
                    )
                    .otherwise(move || button(&z, move || t!("common.previous"), false, || {})),
                )
                .child(
                    when(
                        move || page.get() + 1 < nexus_page_count(total.get()),
                        move || {
                            let next = next.clone();
                            button(
                                &z,
                                move || t!("common.next"),
                                true,
                                move || {
                                    next.set_nexus_page(page.get() + 1);
                                },
                            )
                        },
                    )
                    .otherwise(move || button(&z, move || t!("common.next"), false, || {})),
                ),
        )
}

fn nexus_row(z: &Signal<u8>, row: &NexusModRow, ui: &Ui) -> View {
    let z = *z;
    let color = row.status.map(status_color).unwrap_or(TEXT_DIM);
    let meta_base = format!("{} · {} · {}", row.author, row.version, row.downloads);
    let installed_version = row.installed_version.clone();
    let state_key = row.state_key();
    let details = ui.clone();
    let download = ui.clone();
    let open = ui.clone();
    let download_id = row.id.clone();
    let open_id = row.id.clone();
    let details_name = row.name.clone();
    let key_saved = ui.nexus_key_saved;
    let can_download = row.can_download;
    let has_page = row.has_page;
    let mut title = view()
        .flex_row()
        .items_center()
        .gap(scaled(&z, 8.0))
        .flex_1()
        .min_w(scaled(&z, 0.0))
        .child(
            view()
                .flex_1()
                .min_w(scaled(&z, 0.0))
                .child(pix(row.name.clone(), 13.0, TEXT, &z).truncate()),
        );
    if row.adult {
        title = title.child(pix("18+", 10.0, REQUEST, &z));
    }
    view()
        .flex_col()
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER_DIM))
        .border_l(scaled(&z, 2.0))
        .border_l_color(rgb(color))
        .hover(|s| s.bg(rgb(SURFACE_RAISED)).border_b_color(rgb(BORDER)))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(scaled(&z, 8.0))
                .px(scaled(&z, 6.0))
                .pt(scaled(&z, 4.0))
                .child(title)
                .child(pix(move || t!(state_key), 10.0, color, &z).flex_shrink_0()),
        )
        .child(view().px(scaled(&z, 6.0)).child(pix(
            move || match &installed_version {
                Some(version) => {
                    format!("{meta_base} · {}", t!("nexus.installed", version = version))
                }
                None => meta_base.clone(),
            },
            10.0,
            TEXT_DIM,
            &z,
        )))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 6.0))
                .px(scaled(&z, 6.0))
                .pb(scaled(&z, 4.0))
                .child(
                    when(
                        move || key_saved.get(),
                        move || {
                            let download = download.clone();
                            let download_id = download_id.clone();
                            button(
                                &z,
                                move || t!("common.download"),
                                can_download,
                                move || {
                                    download.download_nexus(download_id.clone());
                                },
                            )
                        },
                    )
                    .otherwise(move || {
                        let open = open.clone();
                        let open_id = open_id.clone();
                        button(
                            &z,
                            move || t!("nexus.open_page"),
                            has_page,
                            move || {
                                open.open_nexus_download(open_id.clone());
                            },
                        )
                    }),
                )
                .child(button(
                    &z,
                    move || t!("common.details"),
                    true,
                    move || {
                        details.show_mod_details(&details_name);
                    },
                )),
        )
}

fn stat(
    z: &Signal<u8>,
    label_key: &'static str,
    value: impl IntoReactive<SharedString>,
    color: u32,
) -> impl IntoElement {
    let z = *z;
    view()
        .flex_col()
        .gap(scaled(&z, 2.0))
        .child(pix(move || t!(label_key), 10.0, TEXT_DIM, &z))
        .child(pix(value, 24.0, color, &z))
}
