//! Sidebar contents as data: identity, grouping and the content renderer per entry.

use nsh_core::prelude::*;

use super::icon_masks;
use crate::app_state::Ui;
use crate::pages;

pub const NEXUS: &str = "nexus";
pub const REMOTE: &str = "remote";
pub const LOCAL: &str = "local";
pub const PROFILES: &str = "profiles";
pub const MULTIPLAYER: &str = "multiplayer";
pub const SKINS: &str = "skins";
pub const HOME: &str = "home";
pub const SETTINGS: &str = "settings";
pub const ABOUT: &str = "about";
pub const LAUNCH: &str = "launch";
pub const GROUP_MODS: &str = "mods";

pub struct NavGroup {
    pub id: &'static str,
    pub key: &'static str,
}

pub struct NavEntry {
    pub id: &'static str,
    pub key: &'static str,
    pub icon: [u8; 8],
    pub group: Option<&'static str>,
    pub kind: NavKind,
}

pub enum NavKind {
    Page(fn(&Ui) -> AnyElement),
    Action(fn(&Ui)),
}

pub const GROUPS: [NavGroup; 1] = [NavGroup {
    id: GROUP_MODS,
    key: "nav.mods",
}];

fn launch(ui: &Ui) {
    ui.launch_game();
}

fn nexus(ui: &Ui) -> AnyElement {
    pages::nexus(ui).into_element()
}

fn remote(ui: &Ui) -> AnyElement {
    pages::remote_mods(ui).into_element()
}

fn local(ui: &Ui) -> AnyElement {
    pages::local_mods(ui).into_element()
}

fn multiplayer(ui: &Ui) -> AnyElement {
    pages::multiplayer(ui).into_element()
}

fn skins(ui: &Ui) -> AnyElement {
    pages::skins_backups(ui).into_element()
}

fn home(ui: &Ui) -> AnyElement {
    pages::hub_home(ui).into_element()
}

fn settings(ui: &Ui) -> AnyElement {
    pages::settings(ui).into_element()
}

fn about(ui: &Ui) -> AnyElement {
    pages::about(ui).into_element()
}

fn profiles(ui: &Ui) -> AnyElement {
    pages::profiles(ui).into_element()
}

pub static NAV: [NavEntry; 9] = [
    NavEntry {
        id: HOME,
        key: "nav.hub_home",
        icon: icon_masks::HOME,
        group: None,
        kind: NavKind::Page(home),
    },
    NavEntry {
        id: NEXUS,
        key: "nav.nexus_dashboard",
        icon: icon_masks::NEXUS,
        group: Some(GROUP_MODS),
        kind: NavKind::Page(nexus),
    },
    NavEntry {
        id: REMOTE,
        key: "nav.remote_mods",
        icon: icon_masks::REMOTE,
        group: Some(GROUP_MODS),
        kind: NavKind::Page(remote),
    },
    NavEntry {
        id: LOCAL,
        key: "nav.local_mods",
        icon: icon_masks::LOCAL,
        group: Some(GROUP_MODS),
        kind: NavKind::Page(local),
    },
    NavEntry {
        id: PROFILES,
        key: "nav.profiles",
        icon: icon_masks::PROFILES,
        group: Some(GROUP_MODS),
        kind: NavKind::Page(profiles),
    },
    NavEntry {
        id: MULTIPLAYER,
        key: "nav.multiplayer",
        icon: icon_masks::MULTIPLAYER,
        group: None,
        kind: NavKind::Page(multiplayer),
    },
    NavEntry {
        id: SKINS,
        key: "nav.skins_backups",
        icon: icon_masks::SKINS,
        group: None,
        kind: NavKind::Page(skins),
    },
    NavEntry {
        id: SETTINGS,
        key: "nav.settings",
        icon: icon_masks::SETTINGS,
        group: None,
        kind: NavKind::Page(settings),
    },
    NavEntry {
        id: ABOUT,
        key: "nav.about",
        icon: icon_masks::ABOUT,
        group: None,
        kind: NavKind::Page(about),
    },
];

pub static LAUNCH_ENTRY: NavEntry = NavEntry {
    id: LAUNCH,
    key: "topbar.launch",
    icon: icon_masks::LAUNCH,
    group: None,
    kind: NavKind::Action(launch),
};

pub fn title(id: &str) -> String {
    match NAV.iter().find(|entry| entry.id == id) {
        Some(entry) if entry.group == Some(GROUP_MODS) => {
            let key = entry.key;
            t!("nav.breadcrumb", parent = t!("nav.mods"), child = t!(key))
        }
        Some(entry) => {
            let key = entry.key;
            t!(key)
        }
        None => t!("nav.settings"),
    }
}

pub fn in_group(page: &str, group: &str) -> bool {
    NAV.iter()
        .any(|entry| entry.group == Some(group) && entry.id == page)
}
