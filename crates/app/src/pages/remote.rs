//! Combined remote catalogue: filters, rows, pager and empty state.

use crate::app_state::{RemoteFilter, RemoteRow, RemoteSource, Ui, nexus_page_count};
use crate::components::{button, choice_button, panel, pix, rise_in, scaled, spinner, text_input};
use crate::palette::*;
use crate::shell::nav;
use nsh_core::prelude::*;

mod thunderstore;

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
    let has_manifest = ui.sources_ready;
    let manifest_path = ui.sources_path();
    let body = view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(scaled(&z, 8.0))
                .child(
                    text_input(&z)
                        .value(ui.remote_query)
                        .placeholder(move || t!("remote.search_placeholder"))
                        .flex_1()
                        .min_w(scaled(&z, 160.0))
                        .on_change(move |text: &str| search.set_remote_query(text.to_owned())),
                )
                .child(remote_filters(&z, ui))
                .child(remote_refresh(&z, ui))
                .child(nexus_refresh(&z, ui))
                .child(thunderstore::refresh(&z, ui)),
        )
        .child(source_filters(&z, ui))
        .child(thunderstore::refresh_error(&z, ui))
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
                match &row.thunderstore {
                    Some(package) => {
                        thunderstore::row(&list_z, package, row.thunderstore_version, &rows_ui)
                    }
                    None => remote_row(&list_z, &row, &rows_ui),
                }
            },
        ))
        .child(when(
            move || total.get() > 0,
            move || remote_pager(&z, &pager_ui),
        ));
    let body = body.child(when(
        move || !has_manifest.get(),
        move || remote_manifest_hint(&z, &manifest_path),
    ));
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
    let apply = ui.clone();
    let current = ui.remote_filter;
    choice_button(
        z,
        move || t!(label_key),
        move || current.get() == filter,
        move || apply.set_remote_filter(filter),
    )
}

fn source_filters(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let current = ui.remote_source;
    let mut row = view().flex_row().flex_wrap().gap(scaled(&z, 6.0));
    for (source, key) in [
        (RemoteSource::All, "remote.all_sources"),
        (RemoteSource::Github, "remote.github_source"),
        (RemoteSource::Nexus, "remote.nexus_source"),
        (RemoteSource::Thunderstore, "remote.thunderstore_source"),
    ] {
        let ui = ui.clone();
        row = row.child(choice_button(
            &z,
            move || t!(key),
            move || current.get() == source,
            move || ui.set_remote_source(source),
        ));
    }
    row
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
    let mut content =
        view()
            .flex_col()
            .gap(scaled(&z, 4.0))
            .child(pix(row.name().to_owned(), 12.0, TEXT, &z));
    if installed {
        content = content.child(pix(
            move || match &version {
                Some(version) => t!("nexus.installed", version = version),
                None => t!("remote.installed_tag"),
            },
            10.0,
            TEXT_DIM,
            &z,
        ));
    }
    if let Some(entry) = &row.github {
        let ui = ui.clone();
        let check = ui.clone();
        let row = row.clone();
        let repo = entry.repo.clone();
        let slug = repo.slug();
        let versions = ui.remote_versions;
        let label = if installed {
            "remote.update"
        } else {
            "remote.install"
        };
        content = content.child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(scaled(&z, 6.0))
                .child(pix(
                    move || {
                        let versions = versions.get();
                        let version = versions
                            .get(&slug)
                            .and_then(|tag| tag.clone())
                            .unwrap_or_else(|| t!("nexus.unknown_version"));
                        format!("GitHub · {slug} · {version}")
                    },
                    10.0,
                    TEXT_MUTED,
                    &z,
                ))
                .child(button(
                    &z,
                    move || t!("remote.check_version"),
                    true,
                    move || check.check_remote_release(repo.clone()),
                ))
                .child(
                    when(
                        move || busy.get().is_some() || pending.get().is_some(),
                        move || button(&z, move || t!(label), false, || {}),
                    )
                    .otherwise(move || {
                        let ui = ui.clone();
                        let row = row.clone();
                        button(
                            &z,
                            move || t!(label),
                            true,
                            move || ui.run_remote(row.clone()),
                        )
                    }),
                ),
        );
    }
    if let Some(entry) = &row.nexus {
        let mut source = view()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(scaled(&z, 6.0))
            .child(pix(
                format!("Nexus · {}", entry.version),
                10.0,
                TEXT_MUTED,
                &z,
            ));
        if let Some(url) = &entry.page_url {
            let url = url.clone();
            let open = ui.clone();
            source = source.child(button(
                &z,
                move || t!("remote.nexus_details"),
                true,
                move || open.open_link(&url),
            ));
            let id = entry.id.clone();
            let open = ui.clone();
            source = source.child(button(
                &z,
                move || t!("nexus.open_files"),
                true,
                move || open.open_nexus_download(id.clone()),
            ));
        }
        if entry.download_id.is_some() {
            let key = ui.nexus_key_saved;
            let id = entry.id.clone();
            let ui = ui.clone();
            source = source.child(when(
                move || key.get(),
                move || {
                    let ui = ui.clone();
                    let id = id.clone();
                    when(
                        move || busy.get().is_some() || pending.get().is_some(),
                        move || button(&z, move || t!("remote.prepare_nexus"), false, || {}),
                    )
                    .otherwise(move || {
                        let ui = ui.clone();
                        let id = id.clone();
                        button(
                            &z,
                            move || t!("remote.prepare_nexus"),
                            true,
                            move || ui.prepare_catalogue_nexus(id.clone()),
                        )
                    })
                },
            ));
        }
        content = content.child(source);
    }
    content
        .px(scaled(&z, 6.0))
        .py(scaled(&z, 6.0))
        .border_b(scaled(&z, 1.0))
        .border_b_color(theme_color(BORDER_DIM))
        .border_l(scaled(&z, 2.0))
        .border_l_color(theme_color(if installed { ACCENT } else { SURFACE }))
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

fn nexus_refresh(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let busy = ui.nexus_refreshing;
    let ui = ui.clone();
    view().child(
        when(
            move || busy.get(),
            move || button(&z, move || t!("remote.refreshing"), false, || {}),
        )
        .otherwise(move || {
            let ui = ui.clone();
            button(
                &z,
                move || t!("remote.refresh_nexus"),
                true,
                move || ui.refresh_metadata(true),
            )
        }),
    )
}

fn remote_manifest_hint(z: &Signal<u8>, path: &str) -> View {
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
}
