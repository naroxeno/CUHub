//! Shared app state, Hub-backed actions and background work.

pub mod batch;
mod bepinex;
mod bridge;
mod bridge_install;
mod delete;
mod github;
mod install;
mod local_import;
mod mods;
mod multiplayer;
mod multiplayer_bridge;
mod multiplayer_rules;
mod nexus;
mod profiles;
mod remote;
mod rows;
mod sources;
mod state;
mod task_issues;
mod tasks;

pub(crate) use bridge::{bridge_color, bridge_text};
pub(crate) use multiplayer::{MpBusy, MultiplayerUi, busy_key};
pub(crate) use multiplayer_rules::{
    GamePhase, HOST_ADDRESS, detected_port, game_address, game_status_key, host_port, join_port,
    port_mismatch, port_source_key,
};
pub(crate) use profiles::{MigrationResult, ProfileRow, ProfilesBusy};
pub(crate) use remote::{RemoteFilter, RemoteRow, RemoteSource};
pub(crate) use rows::{LocalModRow, NEXUS_PAGE_SIZE, NexusModRow, nexus_page_count};
pub(crate) use state::Ui;
pub(crate) use tasks::{
    TaskAction, TaskCard, TaskKind, TaskProgress, TaskSection, attention_count, bytes_label, cards,
};
