use crate::app_state::{LocalModRow, Ui};
use crate::components::{button, focus_ring, pix, scaled, spinner};
use crate::palette::*;
use crate::shell::keyboard::Keyboard;
use nsh_base::{MenuEvent, OpenChangeReason, menu, menu_item};
use nsh_core::prelude::*;
use nsh_core::{Checked, LayoutAnimation, LayoutTransition};

pub(super) fn bulk_menu(z: &Signal<u8>, ui: &Ui) -> impl IntoElement {
    let z = *z;
    let keys = Keyboard::get();
    let watch = ui.clone();
    let ui = ui.clone();
    when(
        move || watch.mod_files_busy() || watch.local_rows.get().is_empty(),
        move || button(&z, move || t!("local.bulk_actions"), false, || {}),
    )
    .otherwise(move || {
        cleanup(move || keys.menu_open("bulk", false));
        let enable = ui.clone();
        let disable = ui.clone();
        let delete = ui.clone();
        actions_menu(&z, "local.bulk_actions")
            .on_open_change(move |event: &MenuEvent| {
                if let MenuEvent::OpenChange(open, _) = event {
                    keys.menu_open("bulk", *open);
                }
            })
            .child(action(&z, "local.enable_all", TEXT, move || {
                enable.set_all_mods_enabled(true);
            }))
            .child(action(&z, "local.disable_all", TEXT, move || {
                disable.set_all_mods_enabled(false);
            }))
            .child(action(&z, "common.delete_all", DANGER, move || {
                delete.request_delete_all();
                keys.bulk_delete.replace(delete.delete_busy.get_untracked());
            }))
    })
}

#[derive(Clone, Copy)]
pub(super) struct RowActionState {
    pub details: Signal<bool>,
    pub menu_open: Signal<bool>,
    pub open_from_row: Signal<bool>,
}

pub(super) fn row_actions(
    z: &Signal<u8>,
    row: &LocalModRow,
    ui: &Ui,
    state: RowActionState,
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
        .child(row_menu(&z, &row, &ui, state))
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

fn row_menu(z: &Signal<u8>, row: &LocalModRow, ui: &Ui, state: RowActionState) -> impl IntoElement {
    let RowActionState {
        details,
        menu_open,
        open_from_row,
    } = state;
    let keyboard = Keyboard::get();
    let menu_id = row.focus_key();
    let cleanup_id = menu_id.clone();
    let watch_id = menu_id.clone();
    cleanup(move || keyboard.menu_open(&cleanup_id, false));
    effect(move || keyboard.menu_open(&watch_id, menu_open.get()));
    let first_item = element_ref();
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
    let mut menu = actions_menu(z, "local.row_actions")
        .open(move || Some(menu_open.get()))
        .on_open_change(move |event: &MenuEvent| {
            if let MenuEvent::OpenChange(next, reason) = event {
                menu_open.set(*next);
                if !next && open_from_row.get_untracked() {
                    open_from_row.set(false);
                    if matches!(reason, OpenChangeReason::Cancel | OpenChangeReason::Select) {
                        after_answer({
                            let menu_id = menu_id.clone();
                            move || keyboard.local.focus(&menu_id)
                        });
                    }
                }
            }
        })
        .on_open(move || {
            if open_from_row.get_untracked() {
                first_item.focus();
            }
        })
        .child(
            menu_item_style(z)
                .element_ref(&first_item)
                .on_select(move || toggle_details(details))
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
            .on_select(move || {
                keyboard.bulk_delete.replace(false);
                delete.request_delete_mod(delete_key.clone());
            })
            .child(pix(move || t!("common.delete"), 11.0, DANGER, z)),
    )
}

pub(super) fn toggle_details(details: Signal<bool>) {
    animate_layout(LayoutAnimation {
        update: Some(LayoutTransition::spring(
            Spring::new(220.0).damping_ratio(0.8),
        )),
        ..LayoutAnimation::default()
    });
    details.set(!details.get());
}

fn actions_menu(z: &Signal<u8>, label: &'static str) -> nsh_base::MenuElement {
    let z = *z;
    menu()
        .align(Align::End)
        .offset(scaled(&z, 4.0))
        .popup_style(move |s| {
            s.bg(theme_color(SURFACE_RAISED))
                .border(scaled(&z, 1.0))
                .border_color(theme_color(BORDER))
        })
        .trigger(
            button(&z, move || t!(label), false, || {})
                .text_color(theme_color(TEXT))
                .border_color(theme_color(BORDER)),
        )
}

fn menu_item_style(z: &Signal<u8>) -> nsh_base::MenuItemElement {
    menu_item()
        .px(scaled(z, 8.0))
        .py(scaled(z, 5.0))
        .hover(|s| s.bg(theme_color(ACCENT_PRESS)))
        .focus(|s| s.bg(theme_color(ACCENT_PRESS)))
        .focus_visible(focus_ring(z))
}

fn action(
    z: &Signal<u8>,
    label: &'static str,
    color: ColorRole,
    select: impl Fn() + 'static,
) -> impl IntoElement {
    menu_item_style(z)
        .on_select(select)
        .child(pix(move || t!(label), 11.0, color, z))
}

pub(super) fn batch_checkbox(z: &Signal<u8>, ui: &Ui, key: String, enabled: bool) -> View {
    let z = *z;
    let press = ui.clone();
    let press_key = key.clone();
    let watch = ui.clone();
    let checked = ui.batch_selection;
    let accessible_key = key.clone();
    let mut cell = view()
        .role(AccessibilityRole::Checkbox)
        .label(move || t!("keyboard.batch_select"))
        .accessibility_state(move || AccessibilityState {
            checked: Some(if checked.get().contains(&accessible_key) {
                Checked::True
            } else {
                Checked::False
            }),
            ..AccessibilityState::default()
        })
        .focus_visible(focus_ring(&z))
        .w(scaled(&z, 12.0))
        .h(scaled(&z, 12.0))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .border(scaled(&z, 1.0))
        .border_color(theme_color(BORDER));
    if enabled {
        cell = cell.cursor(Cursor::Pointer).on_press(move || {
            press.toggle_batch_selection(press_key.clone());
        });
    }
    cell.child(when(
        move || watch.batch_selection.get().contains(&key),
        move || pix("✓", 9.0, ACCENT, &z),
    ))
}
