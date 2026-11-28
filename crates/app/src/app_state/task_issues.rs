//! Actionable task blockers and copy-safe summaries for session failures.

use cuprum_core::hub::HubError;
use cuprum_core::multiplayer::TunnelState;
use nsh_core::t;

use super::state::Ui;
use super::tasks::{TaskAction, TaskButton, TaskCard, TaskKind};
use crate::palette::{DANGER, TEXT_MUTED, WARN};
use crate::shell::nav;

struct Blocker {
    id: &'static str,
    title: &'static str,
    reason: &'static str,
    action: Option<TaskButton>,
}

impl Blocker {
    fn card(self) -> TaskCard {
        let title = t!(self.title);
        let reason = t!(self.reason);
        TaskCard {
            key: self.id.into(),
            kind: TaskKind::Blocker,
            title: title.clone(),
            subtitle: String::new(),
            lines: vec![(reason.clone(), WARN)],
            progress: None,
            status: String::new(),
            buttons: self.action.into_iter().collect(),
            busy: false,
            copy_details: Some(format!("{title}\n{reason}\n[{}]", self.reason)),
        }
    }
}

fn link(page: &'static str, label: &'static str) -> TaskButton {
    TaskButton {
        action: TaskAction::OpenPage(page),
        label: t!(label),
        enabled: true,
    }
}

pub(super) fn blocker_cards(ui: &Ui) -> Vec<TaskCard> {
    let mut blockers = Vec::new();
    if ui.settings.get().game_path.trim().is_empty() && !ui.setup.open.get() {
        blockers.push(Blocker {
            id: "game-folder",
            title: "task.issue_game",
            reason: "notice.no_game_folder",
            action: Some(link(nav::SETTINGS, "task.open_settings")),
        });
    }
    blockers.extend(multiplayer_blockers(ui));
    let (profile_reason, profile_detail) = ui.profiles.error.get();
    if !profile_reason.is_empty() {
        blockers.push(Blocker {
            id: "profiles",
            title: "task.issue_profiles",
            reason: if profile_detail.is_empty() {
                profile_reason
            } else {
                "task.reason_failed"
            },
            action: Some(link(nav::PROFILES, "task.open_profiles")),
        });
    }
    let mp = ui.mp;
    blockers
        .into_iter()
        .map(|blocker| {
            let mut card = blocker.card();
            match card.key.as_str() {
                "multiplayer-room" => {
                    if let TunnelState::Failed(reason) = mp.state.get() {
                        card.lines.push((reason, TEXT_MUTED));
                    }
                }
                "multiplayer-bridge" => {
                    card.lines
                        .push((super::bridge_text(ui.bridge.get()), TEXT_MUTED));
                }
                "multiplayer-game" => {
                    for detail in [mp.game_error.get(), mp.port_error.get()] {
                        if !detail.is_empty() {
                            card.lines.push((detail, TEXT_MUTED));
                        }
                    }
                }
                "profiles" if !profile_detail.is_empty() => {
                    card.lines.push((profile_detail.clone(), TEXT_MUTED));
                }
                _ => {}
            }
            card
        })
        .collect()
}

fn multiplayer_blockers(ui: &Ui) -> Vec<Blocker> {
    let mp = ui.mp;
    let mut blockers = Vec::new();
    let room_error = mp.error.get();
    let state = mp.state.get();
    let relevant = ui.page.get() == nav::MULTIPLAYER || mp.running() || !room_error.is_empty();
    if relevant && !mp.elevated.get() {
        blockers.push(Blocker {
            id: "multiplayer-permission",
            title: "task.issue_multiplayer",
            reason: "multiplayer.state_elevation_hint",
            action: None,
        });
    }
    let room_reason = match room_error {
        "" if matches!(state, TunnelState::Failed(_)) => "multiplayer.error_start",
        "multiplayer.error_elevation" => "",
        other => other,
    };
    if !room_reason.is_empty() {
        blockers.push(Blocker {
            id: "multiplayer-room",
            title: "task.issue_multiplayer",
            reason: room_reason,
            action: Some(link(nav::MULTIPLAYER, "task.open_multiplayer")),
        });
    }
    let bridge = ui.bridge.get();
    if relevant && bridge.game_running && bridge.status.session.is_none() {
        let missing = ui
            .bridge_install
            .get()
            .is_some_and(|status| !status.installed);
        if missing || bridge.start_error.is_some() || bridge.status.last_error.is_some() {
            blockers.push(Blocker {
                id: "multiplayer-bridge",
                title: "local.bridge_title",
                reason: if missing {
                    "task.reason_bridge_missing"
                } else {
                    "task.reason_bridge"
                },
                action: Some(if missing {
                    link(nav::LOCAL, "task.open_bridge")
                } else {
                    link(nav::MULTIPLAYER, "task.open_multiplayer")
                }),
            });
        }
    }
    if !mp.game_error.get().is_empty() || !mp.port_error.get().is_empty() {
        let retry = ui.mp_game_phase() == super::GamePhase::Ready && !mp.is_busy();
        blockers.push(Blocker {
            id: "multiplayer-game",
            title: "task.issue_in_game",
            reason: if !mp.port_error.get().is_empty() {
                "task.reason_room_port"
            } else {
                "task.reason_in_game"
            },
            action: Some(if retry {
                TaskButton {
                    action: TaskAction::RetryGame,
                    label: t!("multiplayer.game_retry"),
                    enabled: true,
                }
            } else {
                link(nav::MULTIPLAYER, "task.open_multiplayer")
            }),
        });
    }
    blockers
}

pub(super) fn failure_cards(ui: &Ui) -> Vec<TaskCard> {
    ui.task_failures
        .get()
        .into_iter()
        .map(|(operation, reason)| {
            let title = t!(operation);
            let blocked_by_game = reason == "task.game_running";
            let summary = t!(reason);
            let mut lines = vec![(summary.clone(), if blocked_by_game { WARN } else { DANGER })];
            let mut copy_details = format!("{title}\n{summary}\n[{reason}]");
            if operation == "task.issue_install"
                && let Some(detail) = ui.thunderstore_install_error.get()
            {
                copy_details.push('\n');
                copy_details.push_str(&detail);
                lines.push((detail, TEXT_MUTED));
            }
            TaskCard {
                key: operation.into(),
                kind: if blocked_by_game {
                    TaskKind::Blocker
                } else {
                    TaskKind::Failure
                },
                title: title.clone(),
                subtitle: String::new(),
                lines,
                progress: None,
                status: String::new(),
                buttons: (!blocked_by_game)
                    .then(|| TaskButton {
                        action: TaskAction::DismissFailure(operation),
                        label: t!("task.dismiss_failure"),
                        enabled: true,
                    })
                    .into_iter()
                    .collect(),
                busy: false,
                copy_details: Some(copy_details),
            }
        })
        .collect()
}

impl Ui {
    pub(crate) fn task_failed(&self, operation: &'static str, error: &HubError) {
        let reason = match error {
            HubError::NoGameFolder => return,
            HubError::GameRunning
                if operation == "task.bepinex_title" && self.bepinex_pending.get().is_some() =>
            {
                return;
            }
            HubError::GameRunning => "task.game_running",
            HubError::Dependencies(_) => "task.reason_dependencies",
            HubError::Network(_) | HubError::NetworkUnavailable | HubError::Nexus(_) => {
                "task.reason_network"
            }
            HubError::Credentials(_)
            | HubError::MissingGithubToken
            | HubError::GithubTokenRejected => "task.reason_credentials",
            HubError::Plan(_) => "task.reason_conflict",
            HubError::Io(_) | HubError::Settings(_) => "task.reason_io",
            _ => "task.reason_failed",
        };
        self.task_failed_with(operation, reason);
    }

    pub(crate) fn task_failed_with(&self, operation: &'static str, reason: &'static str) {
        if operation == "task.issue_install" {
            self.thunderstore_install_error.set(None);
        }
        self.task_failures.update(|failures| {
            failures.insert(operation, reason);
        });
    }

    pub(crate) fn clear_task_failure(&self, operation: &'static str) {
        if operation == "task.issue_install" {
            self.thunderstore_install_error.set(None);
        }
        self.task_failures.update(|failures| {
            failures.remove(operation);
        });
    }

    pub(super) fn clear_game_task_blocks(&self) {
        if self
            .task_failures
            .get()
            .values()
            .any(|&reason| reason == "task.game_running")
        {
            self.task_failures.update(|failures| {
                failures.retain(|_, reason| *reason != "task.game_running");
            });
        }
    }
}
