//! The Win10-style cards inside the task drawer.

use crate::app_state::batch::BatchMode;
use crate::app_state::{TaskAction, TaskCard, TaskKind, TaskProgress, Ui, bytes_label};
use crate::components::{pix, scaled, spinner, states};
use crate::palette::*;
use nsh_core::animation::{MotionSpec, stagger_delay};
use nsh_core::prelude::*;

const STAGGER_MS: f32 = 45.0;
const STAGGER_STEPS: usize = 6;
const BAR_HEIGHT: f32 = 6.0;

pub(super) fn card_list(z: &Signal<u8>, ui: &Ui) -> impl IntoElement {
    let z = *z;
    let list_ui = ui.clone();
    let row_ui = ui.clone();
    let empty_ui = ui.clone();
    let empty_z = z;
    view()
        .flex_col()
        .w_full()
        .gap(scaled(&z, 8.0))
        .p(scaled(&z, 12.0))
        .child(each(
            move || crate::app_state::cards(&list_ui),
            |card: &TaskCard| card.key.clone(),
            move |card: ReadSignal<TaskCard>, index: ReadSignal<usize>| {
                card_view(&z, &row_ui, &card.get(), index.get())
            },
        ))
        .child(when(
            move || crate::app_state::pending_count(&empty_ui) == 0,
            move || pix(move || t!("task.empty"), 10.0, TEXT_MUTED, &empty_z),
        ))
}

fn card_view(z: &Signal<u8>, ui: &Ui, card: &TaskCard, index: usize) -> View {
    let z = *z;
    let accent = match card.kind {
        TaskKind::Install => ACCENT,
        TaskKind::Dependency => WARN,
        TaskKind::BepInEx => ACCENT,
        TaskKind::Batch => ACCENT,
        TaskKind::Download => TEXT_MUTED,
    };
    let spec = MotionSpec::from(MotionPreset::FadeInRight).timing(|timing| {
        timing.delay(stagger_delay(index, STAGGER_STEPS, StaggerFrom::First) * STAGGER_MS)
    });
    let motion = motion().entering(spec);
    let mut body = states(
        view()
            .flex_col()
            .w_full()
            .gap(scaled(&z, 5.0))
            .p(scaled(&z, 10.0))
            .bg(theme_color(SURFACE))
            .border(scaled(&z, 1.0))
            .border_color(theme_color(BORDER))
            .border_l(scaled(&z, 3.0))
            .border_l_color(theme_color(accent))
            .bind_motion(&motion),
    )
    .child(
        view()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(scaled(&z, 8.0))
            .child(pix(card.title.clone(), 12.0, TEXT, &z))
            .child(pix(kind_label(card.kind), 9.0, accent, &z)),
    );
    if !card.subtitle.is_empty() {
        body = body.child(pix(card.subtitle.clone(), 10.0, TEXT_MUTED, &z));
    }
    for (text, color) in &card.lines {
        body = body.child(pix(text.clone(), 9.0, *color, &z));
    }
    if let Some(progress) = card.progress {
        body = body.child(progress_bar(&z, progress));
    }
    if card.busy {
        body = body.child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 5.0))
                .child(spinner(&z))
                .child(pix(card.status.clone(), 9.0, TEXT_MUTED, &z)),
        );
    } else if !card.status.is_empty() {
        body = body.child(pix(card.status.clone(), 9.0, TEXT_MUTED, &z));
    }
    if !card.buttons.is_empty() {
        let mut row = view()
            .flex_row()
            .items_center()
            .justify_end()
            .gap(scaled(&z, 6.0));
        for item in &card.buttons {
            let press = ui.clone();
            let action = item.action;
            row = row.child(crate::components::button(
                &z,
                item.label.clone(),
                item.enabled,
                move || press_action(&press, action),
            ));
        }
        body = body.child(row);
    }
    body
}

fn progress_bar(z: &Signal<u8>, progress: TaskProgress) -> View {
    let z = *z;
    let fraction = progress.fraction();
    let mut fill = view()
        .h_full()
        .w(relative(fraction))
        .bg(theme_color(ACCENT));
    if progress.total.is_none() {
        fill = fill.animation(
            KeyframeAnimation::new(Keyframes::Pulse)
                .duration(900.0)
                .frame_rate(20.0),
        );
    }
    let label = match progress.percent() {
        Some(percent) => t!(
            "task.progress",
            percent = percent,
            written = bytes_label(progress.written),
            total = bytes_label(progress.total.unwrap_or_default())
        ),
        None => t!(
            "task.progress_unknown",
            written = bytes_label(progress.written)
        ),
    };
    view()
        .flex_col()
        .gap(scaled(&z, 3.0))
        .child(
            view()
                .h(scaled(&z, BAR_HEIGHT))
                .w_full()
                .bg(theme_color(BORDER_DIM))
                .border(scaled(&z, 1.0))
                .border_color(theme_color(BORDER))
                .child(fill),
        )
        .child(pix(label, 9.0, TEXT_MUTED, &z))
}

fn kind_label(kind: TaskKind) -> String {
    match kind {
        TaskKind::Install => t!("task.kind_install"),
        TaskKind::Dependency => t!("task.kind_dependency"),
        TaskKind::BepInEx => t!("task.kind_bepinex"),
        TaskKind::Batch => t!("task.kind_batch"),
        TaskKind::Download => t!("task.kind_download"),
    }
}

fn press_action(ui: &Ui, action: TaskAction) {
    match action {
        TaskAction::Apply => ui.apply_pending(),
        TaskAction::Discard => ui.discard_pending(),
        TaskAction::InstallDependencies => ui.install_dependencies(),
        TaskAction::DismissDependencies => ui.dismiss_dependency_prompt(),
        TaskAction::ApplyBepInEx => ui.apply_bepinex(),
        TaskAction::CancelBepInEx => ui.cancel_bepinex(),
        TaskAction::UpdateSelected => ui.start_batch(BatchMode::Selected),
        TaskAction::UpdateAll => ui.start_batch(BatchMode::All),
    }
}
