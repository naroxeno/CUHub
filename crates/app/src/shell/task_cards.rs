//! The Win10-style cards inside the task drawer.

use super::keyboard::{Area, FocusItem, Keyboard};
use crate::app_state::batch::BatchMode;
use crate::app_state::{
    TaskAction, TaskCard, TaskKind, TaskProgress, TaskSection, Ui, bytes_label,
};
use crate::components::{focus_ring, pix, scaled, spinner, states};
use crate::palette::*;
use nsh_core::animation::{MotionSpec, stagger_delay};
use nsh_core::prelude::*;

const STAGGER_MS: f32 = 45.0;
const STAGGER_STEPS: usize = 6;
const BAR_HEIGHT: f32 = 6.0;

const SECTIONS: [TaskSection; 3] = [
    TaskSection::Attention,
    TaskSection::Progress,
    TaskSection::Failed,
];

pub(super) fn card_list(z: &Signal<u8>, ui: &Ui) -> impl IntoElement {
    let z = *z;
    let list_ui = ui.clone();
    let cards = computed(move || crate::app_state::cards(&list_ui));
    let keyboard = Keyboard::get();
    effect(move || {
        let cards = cards.get();
        let order = SECTIONS
            .iter()
            .flat_map(|section| {
                cards
                    .iter()
                    .filter(move |card| card.section() == *section)
                    .map(|card| card.key.clone())
            })
            .collect();
        keyboard
            .tasks
            .set_order(order, keyboard.active(Area::Tasks));
    });
    let mut view = view()
        .flex_col()
        .w_full()
        .gap(scaled(&z, 8.0))
        .p(scaled(&z, 12.0));
    for section in SECTIONS {
        view = view.child(task_group(&z, ui, cards, section));
    }
    view.child(when(
        move || cards.get().is_empty(),
        move || pix(move || t!("task.empty"), 11.0, TEXT_MUTED, &z),
    ))
}

fn task_group(z: &Signal<u8>, ui: &Ui, cards: Memo<Vec<TaskCard>>, section: TaskSection) -> View {
    let z = *z;
    let row_ui = ui.clone();
    let title = match section {
        TaskSection::Attention => "task.section_attention",
        TaskSection::Progress => "task.section_progress",
        TaskSection::Failed => "task.section_failed",
    };
    view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .child(when(
            move || cards.get().iter().any(|card| card.section() == section),
            move || pix(move || t!(title), 11.0, TEXT_MUTED, &z),
        ))
        .child(each(
            move || {
                cards
                    .get()
                    .into_iter()
                    .filter(|card| card.section() == section)
                    .collect::<Vec<_>>()
            },
            |card: &TaskCard| card.key.clone(),
            move |card: ReadSignal<TaskCard>, index: ReadSignal<usize>| {
                let ui = row_ui.clone();
                let spec = MotionSpec::from(MotionPreset::FadeInRight).timing(|timing| {
                    timing.delay(
                        stagger_delay(index.get(), STAGGER_STEPS, StaggerFrom::First) * STAGGER_MS,
                    )
                });
                view()
                    .bind_motion(&motion().entering(spec))
                    .child(card_view(&z, &ui, card))
            },
        ))
}

fn card_view(z: &Signal<u8>, ui: &Ui, card: ReadSignal<TaskCard>) -> View {
    let z = *z;
    let key = card.get_untracked().key;
    let first_action = element_ref();
    let actions_region = element_ref();
    let inside_actions = signal(false);
    actions_region.on_focus_in(move || inside_actions.set(true));
    actions_region.on_focus_out(move || inside_actions.set(false));
    let ui = ui.clone();
    let actions = computed(move || {
        let card = card.get();
        (card.copy_details, card.buttons)
    });
    let body = states(
        view()
            .label(move || card.get().title)
            .role(AccessibilityRole::Group)
            .flex_col()
            .w_full()
            .gap(scaled(&z, 5.0))
            .p(scaled(&z, 10.0))
            .bg(theme_color(SURFACE))
            .border(scaled(&z, 1.0))
            .border_color(theme_color(BORDER))
            .border_l(scaled(&z, 3.0))
            .border_l_color(theme_color_with(move || card_accent(card.get().kind)))
            .focus_within(|s| s.bg(theme_color(SURFACE_RAISED)))
            .focus_visible(focus_ring(&z)),
    )
    .child(dynamic(move || card_description(&z, &card.get())))
    .child(when(
        move || actions.with(|(copy, buttons)| copy.is_some() || !buttons.is_empty()),
        move || {
            let ui = ui.clone();
            view()
                .element_ref(&actions_region)
                .flex_col()
                .child(dynamic(move || {
                    let (copy, buttons) = actions.get();
                    let mut actions = view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .justify_end()
                        .gap(scaled(&z, 6.0));
                    let mut first = true;
                    if let Some(details) = copy {
                        actions = actions.child(
                            crate::components::copy_button(&z, "task.copy_details", move || {
                                Some(details.clone())
                            })
                            .element_ref(&first_action),
                        );
                        first = false;
                    }
                    for item in buttons {
                        let press = ui.clone();
                        let action = item.action;
                        let mut button =
                            crate::components::button(&z, item.label, item.enabled, move || {
                                press_action(&press, action);
                            });
                        if first && item.enabled {
                            button = button.element_ref(&first_action);
                            first = false;
                        }
                        actions = actions.child(button);
                    }
                    actions
                }))
        },
    ));
    Keyboard::get().item(
        body,
        FocusItem {
            area: Area::Tasks,
            id: key,
            extra: &["l", "right"],
            action: Box::new(move |key| {
                if matches!(key, "l" | "ArrowRight") {
                    first_action.focus();
                }
            }),
            back: Some(inside_actions),
        },
    )
}

fn card_accent(kind: TaskKind) -> ColorRole {
    match kind {
        TaskKind::Install => ACCENT,
        TaskKind::Dependency => WARN,
        TaskKind::BepInEx => ACCENT,
        TaskKind::Batch => ACCENT,
        TaskKind::Download => TEXT_MUTED,
        TaskKind::Blocker => WARN,
        TaskKind::Failure => DANGER,
    }
}

fn card_description(z: &Signal<u8>, card: &TaskCard) -> View {
    let mut body = view().flex_col().gap(scaled(z, 5.0)).child(
        view()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(scaled(z, 8.0))
            .child(
                pix(card.title.clone(), 12.0, TEXT, z)
                    .flex_1()
                    .min_w(px(0.0)),
            )
            .child(pix(kind_label(card.kind), 9.0, card_accent(card.kind), z)),
    );
    if !card.subtitle.is_empty() {
        body = body.child(pix(card.subtitle.clone(), 11.0, TEXT_MUTED, z));
    }
    for (text, color) in &card.lines {
        body = body.child(pix(text.clone(), 11.0, *color, z));
    }
    if let Some(progress) = card.progress {
        body = body.child(progress_bar(z, progress));
    }
    if card.busy {
        body = body.child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(z, 5.0))
                .child(spinner(z))
                .child(pix(card.status.clone(), 9.0, TEXT_MUTED, z)),
        );
    } else if !card.status.is_empty() {
        body = body.child(pix(card.status.clone(), 9.0, TEXT_MUTED, z));
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
        TaskKind::Blocker => t!("task.kind_blocked"),
        TaskKind::Failure => t!("task.kind_failed"),
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
        TaskAction::OpenPage(page) => {
            ui.page.set(page);
            ui.task_open.set(false);
        }
        TaskAction::RetryGame => ui.mp_retry_game_control(),
        TaskAction::DismissFailure(operation) => ui.clear_task_failure(operation),
    }
}
