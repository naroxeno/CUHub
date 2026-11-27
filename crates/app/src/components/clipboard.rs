//! Clipboard buttons report the result beside the action.

use nsh_core::element::ClipboardWrite;
use nsh_core::prelude::*;

use super::button;

pub fn copy_button(
    z: &Signal<u8>,
    label: &'static str,
    value: impl Fn() -> Option<String> + 'static,
) -> Text {
    let feedback = signal(label);
    button(
        z,
        move || t!(feedback.get()),
        true,
        move || {
            if let Some(text) = value() {
                let result = set_clipboard(text);
                spawn(async move {
                    feedback.set(match result.await {
                        ClipboardWrite::Written => "task.copy_done",
                        ClipboardWrite::Requested => "task.copy_requested",
                        ClipboardWrite::Failed => "task.copy_failed",
                    });
                });
            }
        },
    )
}
