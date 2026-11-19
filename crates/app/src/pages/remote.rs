//! Remote (Nexus) list page: filters, rows, pager and empty state.

use crate::palette::*;
use crate::shell::nav;
use crate::ui::{
    RemoteFilter, RemoteRow, Ui, button, nexus_page_count, panel, pix, pix_c, rise_in, scaled,
    spinner,
};
use nsh_base::input;
use nsh_core::prelude::*;

pub fn remote_mods(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let count = ui.clone();
    let search = ui.clone();
    let list_z = z;
    let rows_ui = ui.clone();
    let pager_ui = ui.clone();
    let rows = ui.remote_rows;
    let total = ui.remote_total;
    let empty_z = z;
    let empty_ui = ui.clone();
    let has_manifest = ui.sources_ready.get();
    let manifest_path = ui.sources_path();
    let mut body = view().flex_col().gap(scaled(&z, 8.0));
    if has_manifest {
        body = body
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(scaled(&z, 8.0))
                    .child(
                        input()
                            .value(ui.remote_query)
                            .placeholder(move || t!("remote.search_placeholder"))
                            .font_family(crate::ui::PIXEL_FONT.family)
                            .text_size(scaled(&z, 12.0))
                            .h(scaled(&z, 24.0))
                            .flex_1()
                            .min_w(scaled(&z, 160.0))
                            .px(scaled(&z, 6.0))
                            .bg(rgb(SURFACE_RAISED))
                            .border(scaled(&z, 1.0))
                            .border_color(rgb(BORDER))
                            .text_color(rgb(TEXT))
                            .on_change(move |text: &str| search.set_remote_query(text.to_owned())),
                    )
                    .child(remote_filters(&z, ui))
                    .child(remote_refresh(&z, ui)),
            )
            .child(pix(
                move || t!("remote.total", count = count.remote_total.get()),
                11.0,
                TEXT_MUTED,
                &z,
            ))
            .child(when(
                move || total.get() == 0,
                move || remote_empty(&empty_z, &empty_ui),
            ))
            .child(each(
                rows,
                |row| row.row_key(),
                move |row, _| {
                    let row = row.get();
                    remote_row(&list_z, &row, &rows_ui)
                },
            ))
            .child(when(
                move || total.get() > 0,
                move || remote_pager(&z, &pager_ui),
            ));
    } else {
        body = body.child(remote_manifest_hint(&z, &manifest_path, ui));
    }
    view()
        .flex_col()
        .flex_1()
        .bind_motion(&rise_in())
        .child(panel(&z, move || nav::title(nav::REMOTE), body))
}

fn remote_filters(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let mut row = view()
        .flex_row()
        .items_center()
        .flex_wrap()
        .gap(scaled(&z, 6.0));
    for (filter, key) in [
        (RemoteFilter::All, "remote.filter_all"),
        (RemoteFilter::Installed, "remote.filter_installed"),
        (RemoteFilter::Missing, "remote.filter_missing"),
    ] {
        row = row.child(remote_filter_button(&z, ui, filter, key));
    }
    row
}

fn remote_filter_button(
    z: &Signal<u8>,
    ui: &Ui,
    filter: RemoteFilter,
    label_key: &'static str,
) -> Text {
    let z = *z;
    let apply = ui.clone();
    let current = ui.remote_filter;
    pix_c(
        move || t!(label_key),
        11.0,
        move || {
            if current.get() == filter {
                rgb(ACCENT)
            } else {
                rgb(TEXT_MUTED)
            }
        },
        &z,
    )
    .px(scaled(&z, 8.0))
    .py(scaled(&z, 3.0))
    .bg(move || {
        if current.get() == filter {
            rgb(SURFACE_RAISED)
        } else {
            rgb(SURFACE)
        }
    })
    .border(scaled(&z, 1.0))
    .border_color(move || {
        if current.get() == filter {
            rgb(ACCENT)
        } else {
            rgb(BORDER_DIM)
        }
    })
    .hover(|s| s.bg(rgb(SURFACE_RAISED)))
    .active(|s| s.bg(rgb(BORDER_DIM)))
    .on_press(move || apply.set_remote_filter(filter))
    .flex_shrink_0()
}

fn remote_row(z: &Signal<u8>, row: &RemoteRow, ui: &Ui) -> View {
    let z = *z;
    let installed = row.is_installed();
    let version = row
        .installed
        .as_ref()
        .and_then(|item| item.installed_version.clone());
    let busy = ui.remote_busy;
    let pending = ui.pending_install;
    let action = ui.clone();
    let target = row.clone();
    let label_key = if installed {
        "remote.update"
    } else {
        "remote.install"
    };
    let mut meta_line = view()
        .flex_row()
        .items_center()
        .gap(scaled(&z, 8.0))
        .flex_shrink_0();
    if installed {
        meta_line = meta_line.child(pix(
            move || match &version {
                Some(version) => t!("nexus.installed", version = version.clone()),
                None => t!("remote.installed_tag"),
            },
            10.0,
            TEXT_DIM,
            &z,
        ));
    }
    view()
        .flex_col()
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER_DIM))
        .border_l(scaled(&z, 2.0))
        .border_l_color(rgb(if installed { ACCENT } else { SURFACE }))
        .hover(|s| s.bg(rgb(SURFACE_RAISED)).border_b_color(rgb(BORDER)))
        .child(
            view()
                .flex_row()
                .items_center()
                .px(scaled(&z, 6.0))
                .py(scaled(&z, 4.0))
                .gap(scaled(&z, 8.0))
                .child(
                    view()
                        .flex_col()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(pix(row.entry.name.clone(), 12.0, TEXT, &z).truncate())
                        .child(pix(row.repo(), 10.0, TEXT_MUTED, &z).truncate()),
                )
                .child(meta_line)
                .child(
                    when(
                        move || busy.get().is_some() || pending.get().is_some(),
                        move || button(&z, move || t!(label_key), false, || {}),
                    )
                    .otherwise(move || {
                        let action = action.clone();
                        let target = target.clone();
                        button(
                            &z,
                            move || t!(label_key),
                            true,
                            move || {
                                action.run_remote(target.clone());
                            },
                        )
                    }),
                ),
        )
}

fn remote_pager(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let page = ui.remote_page;
    let total = ui.remote_total;
    let prev = ui.clone();
    let next = ui.clone();
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .pt(scaled(&z, 4.0))
        .child(pix(
            move || {
                t!(
                    "remote.pager",
                    page = page.get() + 1,
                    pages = nexus_page_count(total.get()),
                    total = total.get()
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
                                move || prev.set_remote_page(page.get().saturating_sub(1)),
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
                                move || next.set_remote_page(page.get() + 1),
                            )
                        },
                    )
                    .otherwise(move || button(&z, move || t!("common.next"), false, || {})),
                ),
        )
}

fn remote_empty(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let all = ui.remote_all;
    view().py(scaled(&z, 10.0)).child(pix(
        move || {
            if all.get().is_empty() {
                t!("remote.empty")
            } else {
                t!("remote.empty_filtered")
            }
        },
        12.0,
        TEXT_MUTED,
        &z,
    ))
}

fn remote_refresh(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let refresh = ui.clone();
    let refreshing = ui.sources_refreshing;
    view().child(
        when(
            move || refreshing.get(),
            move || {
                view()
                    .flex_row()
                    .items_center()
                    .gap(scaled(&z, 6.0))
                    .child(spinner(&z))
                    .child(pix(move || t!("remote.refreshing"), 11.0, WARN, &z))
            },
        )
        .otherwise(move || {
            let refresh = refresh.clone();
            button(
                &z,
                move || t!("remote.refresh"),
                true,
                move || refresh.refresh_sources_now(),
            )
        }),
    )
}

fn remote_manifest_hint(z: &Signal<u8>, path: &str, ui: &Ui) -> View {
    let z = *z;
    let path = path.to_owned();
    view()
        .flex_col()
        .gap(scaled(&z, 6.0))
        .py(scaled(&z, 10.0))
        .child(pix(
            move || t!("remote.empty_no_manifest"),
            12.0,
            TEXT_MUTED,
            &z,
        ))
        .child(pix(
            move || t!("remote.manifest_hint", path = path.clone()),
            10.0,
            TEXT_DIM,
            &z,
        ))
        .child(remote_refresh(&z, ui))
}
