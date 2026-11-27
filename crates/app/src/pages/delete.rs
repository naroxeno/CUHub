//! Delete confirmation overlay.

use crate::app_state::Ui;
use crate::components::{button, confirmation_dialog, danger_button, focus_ring, pix, scaled};
use crate::palette::*;
use crate::shell::keyboard::Keyboard;
use nsh_core::prelude::*;

pub fn delete_overlay(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let prompt = ui.delete_prompt;
    let busy = ui.delete_busy;
    let confirm_ui = ui.clone();
    let cancel_ui = ui.clone();
    let keyboard = Keyboard::get();
    let was_open = stored(false);
    effect(move || {
        let open = prompt.with(|prompt| prompt.is_some());
        if was_open.replace(open) && !open && keyboard.bulk_delete.replace(false) {
            spawn(async move {
                next_frame().await;
                if keyboard.can_restore_local() {
                    keyboard.local.fallback.focus();
                }
            });
        }
    });
    when(
        move || prompt.get().is_some(),
        move || {
            let Some(prompt) = prompt.get() else {
                return view().into_element();
            };
            let title_key = prompt.title_key();
            let title_name = prompt.target.label();
            let action_key = prompt.action_key();
            let note_key = prompt.scope_note_key();
            let summary_key = prompt.summary_key();
            let irreversible_key = prompt.irreversible_key();
            let managed = prompt.managed;
            let irreversible_color = if managed { TEXT_MUTED } else { DANGER };
            let files = prompt.scope.files;
            let root_count = prompt.scope.roots.len();
            let mut list = view().flex_col();
            for root in &prompt.scope.roots {
                list = list.child(pix(root.display().to_string(), 10.0, TEXT_MUTED, &z).w_full());
            }
            let cancel_ref = element_ref();
            let busy_ref = element_ref();
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
                        move || {
                            confirm_ui.confirm_delete();
                            if busy.get_untracked() {
                                after_answer(move || busy_ref.focus());
                            }
                        },
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
                    .element_ref(&cancel_ref)
                }
            });
            let body = view()
                .element_ref(&busy_ref)
                .focusable(move || busy.get())
                .focus_visible(focus_ring(&z))
                .role(AccessibilityRole::Group)
                .label(move || t!("delete.deleting"))
                .accessibility_state(move || AccessibilityState {
                    busy: busy.get(),
                    ..Default::default()
                })
                .flex_col()
                .gap(scaled(&z, 8.0))
                .child(pix(
                    move || {
                        if managed {
                            t!(summary_key)
                        } else {
                            t!(summary_key, files = files, paths = root_count)
                        }
                    },
                    12.0,
                    TEXT,
                    &z,
                ))
                .child(pix(move || t!(note_key), 10.0, TEXT_MUTED, &z))
                .child(pix(
                    move || t!(irreversible_key),
                    10.0,
                    irreversible_color,
                    &z,
                ))
                .child(scroll().w_full().max_h(scaled(&z, 132.0)).child(list))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 8.0))
                        .child(cancel)
                        .child(confirm),
                );
            let escape_ui = cancel_ui.clone();
            confirmation_dialog(&z, move || t!(title_key, name = title_name), body)
                .on_open(move || cancel_ref.focus())
                .on_cancel(move || escape_ui.cancel_delete())
                .into_element()
        },
    )
}
