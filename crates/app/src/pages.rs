//! The six pages behind the shell navigation; data comes from the Hub facade.

use cuhub_core::mods::ModStatus;
use cuhub_core::nexus_auth::{GithubTokenStore, NexusKeyStore};
use nsh_base::input;
use nsh_core::prelude::*;

use crate::i18n;
use crate::palette::*;
use crate::ui::{
    LocalModRow, NexusModRow, Ui, button, danger_button, fade_in, nexus_page_count, panel, pix,
    pix_c, rise_in, scaled, spinner,
};

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
            move || t!("panel.nexus_dashboard"),
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

fn pending_panel(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let Some(pending) = ui.pending_install.get() else {
        return view();
    };
    let replaced = pending
        .plan
        .files
        .iter()
        .filter(|file| file.action == cuhub_core::install::FileAction::Replace)
        .count();
    let created = pending.plan.files.len().saturating_sub(replaced);
    let mod_name = pending.mod_name.clone();
    let file_count = pending.plan.files.len();
    let from_source = match pending.source {
        cuhub_core::sources::SourceKind::Github => "GitHub",
        cuhub_core::sources::SourceKind::Nexus => "Nexus",
    };
    let target_root = pending.plan.target_root.display().to_string();
    let mut notes = pending.notes.clone();
    notes.extend(pending.plan.warnings.clone());
    let mut body = view()
        .flex_col()
        .gap(scaled(&z, 4.0))
        .child(pix(
            move || {
                t!(
                    "pending.summary",
                    name = mod_name,
                    files = file_count,
                    replaced = replaced,
                    created = created
                )
            },
            12.0,
            TEXT,
            &z,
        ))
        .child(pix(
            move || t!("pending.from", source = from_source, target = target_root),
            10.0,
            TEXT_MUTED,
            &z,
        ));
    if !notes.is_empty() {
        body = body.child(pix(notes.join(" "), 10.0, WARN, &z));
    }
    if !pending.plan.conflicts.is_empty() {
        let list = pending
            .plan
            .conflicts
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        body = body.child(pix(
            move || t!("pending.conflicts", list = list),
            10.0,
            DANGER,
            &z,
        ));
    }
    if pending.game_running {
        body = body.child(pix(move || t!("pending.game_running"), 10.0, WARN, &z));
    }
    let status_ui = ui.clone();
    body = body.child(pix(
        move || status_ui.apply_status.get(),
        10.0,
        TEXT_MUTED,
        &z,
    ));
    let apply_ui = ui.clone();
    let cancel_ui = ui.clone();
    let can_apply = pending.plan.can_apply();
    body = body.child(
        view()
            .flex_row()
            .gap(scaled(&z, 8.0))
            .child(button(
                &z,
                move || t!("common.apply"),
                can_apply,
                move || {
                    apply_ui.apply_pending();
                },
            ))
            .child(button(
                &z,
                move || t!("common.cancel"),
                true,
                move || {
                    cancel_ui.discard_pending();
                },
            )),
    );
    panel(&z, move || t!("panel.pending_update"), body).bind_motion(&rise_in())
}

pub fn local_mods(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let summary = ui.clone();
    let empty = ui.clone();
    let empty_ui = ui.clone();
    let empty_z = z;
    let pending = ui.pending_install;
    let pending_z = z;
    let pending_ui = ui.clone();
    let refresh = ui.clone();
    let bulk_state = ui.clone();
    let bulk_locked = ui.clone();
    let bulk_free = ui.clone();
    let bulk_z = z;
    let refresh_watch = ui.clone();
    let list_z = z;
    let rows_ui = ui.clone();
    let rows = each(
        ui.local_rows,
        |row| row.row_key(),
        move |row, _| {
            let row = row.get();
            mod_row(&list_z, &row, &rows_ui)
        },
    );
    view().flex_col().flex_1().bind_motion(&rise_in()).child(
        panel(
            &z,
            move || t!("panel.local_mods"),
            view()
                .flex_col()
                .gap(scaled(&z, 8.0))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .justify_between()
                        .gap(scaled(&z, 8.0))
                        .child(pix(
                            move || t!("local.count", count = summary.mods.get().len()),
                            11.0,
                            TEXT_MUTED,
                            &z,
                        ))
                        .child(
                            view()
                                .flex_row()
                                .flex_wrap()
                                .items_center()
                                .gap(scaled(&z, 8.0))
                                .child(
                                    when(
                                        move || bulk_state.mod_files_busy(),
                                        move || mod_bulk_buttons(&bulk_z, &bulk_locked, false),
                                    )
                                    .otherwise(move || mod_bulk_buttons(&bulk_z, &bulk_free, true)),
                                )
                                .child(
                                    when(
                                        move || refresh_watch.mod_files_busy(),
                                        move || {
                                            button(&z, move || t!("common.refresh"), false, || {})
                                        },
                                    )
                                    .otherwise({
                                        let refresh = refresh.clone();
                                        move || {
                                            let refresh = refresh.clone();
                                            button(
                                                &z,
                                                move || t!("common.refresh"),
                                                true,
                                                move || {
                                                    refresh.refresh_mods();
                                                },
                                            )
                                        }
                                    }),
                                ),
                        ),
                )
                .child(when(
                    move || empty.mods.get().is_empty(),
                    move || empty_state(&empty_z, &empty_ui),
                ))
                .child(when(
                    move || pending.get().is_some(),
                    move || pending_panel(&pending_z, &pending_ui),
                ))
                .child(table_header(&z))
                .child(rows),
        )
        .flex_1(),
    )
}

fn mod_bulk_buttons(z: &Signal<u8>, ui: &Ui, enabled: bool) -> View {
    let z = *z;
    let disable_ui = ui.clone();
    let enable_ui = ui.clone();
    let delete_ui = ui.clone();
    view()
        .flex_row()
        .items_center()
        .gap(scaled(&z, 8.0))
        .child(button(
            &z,
            move || t!("local.disable_all"),
            enabled,
            move || {
                disable_ui.set_all_mods_enabled(false);
            },
        ))
        .child(button(
            &z,
            move || t!("local.enable_all"),
            enabled,
            move || {
                enable_ui.set_all_mods_enabled(true);
            },
        ))
        .child(danger_button(
            &z,
            move || t!("common.delete_all"),
            enabled,
            move || {
                delete_ui.request_delete_all();
            },
        ))
}

fn empty_state(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let ui = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .py(scaled(&z, 12.0))
        .child(pix(move || t!("local.empty"), 12.0, TEXT_MUTED, &z))
        .child(button(
            &z,
            move || t!("common.detect_game"),
            true,
            move || {
                ui.detect_game();
            },
        ))
}

fn mod_row(z: &Signal<u8>, row: &LocalModRow, ui: &Ui) -> View {
    let z = *z;
    let item = &row.item;
    let disabled = item.disabled;
    let name_color = if disabled { DANGER } else { TEXT };
    let meta_color = if disabled { DANGER } else { TEXT_MUTED };
    let detail_color = if disabled { DANGER } else { TEXT_DIM };
    let update_ready = row.can_update();
    let status_key = if update_ready && item.status != ModStatus::OutOfDate {
        "status.update_available"
    } else {
        i18n::status_key(item.status)
    };
    let color = if update_ready && item.status != ModStatus::OutOfDate {
        WARN
    } else {
        status_color(item.status)
    };
    let mut state = pix(move || t!(status_key), 11.0, color, &z);
    if update_ready {
        state = state.animation(
            KeyframeAnimation::new(Keyframes::Pulse)
                .duration(1800.0)
                .frame_rate(10.0),
        );
    }
    let detail_key = i18n::status_detail_key(item.status);
    let missing = item.missing_dependencies.join(", ");
    let conflicts = item.incompatible_with.join(", ");
    let has_missing = !item.missing_dependencies.is_empty();
    let has_conflicts = !item.incompatible_with.is_empty();
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
                .h(scaled(&z, 24.0))
                .px(scaled(&z, 6.0))
                .child(view().w(percentage(0.38)).child(pix(
                    item.name.clone(),
                    12.0,
                    name_color,
                    &z,
                )))
                .child(view().w(percentage(0.18)).child(pix(
                    item.version_label(),
                    11.0,
                    meta_color,
                    &z,
                )))
                .child(view().w(percentage(0.24)).child(pix(
                    item.author_label(),
                    11.0,
                    meta_color,
                    &z,
                )))
                .child(view().w(percentage(0.20)).child(state)),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 12.0))
                .px(scaled(&z, 6.0))
                .pb(scaled(&z, 3.0))
                .child(pix(item.evidence_label(), 10.0, detail_color, &z).flex_shrink_0())
                .child(
                    view().flex_1().min_w(px(0.0)).child(
                        pix(
                            move || {
                                let mut text = t!(detail_key);
                                if has_missing {
                                    text.push_str(&t!("status_detail.missing", list = missing));
                                }
                                if has_conflicts {
                                    text.push_str(&t!("status_detail.conflicts", list = conflicts));
                                }
                                text
                            },
                            10.0,
                            detail_color,
                            &z,
                        )
                        .truncate(),
                    ),
                ),
        )
        .child(source_line(&z, row, ui))
}

fn source_line(z: &Signal<u8>, row: &LocalModRow, ui: &Ui) -> View {
    let mut line = view()
        .flex_row()
        .items_center()
        .gap(scaled(z, 8.0))
        .px(scaled(z, 6.0))
        .pb(scaled(z, 3.0));
    if !row.source.text.is_empty() {
        let color = if row.source.warning {
            WARN
        } else if row.item.disabled {
            DANGER
        } else {
            TEXT_MUTED
        };
        line = line.child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .child(pix(row.source.text.clone(), 10.0, color, z).truncate()),
        );
    }
    if !row.source.can_check && row.source.repo.is_some() {
        line = line.child(spinner(z));
    }
    if row.source.can_pin() {
        let pin_ui = ui.clone();
        let key = row.source_key.clone();
        let options = row.source.options.clone();
        let pin_key = row.source.pin_key();
        line = line.child(button(
            z,
            move || t!(pin_key),
            true,
            move || {
                pin_ui.cycle_source_pin(key.clone(), options.clone());
            },
        ));
    }
    if row.source.can_check
        && let Some(repo) = row.source.repo.clone()
    {
        let check_ui = ui.clone();
        let key = row.source_key.clone();
        line = line.child(button(
            z,
            move || t!("common.check"),
            true,
            move || {
                check_ui.check_source(key.clone(), repo.clone());
            },
        ));
    }
    if row.can_update() {
        let update_ui = ui.clone();
        let key = row.source_key.clone();
        line = line.child(button(
            z,
            move || t!("common.update"),
            true,
            move || {
                update_ui.update_source(key.clone());
            },
        ));
    }
    let toggle_ui = ui.clone();
    let toggle_key = row.source_key.clone();
    let enable = row.item.disabled;
    let toggle_label_key = if enable {
        "common.enable"
    } else {
        "common.disable"
    };
    let toggle_z = *z;
    let toggle_watch = ui.clone();
    line = line.child(
        when(
            move || toggle_watch.mod_files_busy(),
            move || button(&toggle_z, move || t!(toggle_label_key), false, || {}),
        )
        .otherwise({
            let toggle_ui = toggle_ui.clone();
            move || {
                let toggle_ui = toggle_ui.clone();
                let toggle_key = toggle_key.clone();
                button(
                    &toggle_z,
                    move || t!(toggle_label_key),
                    true,
                    move || {
                        toggle_ui.set_mod_enabled(toggle_key.clone(), enable);
                    },
                )
            }
        }),
    );
    let delete_watch = ui.clone();
    let delete_z = *z;
    line = line.child(
        when(
            move || delete_watch.mod_files_busy(),
            move || danger_button(&delete_z, move || t!("common.delete"), false, || {}),
        )
        .otherwise({
            let delete_ui = ui.clone();
            let delete_key = row.source_key.clone();
            move || {
                let delete_ui = delete_ui.clone();
                let delete_key = delete_key.clone();
                danger_button(
                    &delete_z,
                    move || t!("common.delete"),
                    true,
                    move || {
                        delete_ui.request_delete_mod(delete_key.clone());
                    },
                )
            }
        }),
    );
    line
}

fn status_color(status: ModStatus) -> u32 {
    match status {
        ModStatus::Disabled => DANGER,
        ModStatus::OutOfDate => WARN,
        ModStatus::UpToDate => ACCENT,
        ModStatus::Unlisted | ModStatus::Unknown => TEXT_MUTED,
    }
}

fn table_header(z: &Signal<u8>) -> impl IntoElement {
    let z = *z;
    view()
        .flex_row()
        .items_center()
        .h(scaled(&z, 20.0))
        .px(scaled(&z, 6.0))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER))
        .child(view().w(percentage(0.38)).child(pix(
            move || t!("local.table.name"),
            10.0,
            TEXT_MUTED,
            &z,
        )))
        .child(view().w(percentage(0.18)).child(pix(
            move || t!("local.table.version"),
            10.0,
            TEXT_MUTED,
            &z,
        )))
        .child(view().w(percentage(0.24)).child(pix(
            move || t!("local.table.author"),
            10.0,
            TEXT_MUTED,
            &z,
        )))
        .child(view().w(percentage(0.20)).child(pix(
            move || t!("local.table.state"),
            10.0,
            TEXT_MUTED,
            &z,
        )))
}

pub fn delete_overlay(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let prompt = ui.delete_prompt;
    let busy = ui.delete_busy;
    let confirm_ui = ui.clone();
    let cancel_ui = ui.clone();
    when(
        move || prompt.get().is_some(),
        move || {
            let Some(prompt) = prompt.get() else {
                return view();
            };
            let title_key = prompt.title_key();
            let title_name = prompt.target.label();
            let action_key = prompt.action_key();
            let note_key = prompt.scope_note_key();
            let files = prompt.scope.files;
            let root_count = prompt.scope.roots.len();
            let mut list = view().flex_col();
            for root in &prompt.scope.roots {
                list = list.child(pix(root.display().to_string(), 10.0, TEXT_MUTED, &z).w_full());
            }
            let veil = fade_in();
            let card = rise_in();
            let confirm_ui = confirm_ui.clone();
            let cancel_ui = cancel_ui.clone();
            let confirm = when(
                move || busy.get(),
                move || danger_button(&z, move || t!("delete.deleting"), false, || {}),
            )
            .otherwise({
                let confirm_ui = confirm_ui.clone();
                move || {
                    let confirm_ui = confirm_ui.clone();
                    danger_button(
                        &z,
                        move || t!(action_key),
                        true,
                        move || confirm_ui.confirm_delete(),
                    )
                }
            });
            let cancel = when(
                move || busy.get(),
                move || button(&z, move || t!("common.cancel"), false, || {}),
            )
            .otherwise({
                let cancel_ui = cancel_ui.clone();
                move || {
                    let cancel_ui = cancel_ui.clone();
                    button(
                        &z,
                        move || t!("common.cancel"),
                        true,
                        move || cancel_ui.cancel_delete(),
                    )
                }
            });
            let body = view()
                .flex_col()
                .gap(scaled(&z, 8.0))
                .child(pix(
                    move || t!("delete.summary", files = files, paths = root_count),
                    12.0,
                    TEXT,
                    &z,
                ))
                .child(pix(move || t!(note_key), 10.0, TEXT_MUTED, &z))
                .child(pix(move || t!("delete.irreversible"), 10.0, DANGER, &z))
                .child(scroll().w_full().max_h(scaled(&z, 132.0)).child(list))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 8.0))
                        .child(cancel)
                        .child(confirm),
                );
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
                    panel(&z, move || t!(title_key, name = title_name), body)
                        .w(scaled(&z, 460.0))
                        .max_w(percentage(0.9))
                        .bind_motion(&card),
                )
        },
    )
}

pub fn multiplayer(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    view()
        .flex_col()
        .flex_1()
        .bind_motion(&rise_in())
        .child(panel(
            &z,
            move || t!("panel.multiplayer"),
            view()
                .flex_col()
                .gap(scaled(&z, 10.0))
                .child(pix(move || t!("multiplayer.intro"), 12.0, TEXT_MUTED, &z))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 10.0))
                        .child(pix(
                            move || t!("multiplayer.share_code"),
                            11.0,
                            TEXT_DIM,
                            &z,
                        ))
                        .child(pix("CUHB-0000-0000", 14.0, REQUEST, &z)),
                )
                .child(
                    view()
                        .flex_row()
                        .gap(scaled(&z, 8.0))
                        .child(button(&z, move || t!("multiplayer.host"), false, || {}))
                        .child(button(&z, move || t!("multiplayer.join"), false, || {})),
                )
                .child(pix(move || t!("multiplayer.not_wired"), 11.0, TEXT_DIM, &z)),
        ))
}

pub fn skins_backups(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    view()
        .flex_row()
        .flex_1()
        .bind_motion(&rise_in())
        .gap(scaled(&z, 12.0))
        .child(
            panel(
                &z,
                move || t!("panel.skins"),
                view()
                    .flex_col()
                    .gap(scaled(&z, 10.0))
                    .child(pix(move || t!("skins.intro"), 12.0, TEXT_MUTED, &z))
                    .child(button(&z, move || t!("skins.import"), false, || {})),
            )
            .flex_1(),
        )
        .child(
            panel(
                &z,
                move || t!("panel.backups"),
                view()
                    .flex_col()
                    .gap(scaled(&z, 10.0))
                    .child(pix(move || t!("backups.intro"), 12.0, TEXT_MUTED, &z))
                    .child(button(&z, move || t!("backups.create"), false, || {})),
            )
            .flex_1(),
        )
}

pub fn hub_home(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let folder = ui.clone();
    let data = ui.clone();
    let mods = ui.clone();
    let catalogue = ui.clone();
    view()
        .flex_col()
        .flex_1()
        .bind_motion(&rise_in())
        .child(panel(
            &z,
            move || t!("panel.hub_home"),
            view()
                .flex_col()
                .gap(scaled(&z, 10.0))
                .child(pix(move || t!("hub.intro"), 12.0, TEXT_MUTED, &z))
                .child(row(
                    &z,
                    "hub.featured",
                    move || t!("hub.not_loaded"),
                    TEXT_DIM,
                ))
                .child(setting_row(&z, "settings.game_folder", move || {
                    game_folder_path(&folder).unwrap_or_else(|| t!("common.not_set"))
                }))
                .child(setting_row(&z, "hub.installed_mods", move || {
                    mods.mods.get().len().to_string()
                }))
                .child(setting_row(&z, "hub.catalogue_entries", move || {
                    catalogue.nexus_total.get().to_string()
                }))
                .child(setting_row(&z, "hub.local_data", move || {
                    data.app_data.clone()
                })),
        ))
}

fn game_folder_path(ui: &Ui) -> Option<String> {
    let path = ui.settings.get().game_path;
    if path.trim().is_empty() {
        None
    } else {
        Some(path)
    }
}

fn row(
    z: &Signal<u8>,
    label_key: &'static str,
    value: impl IntoReactive<SharedString>,
    color: u32,
) -> impl IntoElement {
    let z = *z;
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .h(scaled(&z, 24.0))
        .px(scaled(&z, 6.0))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER_DIM))
        .child(pix(move || t!(label_key), 12.0, TEXT_MUTED, &z))
        .child(pix(value, 12.0, color, &z))
}

fn setting_row(
    z: &Signal<u8>,
    label_key: &'static str,
    value: impl IntoReactive<SharedString>,
) -> impl IntoElement {
    row(z, label_key, value, TEXT)
}

pub fn settings(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let folder = ui.clone();
    let detect = ui.clone();
    let rescan = ui.clone();
    let path = ui.clone();
    let key_status = ui.clone();
    let save_key = ui.clone();
    let clear_key = ui.clone();
    let check_key = ui.clone();
    let draft = ui.nexus_key_draft;
    let token_status = ui.clone();
    let save_token = ui.clone();
    let clear_token = ui.clone();
    let check_token = ui.clone();
    let token_draft = ui.github_token_draft;
    view()
        .flex_col()
        .flex_1()
        .bind_motion(&rise_in())
        .child(panel(
            &z,
            move || t!("panel.settings"),
            view()
                .flex_col()
                .gap(scaled(&z, 10.0))
                .child(pix(
                    move || t!("settings.game_folder"),
                    12.0,
                    TEXT_MUTED,
                    &z,
                ))
                .child(setting_row(&z, "settings.detected_folder", move || {
                    game_folder_path(&folder).unwrap_or_else(|| t!("common.not_set"))
                }))
                .child(
                    view()
                        .flex_row()
                        .gap(scaled(&z, 8.0))
                        .child(button(
                            &z,
                            move || t!("common.detect_game"),
                            true,
                            move || {
                                detect.detect_game();
                            },
                        ))
                        .child(button(
                            &z,
                            move || t!("common.rescan_mods"),
                            true,
                            move || {
                                rescan.refresh_mods();
                            },
                        )),
                )
                .child(pix(move || t!("settings.detect_note"), 11.0, TEXT_DIM, &z))
                .child(pix(
                    move || t!("settings.interface_scale"),
                    12.0,
                    TEXT_MUTED,
                    &z,
                ))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .gap(scaled(&z, 8.0))
                        .child(zoom_button(ui, 1))
                        .child(zoom_button(ui, 2))
                        .child(zoom_button(ui, 3)),
                )
                .child(pix(move || t!("settings.scale_note"), 11.0, TEXT_DIM, &z))
                .child(pix(move || t!("settings.theme"), 12.0, TEXT_MUTED, &z))
                .child(pix(move || t!("settings.theme_pixel"), 12.0, TEXT, &z))
                .child(pix(move || t!("settings.language"), 12.0, TEXT_MUTED, &z))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .gap(scaled(&z, 8.0))
                        .child(language_button(ui, 0, &z))
                        .child(language_button(ui, 1, &z))
                        .child(language_button(ui, 2, &z))
                        .child(language_button(ui, 3, &z)),
                )
                .child(pix(
                    move || t!("settings.language_note"),
                    11.0,
                    TEXT_DIM,
                    &z,
                ))
                .child(pix(move || t!("settings.storage"), 12.0, TEXT_MUTED, &z))
                .child(setting_row(&z, "settings.data_folder", move || {
                    path.app_data.clone()
                }))
                .child(pix(move || t!("settings.nexus_key"), 12.0, TEXT_MUTED, &z))
                .child(pix(
                    move || {
                        t!(
                            "settings.nexus_key_note",
                            note = NexusKeyStore::protection_note()
                        )
                    },
                    11.0,
                    TEXT_DIM,
                    &z,
                ))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 8.0))
                        .child(
                            input()
                                .value(ui.nexus_key_draft)
                                .masked(true)
                                .placeholder(move || t!("settings.nexus_key_placeholder"))
                                .h(scaled(&z, 24.0))
                                .flex_1()
                                .px(scaled(&z, 6.0))
                                .bg(rgb(SURFACE_RAISED))
                                .border(scaled(&z, 1.0))
                                .border_color(rgb(BORDER))
                                .text_color(rgb(TEXT))
                                .on_change(move |text: &str| draft.set(text.to_owned())),
                        )
                        .child(button(
                            &z,
                            move || t!("settings.save_key"),
                            true,
                            move || {
                                save_key.save_nexus_key();
                            },
                        ))
                        .child(button(
                            &z,
                            move || t!("settings.test_key"),
                            true,
                            move || {
                                check_key.validate_nexus_key();
                            },
                        ))
                        .child(button(
                            &z,
                            move || t!("settings.clear_key"),
                            true,
                            move || {
                                clear_key.clear_nexus_key();
                            },
                        )),
                )
                .child(pix(
                    move || key_status.nexus_key_check.get(),
                    11.0,
                    TEXT_DIM,
                    &z,
                ))
                .child(pix(move || t!("settings.github_key"), 12.0, TEXT_MUTED, &z))
                .child(pix(
                    move || {
                        t!(
                            "settings.github_key_note",
                            note = GithubTokenStore::protection_note()
                        )
                    },
                    11.0,
                    TEXT_DIM,
                    &z,
                ))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 8.0))
                        .child(
                            input()
                                .value(token_draft)
                                .masked(true)
                                .placeholder(move || t!("settings.github_placeholder"))
                                .h(scaled(&z, 24.0))
                                .flex_1()
                                .px(scaled(&z, 6.0))
                                .bg(rgb(SURFACE_RAISED))
                                .border(scaled(&z, 1.0))
                                .border_color(rgb(BORDER))
                                .text_color(rgb(TEXT))
                                .on_change(move |text: &str| token_draft.set(text.to_owned())),
                        )
                        .child(button(
                            &z,
                            move || t!("settings.save_token"),
                            true,
                            move || {
                                save_token.save_github_token();
                            },
                        ))
                        .child(button(
                            &z,
                            move || t!("settings.test_token"),
                            true,
                            move || {
                                check_token.validate_github_token();
                            },
                        ))
                        .child(button(
                            &z,
                            move || t!("settings.clear_token"),
                            true,
                            move || {
                                clear_token.clear_github_token();
                            },
                        )),
                )
                .child(pix(
                    move || token_status.github_token_check.get(),
                    11.0,
                    TEXT_DIM,
                    &z,
                )),
        ))
}

fn zoom_button(ui: &Ui, level: u8) -> Text {
    let z = ui.zoom;
    let label = match level {
        1 => "1x",
        2 => "2x",
        _ => "3x",
    };
    let colored = ui.clone();
    let bordered = ui.clone();
    let pressed = ui.clone();
    pix_c(
        label,
        12.0,
        move || {
            if colored.zoom.get() == level {
                rgb(ACCENT)
            } else {
                rgb(TEXT_MUTED)
            }
        },
        &z,
    )
    .px(scaled(&z, 10.0))
    .py(scaled(&z, 4.0))
    .bg(rgb(SURFACE_RAISED))
    .border(scaled(&z, 1.0))
    .border_color(move || {
        if bordered.zoom.get() == level {
            rgb(ACCENT)
        } else {
            rgb(BORDER_DIM)
        }
    })
    .on_press(move || pressed.set_ui_scale(level))
}

fn language_button(ui: &Ui, index: usize, z: &Signal<u8>) -> Text {
    let (code, _) = i18n::LANGUAGES[index];
    let colored = ui.clone();
    let bordered = ui.clone();
    let pressed = ui.clone();
    pix_c(
        move || i18n::language_label(code),
        12.0,
        move || {
            if colored.settings.get().language == code {
                rgb(REQUEST)
            } else {
                rgb(TEXT_MUTED)
            }
        },
        z,
    )
    .px(scaled(z, 10.0))
    .py(scaled(z, 4.0))
    .bg(rgb(SURFACE_RAISED))
    .border(scaled(z, 1.0))
    .border_color(move || {
        if bordered.settings.get().language == code {
            rgb(ACCENT)
        } else {
            rgb(BORDER_DIM)
        }
    })
    .on_press(move || pressed.set_language(code))
}
