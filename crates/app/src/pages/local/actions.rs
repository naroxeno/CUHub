use crate::palette::*;
use crate::ui::{LocalModRow, Ui, button, pix, scaled, spinner};
use nsh_base::{menu, menu_item};
use nsh_core::prelude::*;
use nsh_core::{LayoutAnimation, LayoutTransition};

pub(super) fn bulk_menu(z: &Signal<u8>, ui: &Ui) -> impl IntoElement {
    let z = *z;
    let watch = ui.clone();
    let ui = ui.clone();
    when(
        move || watch.mod_files_busy() || watch.local_rows.get().is_empty(),
        move || button(&z, move || t!("local.bulk_actions"), false, || {}),
    )
    .otherwise(move || {
        let enable = ui.clone();
        let disable = ui.clone();
        let delete = ui.clone();
        actions_menu(&z, "local.bulk_actions")
            .child(action(&z, "local.enable_all", TEXT, move || {
                enable.set_all_mods_enabled(true);
            }))
            .child(action(&z, "local.disable_all", TEXT, move || {
                disable.set_all_mods_enabled(false);
            }))
            .child(action(&z, "common.delete_all", DANGER, move || {
                delete.request_delete_all();
            }))
    })
}

pub(super) fn row_actions(
    z: &Signal<u8>,
    row: &LocalModRow,
    ui: &Ui,
    details: Signal<bool>,
) -> View {
    let watch = ui.clone();
    let locked_watch = ui.clone();
    let row = row.clone();
    let locked_row = row.clone();
    let z = *z;
    let ui = ui.clone();
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(scaled(&z, 6.0))
        .child(
            when(
                move || watch.mod_files_busy(),
                move || mod_buttons(&z, &locked_row, &locked_watch, false),
            )
            .otherwise({
                let ui = ui.clone();
                let row = row.clone();
                move || mod_buttons(&z, &row, &ui, true)
            }),
        )
        .child(row_menu(&z, &row, &ui, details))
}

fn mod_buttons(z: &Signal<u8>, row: &LocalModRow, ui: &Ui, enabled: bool) -> View {
    let mut buttons = view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(scaled(z, 6.0));
    if row.can_update() {
        let update = ui.clone();
        let key = row.source_key.clone();
        buttons = buttons.child(button(
            z,
            move || t!("common.update"),
            enabled,
            move || {
                update.update_source(key.clone());
            },
        ));
    } else if let Some(repo) = row.source.repo.clone() {
        if row.source.can_check {
            let check = ui.clone();
            let key = row.source_key.clone();
            buttons = buttons.child(button(
                z,
                move || t!("local.check_updates"),
                enabled,
                move || {
                    check.check_source(key.clone(), repo.clone());
                },
            ));
        } else {
            buttons = buttons.child(spinner(z));
        }
    }
    let toggle = ui.clone();
    let key = row.source_key.clone();
    let enable = row.item.disabled;
    let label = if enable {
        "common.enable"
    } else {
        "common.disable"
    };
    buttons.child(button(
        z,
        move || t!(label),
        enabled,
        move || {
            toggle.set_mod_enabled(key.clone(), enable);
        },
    ))
}

fn row_menu(z: &Signal<u8>, row: &LocalModRow, ui: &Ui, details: Signal<bool>) -> impl IntoElement {
    let detail_label = move || {
        if details.get() {
            t!("local.hide_details")
        } else {
            t!("common.details")
        }
    };
    let delete = ui.clone();
    let delete_busy = ui.clone();
    let delete_key = row.source_key.clone();
    let mut menu = actions_menu(z, "local.row_actions").child(
        menu_item_style(z)
            .on_select(move || {
                animate_layout(LayoutAnimation {
                    update: Some(LayoutTransition::spring(
                        Spring::new(220.0).damping_ratio(0.8),
                    )),
                    ..LayoutAnimation::default()
                });
                details.set(!details.get());
            })
            .child(pix(detail_label, 11.0, TEXT, z)),
    );
    if row.source.can_pin() {
        let pin = ui.clone();
        let key = row.source_key.clone();
        let options = row.source.options.clone();
        let current = row.source.pin_key();
        let busy = ui.clone();
        menu = menu.child(
            menu_item_style(z)
                .disabled(move || busy.mod_files_busy())
                .on_select(move || pin.cycle_source_pin(key.clone(), options.clone()))
                .child(pix(
                    move || t!("local.change_source", current = t!(current)),
                    11.0,
                    TEXT,
                    z,
                )),
        );
    }
    if row.can_update()
        && row.source.can_check
        && let Some(repo) = row.source.repo.clone()
    {
        let check = ui.clone();
        let key = row.source_key.clone();
        let busy = ui.clone();
        menu = menu.child(
            menu_item_style(z)
                .disabled(move || busy.mod_files_busy())
                .on_select(move || check.check_source(key.clone(), repo.clone()))
                .child(pix(move || t!("local.check_updates"), 11.0, TEXT, z)),
        );
    }
    menu.child(
        menu_item_style(z)
            .disabled(move || delete_busy.mod_files_busy())
            .on_select(move || delete.request_delete_mod(delete_key.clone()))
            .child(pix(move || t!("common.delete"), 11.0, DANGER, z)),
    )
}

fn actions_menu(z: &Signal<u8>, label: &'static str) -> nsh_base::MenuElement {
    let z = *z;
    menu()
        .align(Align::End)
        .offset(scaled(&z, 4.0))
        .popup_style(move |s| {
            s.bg(rgb(SURFACE_RAISED))
                .border(scaled(&z, 1.0))
                .border_color(rgb(BORDER))
        })
        .trigger(
            button(&z, move || t!(label), false, || {})
                .text_color(rgb(TEXT))
                .border_color(rgb(BORDER)),
        )
}

fn menu_item_style(z: &Signal<u8>) -> nsh_base::MenuItemElement {
    menu_item()
        .px(scaled(z, 8.0))
        .py(scaled(z, 5.0))
        .hover(|s| s.bg(rgb(ACCENT_PRESS)))
        .focus(|s| s.bg(rgb(ACCENT_PRESS)))
}

fn action(
    z: &Signal<u8>,
    label: &'static str,
    color: u32,
    select: impl Fn() + 'static,
) -> impl IntoElement {
    menu_item_style(z)
        .on_select(select)
        .child(pix(move || t!(label), 11.0, color, z))
}
