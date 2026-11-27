//! Catalogue tile, compact list row and shared mod details.

use super::{Browse, markup::description_html};
use crate::app_state::{NexusModRow, Ui};
use crate::components::{button, pix, pix_c, scaled};
use crate::palette::*;
use cuprum_core::mods::ModStatus;
use nsh_base::{SheetEvent, context_menu, menu_item, sheet, text_view};
use nsh_core::element::ImageSrc;
use nsh_core::prelude::*;
use nsh_core::{LayoutAnimation, LayoutTransition};

pub(super) fn catalogue_card(
    ui: &Ui,
    browse: Browse,
    row: ReadSignal<NexusModRow>,
    list: bool,
) -> View {
    let z = ui.zoom;
    let summary = if list {
        list_summary(&z, row, ui)
    } else {
        tile_summary(&z, row, ui)
    };
    let selected = move || browse.detail.get().as_deref() == Some(row.get().id.as_str());
    let trigger = summary
        .role(AccessibilityRole::Button)
        .label(move || row.get().name)
        .accessibility_state(move || AccessibilityState {
            expanded: Some(selected()),
            ..AccessibilityState::default()
        })
        .cursor(Cursor::Pointer)
        .hover(|s| s.bg(theme_color(SURFACE_RAISED)))
        .focus_visible(|s| s.bg(theme_color(BORDER_DIM)))
        .on_press(move || toggle_details(browse, row.get().id, list));
    view()
        .flex_col()
        .min_w(px(0.0))
        .bg(theme_color(SURFACE))
        .border(scaled(&z, 1.0))
        .border_color(theme_color_with(move || {
            state_color(row.get().status, BORDER_DIM)
        }))
        .child(card_menu(ui, browse, row, list).trigger(trigger))
        .child(when(move || list && selected(), {
            let ui = ui.clone();
            move || detail_body(&ui, row, true)
        }))
}

fn toggle_details(browse: Browse, id: String, list: bool) {
    if list {
        animate_layout(LayoutAnimation {
            update: Some(LayoutTransition::spring(
                Spring::new(220.0).damping_ratio(0.8),
            )),
            ..LayoutAnimation::default()
        });
    }
    let open = browse.detail.get().as_deref() == Some(id.as_str());
    browse.detail.set((!open).then_some(id));
}

fn card_menu(
    ui: &Ui,
    browse: Browse,
    row: ReadSignal<NexusModRow>,
    list: bool,
) -> nsh_base::ContextMenuElement {
    let z = ui.zoom;
    let key = ui.nexus_key_saved;
    let downloading = ui.remote_busy;
    let download = ui.clone();
    let files = ui.clone();
    let public = ui.clone();
    let notice = ui.notice;
    context_menu()
        .popup_style(move |s| {
            s.bg(theme_color(SURFACE_RAISED))
                .border(scaled(&z, 1.0))
                .border_color(theme_color(BORDER_DIM))
        })
        .child(context_action(&z, "common.details", move || {
            toggle_details(browse, row.get().id, list)
        }))
        .child(
            menu_item()
                .disabled(move || {
                    !key.get() || downloading.get().is_some() || row.get().download_id.is_none()
                })
                .on_select(move || {
                    if row.get().download_id.is_some() {
                        download.prepare_catalogue_nexus(row.get().id);
                    }
                })
                .px(scaled(&z, 8.0))
                .py(scaled(&z, 5.0))
                .hover(|s| s.bg(theme_color(BORDER_DIM)))
                .focus(|s| s.bg(theme_color(BORDER_DIM)))
                .child(pix(move || t!("remote.prepare_nexus"), 11.0, TEXT, &z)),
        )
        .child(when(
            move || row.get().page_url.is_some(),
            move || {
                let files = files.clone();
                context_action(&z, "nexus.open_files", move || {
                    files.open_nexus_download(row.get().id)
                })
            },
        ))
        .child(when(
            move || row.get().page_url.is_some(),
            move || {
                let public = public.clone();
                context_action(&z, "nexus.view_mod", move || {
                    if let Some(url) = row.get().page_url {
                        public.open_link(&url);
                    }
                })
            },
        ))
        .child(when(
            move || row.get().page_url.is_some(),
            move || {
                context_action(&z, "nexus.copy_link", move || {
                    copy_link(row.get().page_url, notice)
                })
            },
        ))
}

fn context_action(
    z: &Signal<u8>,
    key: &'static str,
    action: impl Fn() + 'static,
) -> nsh_base::MenuItemElement {
    menu_item()
        .on_select(action)
        .px(scaled(z, 8.0))
        .py(scaled(z, 5.0))
        .hover(|s| s.bg(theme_color(BORDER_DIM)))
        .focus(|s| s.bg(theme_color(BORDER_DIM)))
        .child(pix(move || t!(key), 11.0, TEXT, z))
}

fn copy_link(url: Option<String>, notice: Signal<String>) {
    if let Some(url) = url {
        drop(set_clipboard(url));
        notice.set(t!("nexus.link_copied"));
    }
}

fn tile_summary(z: &Signal<u8>, row: ReadSignal<NexusModRow>, ui: &Ui) -> View {
    view()
        .flex_col()
        .min_w(px(0.0))
        .child(cover(z, move || row.get().cover, ui, CoverSize::Tile))
        .child(
            view()
                .flex_col()
                .gap(scaled(z, 3.0))
                .p(scaled(z, 6.0))
                .child(pix(move || row.get().name, 12.0, TEXT, z).truncate())
                .child(
                    pix(
                        move || {
                            let row = row.get();
                            format!(
                                "{} · {}",
                                row.author,
                                t!("nexus.download_count", count = row.downloads_count)
                            )
                        },
                        10.0,
                        TEXT_MUTED,
                        z,
                    )
                    .truncate(),
                )
                .child(badges(z, move || row.get())),
        )
}

fn list_summary(z: &Signal<u8>, row: ReadSignal<NexusModRow>, ui: &Ui) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(scaled(z, 8.0))
        .p(scaled(z, 5.0))
        .child(view().w(scaled(z, 62.0)).flex_shrink_0().child(cover(
            z,
            move || row.get().cover,
            ui,
            CoverSize::Thumb,
        )))
        .child(
            view()
                .flex_col()
                .flex_1()
                .min_w(px(0.0))
                .gap(scaled(z, 2.0))
                .child(pix(move || row.get().name, 12.0, TEXT, z).truncate())
                .child(
                    pix(
                        move || {
                            let row = row.get();
                            format!(
                                "{} · {} · {}",
                                row.author,
                                t!("nexus.download_count", count = row.downloads_count),
                                t!("nexus.endorsements", count = row.endorsements),
                            )
                        },
                        10.0,
                        TEXT_MUTED,
                        z,
                    )
                    .truncate(),
                )
                .child(
                    pix(
                        move || {
                            row.get()
                                .description
                                .lines()
                                .find(|line| !line.trim().is_empty())
                                .unwrap_or_default()
                                .to_owned()
                        },
                        10.0,
                        TEXT_MUTED,
                        z,
                    )
                    .truncate(),
                ),
        )
        .child(badges(z, move || row.get()))
}

enum CoverSize {
    Tile,
    Thumb,
    InlineDetail,
    SheetDetail,
}

fn cover(
    z: &Signal<u8>,
    source: impl Fn() -> Option<String> + 'static,
    ui: &Ui,
    size: CoverSize,
) -> View {
    let (height, fit) = match size {
        CoverSize::Tile => (101.0, ResizeMode::Cover),
        CoverSize::Thumb => (43.0, ResizeMode::Cover),
        CoverSize::InlineDetail => (130.0, ResizeMode::Contain),
        CoverSize::SheetDetail => (180.0, ResizeMode::Contain),
    };
    let z = *z;
    let ui = ui.clone();
    let path = computed(move || source().and_then(|source| ui.nexus_cover_path(&source)));
    let failed = signal(None::<String>);
    view()
        .w_full()
        .h(scaled(&z, height))
        .overflow_hidden()
        .bg(theme_color(SURFACE_RAISED))
        .child(
            when(
                move || {
                    path.get()
                        .is_some_and(|uri| failed.get().as_ref() != Some(&uri))
                },
                move || {
                    image(move || ImageSrc::from(path.get().unwrap_or_default()))
                        .w_full()
                        .h_full()
                        .object_fit(fit)
                        .on_error(move |_: &str| failed.set(path.get()))
                },
            )
            .otherwise(move || {
                view()
                    .w_full()
                    .h_full()
                    .items_center()
                    .justify_center()
                    .child(pix(move || t!("nexus.no_cover"), 10.0, TEXT_MUTED, &z))
            }),
        )
}

fn badges(z: &Signal<u8>, row: impl Fn() -> NexusModRow + Copy + 'static) -> View {
    let z = *z;
    view()
        .flex_row()
        .flex_wrap()
        .gap(scaled(&z, 5.0))
        .child(when(
            move || row().status.is_some(),
            move || {
                pix_c(
                    move || t!(row().state_key()),
                    10.0,
                    theme_color_with(move || state_color(row().status, TEXT_MUTED)),
                    &z,
                )
            },
        ))
        .child(when(
            move || row().adult,
            move || pix("18+", 10.0, REQUEST, &z),
        ))
}

fn state_color(status: Option<ModStatus>, absent: ColorRole) -> ColorRole {
    match status {
        Some(ModStatus::Disabled) => DANGER,
        Some(ModStatus::OutOfDate) => WARN,
        Some(_) => ACCENT,
        None => absent,
    }
}

pub(super) fn detail_sheet(ui: &Ui, browse: Browse) -> impl IntoElement {
    let z = ui.zoom;
    let settings = ui.settings;
    let details = computed(move || {
        let selected = browse.detail.get()?;
        browse.rows.get().into_iter().find(|row| row.id == selected)
    });
    let ui = ui.clone();
    sheet()
        .side(Placement::Right)
        .popup_style(|style| style.w(relative(0.60)))
        .open(move || Some(!settings.get().dashboard_list_view && details.get().is_some()))
        .on_open_change(move |event: &SheetEvent| {
            if let SheetEvent::OpenChange(false, _) = event {
                browse.detail.set(None);
            }
        })
        .backdrop_style(|style| style.bg(rgba(0x00000066)))
        .child(
            view()
                .flex_col()
                .w_full()
                .h_full()
                .p(scaled(&z, 10.0))
                .gap(scaled(&z, 8.0))
                .bg(theme_color(SURFACE))
                .border_l(scaled(&z, 1.0))
                .border_l_color(theme_color(BORDER))
                .child(view().flex_row().justify_end().child(button(
                    &z,
                    move || t!("common.close"),
                    true,
                    move || browse.detail.set(None),
                )))
                .child(scroll().flex_1().min_h(px(0.0)).scrollbar(true).child(each(
                    move || details.get().into_iter().collect::<Vec<_>>(),
                    |row| row.id.clone(),
                    move |row, _| detail_body(&ui, row, false),
                ))),
        )
}

fn detail_body(ui: &Ui, row: ReadSignal<NexusModRow>, inline: bool) -> View {
    let z = ui.zoom;
    let name = row.get_untracked().name;
    let dependencies = ui.nexus_dependencies(&name);
    view()
        .flex_col()
        .min_w(px(0.0))
        .gap(scaled(&z, 8.0))
        .p(scaled(&z, 8.0))
        .bg(theme_color(SURFACE_RAISED))
        .child(cover(
            &z,
            move || row.get().cover,
            ui,
            if inline {
                CoverSize::InlineDetail
            } else {
                CoverSize::SheetDetail
            },
        ))
        .child(pix(move || row.get().name, 15.0, TEXT, &z))
        .child(pix(
            move || {
                let row = row.get();
                t!(
                    "nexus.detail_meta",
                    author = row.author,
                    version = row.version
                )
            },
            11.0,
            TEXT_MUTED,
            &z,
        ))
        .child(pix(
            move || {
                let row = row.get();
                t!(
                    "nexus.detail_stats",
                    downloads = row.downloads_count,
                    unique = row.unique_downloads,
                    endorsements = row.endorsements
                )
            },
            10.0,
            TEXT_MUTED,
            &z,
        ))
        .child(pix_c(
            move || {
                let row = row.get();
                let key = row.state_key();
                match row.installed_version {
                    Some(version) => {
                        format!("{} · {}", t!(key), t!("nexus.installed", version = version))
                    }
                    None => t!(key),
                }
            },
            11.0,
            theme_color_with(move || state_color(row.get().status, TEXT_MUTED)),
            &z,
        ))
        .child(detail_actions(ui, row))
        .child(pix(
            move || {
                if dependencies.is_empty() {
                    t!("nexus.no_dependencies")
                } else {
                    t!("nexus.dependencies", list = dependencies.join(", "))
                }
            },
            10.0,
            TEXT_MUTED,
            &z,
        ))
        .child(
            text_view()
                .html(move || {
                    let row = row.get();
                    Some(if row.description.is_empty() {
                        description_html(&t!("nexus.no_description")).into()
                    } else {
                        description_html(&row.description_source).into()
                    })
                })
                .selectable(true)
                .font_family(crate::components::PIXEL_FONT.family)
                .text_size(scaled(&z, 12.0))
                .line_height(scaled(&z, 17.0))
                .text_color(theme_color(TEXT))
                .gap(scaled(&z, 6.0)),
        )
}

fn detail_actions(ui: &Ui, row: ReadSignal<NexusModRow>) -> View {
    let z = ui.zoom;
    let key = ui.nexus_key_saved;
    let download_busy = ui.remote_busy;
    let download = ui.clone();
    let open = ui.clone();
    let browser = ui.clone();
    let notice = ui.notice;
    view()
        .flex_row()
        .flex_wrap()
        .gap(scaled(&z, 6.0))
        .child(when(
            move || key.get() && row.get().download_id.is_some(),
            move || {
                let ui = download.clone();
                when(
                    move || download_busy.get().is_none(),
                    move || {
                        let ui = ui.clone();
                        button(
                            &z,
                            move || t!("remote.prepare_nexus"),
                            true,
                            move || {
                                if row.get().download_id.is_some() {
                                    ui.prepare_catalogue_nexus(row.get().id);
                                }
                            },
                        )
                    },
                )
                .otherwise(move || button(&z, move || t!("remote.prepare_nexus"), false, || {}))
            },
        ))
        .child(when(
            move || row.get().has_page && row.get().page_url.is_some(),
            move || {
                let ui = browser.clone();
                button(
                    &z,
                    move || t!("nexus.open_files"),
                    true,
                    move || ui.open_nexus_download(row.get().id),
                )
            },
        ))
        .child(when(
            move || row.get().page_url.is_some(),
            move || {
                let ui = open.clone();
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap(scaled(&z, 6.0))
                    .child(button(
                        &z,
                        move || t!("nexus.view_mod"),
                        true,
                        move || {
                            if let Some(url) = row.get().page_url {
                                ui.open_link(&url);
                            }
                        },
                    ))
                    .child(button(
                        &z,
                        move || t!("nexus.copy_link"),
                        true,
                        move || {
                            if let Some(url) = row.get().page_url {
                                drop(set_clipboard(url));
                                notice.set(t!("nexus.link_copied"));
                            }
                        },
                    ))
            },
        ))
}
