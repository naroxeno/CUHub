//! Catalogue filters, sorting and controls.

use crate::app_state::{NEXUS_PAGE_SIZE, NexusModRow, Ui, nexus_page_count};
use crate::components::{button, pix, scaled, spinner, text_input};
use crate::palette::*;
use cuprum_core::mods::ModStatus;
use nsh_base::{menu, menu_item};
use nsh_core::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Installed,
    Missing,
    Updates,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Downloads,
    Endorsements,
    Name,
    Author,
}

const FILTERS: [(Filter, &str); 4] = [
    (Filter::All, "nexus.filter_all"),
    (Filter::Installed, "nexus.filter_installed"),
    (Filter::Missing, "nexus.filter_missing"),
    (Filter::Updates, "nexus.filter_updates"),
];
const SORTS: [(Sort, &str); 4] = [
    (Sort::Downloads, "nexus.sort_downloads"),
    (Sort::Endorsements, "nexus.sort_endorsements"),
    (Sort::Name, "nexus.sort_name"),
    (Sort::Author, "nexus.sort_author"),
];

#[derive(Clone, Copy)]
pub(super) struct Browse {
    query: Signal<String>,
    filter: Signal<Filter>,
    sort: Signal<Sort>,
    page: Signal<usize>,
    pub current: Memo<usize>,
    pub results: Memo<Vec<NexusModRow>>,
    pub rows: Memo<Vec<NexusModRow>>,
    pub detail: Signal<Option<String>>,
    pub scroll: ElementRef,
}

impl Browse {
    pub fn new(ui: &Ui) -> Self {
        let query = signal(String::new());
        let filter = signal(Filter::All);
        let sort = signal(Sort::Downloads);
        let page = signal(0usize);
        let all = ui.nexus_rows;
        let settings = ui.settings;
        let results = computed(move || {
            select_rows(
                all.get(),
                &query.get(),
                filter.get(),
                sort.get(),
                settings.get().dashboard_show_adult_mods,
            )
        });
        let current = computed(move || page.get().min(nexus_page_count(results.get().len()) - 1));
        let rows = computed(move || {
            results
                .get()
                .into_iter()
                .skip(current.get() * NEXUS_PAGE_SIZE)
                .take(NEXUS_PAGE_SIZE)
                .collect()
        });
        Self {
            query,
            filter,
            sort,
            page,
            current,
            results,
            rows,
            detail: signal(None),
            scroll: element_ref(),
        }
    }

    pub fn go_to(self, page: usize) {
        self.detail.set(None);
        self.page.set(page);
        self.scroll.scroll_to_offset(px(0.0), false);
    }
}

fn select_rows(
    mut rows: Vec<NexusModRow>,
    query: &str,
    filter: Filter,
    sort: Sort,
    adult: bool,
) -> Vec<NexusModRow> {
    let query = query.trim().to_lowercase();
    rows.retain(|row| {
        (adult || !row.adult)
            && match filter {
                Filter::All => true,
                Filter::Installed => row.status.is_some(),
                Filter::Missing => row.status.is_none(),
                Filter::Updates => row.status == Some(ModStatus::OutOfDate),
            }
            && (query.is_empty()
                || [&row.name, &row.author, &row.description]
                    .iter()
                    .any(|value| value.to_lowercase().contains(&query)))
    });
    match sort {
        Sort::Downloads => rows.sort_by_key(|row| std::cmp::Reverse(row.downloads_count)),
        Sort::Endorsements => rows.sort_by_key(|row| std::cmp::Reverse(row.endorsements)),
        Sort::Name => rows.sort_by_cached_key(|row| row.name.to_lowercase()),
        Sort::Author => rows.sort_by_cached_key(|row| row.author.to_lowercase()),
    }
    rows
}

pub(super) fn toolbar(ui: &Ui, browse: Browse) -> View {
    let z = ui.zoom;
    view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .p(scaled(&z, 8.0))
        .bg(theme_color(SURFACE_RAISED))
        .border_b(scaled(&z, 1.0))
        .border_b_color(theme_color(BORDER_DIM))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .justify_between()
                .items_center()
                .gap(scaled(&z, 8.0))
                .child(summary(ui))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .gap(scaled(&z, 6.0))
                        .child(preference_button(ui, browse, true))
                        .child(preference_button(ui, browse, false))
                        .child(refresh_button(ui)),
                ),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .flex_wrap()
                .gap(scaled(&z, 6.0))
                .child(
                    text_input(&z)
                        .value(browse.query)
                        .placeholder(move || t!("nexus.search"))
                        .label(move || t!("nexus.search"))
                        .h(scaled(&z, 27.0))
                        .min_w(scaled(&z, 140.0))
                        .flex_1()
                        .bg(theme_color(SURFACE))
                        .border_color(theme_color(BORDER_DIM))
                        .on_change(move |text: &str| {
                            browse.query.set(text.to_owned());
                            browse.go_to(0);
                        }),
                )
                .child(choices(&z, browse, browse.filter, &FILTERS))
                .child(choices(&z, browse, browse.sort, &SORTS)),
        )
}

fn summary(ui: &Ui) -> View {
    let z = ui.zoom;
    let rows = ui.nexus_rows;
    view()
        .flex_col()
        .gap(scaled(&z, 2.0))
        .child(pix(move || t!("nexus.dashboard_title"), 14.0, TEXT, &z))
        .child(pix(
            move || {
                let rows = rows.get();
                let installed = rows.iter().filter(|row| row.status.is_some()).count();
                let updates = rows
                    .iter()
                    .filter(|row| row.status == Some(ModStatus::OutOfDate))
                    .count();
                t!(
                    "nexus.summary",
                    total = rows.len(),
                    installed = installed,
                    updates = updates
                )
            },
            10.0,
            TEXT_MUTED,
            &z,
        ))
}

fn preference_button(ui: &Ui, browse: Browse, adult: bool) -> impl IntoElement {
    let z = ui.zoom;
    let settings = ui.settings;
    let ui = ui.clone();
    button(
        &z,
        move || {
            let value = settings.get();
            if adult {
                if value.dashboard_show_adult_mods {
                    t!("nexus.adult_on")
                } else {
                    t!("nexus.adult_off")
                }
            } else if value.dashboard_list_view {
                t!("nexus.switch_grid")
            } else {
                t!("nexus.switch_list")
            }
        },
        true,
        move || {
            let value = settings.get();
            ui.set_nexus_dashboard_preferences(
                value.dashboard_list_view ^ !adult,
                value.dashboard_show_adult_mods ^ adult,
            );
            browse.detail.set(None);
            if adult {
                browse.go_to(0);
            }
        },
    )
}

fn refresh_button(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let busy = ui.nexus_refreshing;
    let ui = ui.clone();
    when(
        move || busy.get(),
        move || {
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 5.0))
                .child(spinner(&z))
                .child(pix(move || t!("nexus.refreshing"), 11.0, TEXT_MUTED, &z))
        },
    )
    .otherwise(move || {
        let ui = ui.clone();
        button(
            &z,
            move || t!("common.refresh"),
            true,
            move || ui.refresh_metadata(true),
        )
    })
}

fn choices<T: Copy + PartialEq + 'static>(
    z: &Signal<u8>,
    browse: Browse,
    value: Signal<T>,
    options: &'static [(T, &'static str)],
) -> impl IntoElement {
    let z = *z;
    let mut dropdown = menu()
        .offset(scaled(&z, 3.0))
        .popup_style(move |s| {
            s.bg(theme_color(SURFACE_RAISED))
                .border(scaled(&z, 1.0))
                .border_color(theme_color(BORDER_DIM))
        })
        .trigger(
            button(
                &z,
                move || {
                    let key = options
                        .iter()
                        .find(|(option, _)| *option == value.get())
                        .map_or(options[0].1, |(_, key)| *key);
                    format!("{} ▾", t!(key))
                },
                false,
                || {},
            )
            .text_color(theme_color(TEXT)),
        );
    for &(option, key) in options {
        dropdown = dropdown.child(
            menu_item()
                .checked(move || Some(value.get() == option))
                .px(scaled(&z, 8.0))
                .py(scaled(&z, 5.0))
                .hover(|s| s.bg(theme_color(BORDER_DIM)))
                .focus(|s| s.bg(theme_color(BORDER_DIM)))
                .on_select(move || {
                    value.set(option);
                    browse.go_to(0);
                })
                .child(pix(move || t!(key), 11.0, TEXT, &z)),
        );
    }
    dropdown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, adult: bool, status: Option<ModStatus>) -> NexusModRow {
        NexusModRow {
            id: name.to_owned(),
            name: name.to_owned(),
            author: "Author".into(),
            version: "1.0".into(),
            downloads_count: 10,
            endorsements: 0,
            unique_downloads: 0,
            description: "Searchable description".into(),
            description_source: "".into(),
            cover: None,
            page_url: None,
            adult,
            has_page: false,
            download_id: None,
            status,
            installed_version: None,
        }
    }

    #[test]
    fn filters_by_description_install_state_and_adult_preference() {
        let rows = vec![
            row("Zulu", false, Some(ModStatus::Disabled)),
            row("Alpha", false, Some(ModStatus::OutOfDate)),
            row("Hidden", true, Some(ModStatus::OutOfDate)),
            row("Uninstalled", false, None),
        ];
        let installed = select_rows(
            rows.clone(),
            " DESCRIPTION ",
            Filter::Installed,
            Sort::Name,
            false,
        );
        assert_eq!(
            installed
                .iter()
                .map(|row| row.name.as_str())
                .collect::<Vec<_>>(),
            ["Alpha", "Zulu"]
        );
        let updated = select_rows(rows, "", Filter::Updates, Sort::Name, true);
        assert_eq!(
            updated
                .iter()
                .map(|row| row.name.as_str())
                .collect::<Vec<_>>(),
            ["Alpha", "Hidden"]
        );
    }
}
