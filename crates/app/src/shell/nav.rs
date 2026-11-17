//! Sidebar contents as data: identity, grouping and the content renderer per entry.

use nsh_core::prelude::*;

use crate::pages;
use crate::ui::Ui;

pub const NEXUS: &str = "nexus";
pub const REMOTE: &str = "remote";
pub const LOCAL: &str = "local";
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
    pub icon: &'static str,
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

pub static NAV: [NavEntry; 8] = [
    NavEntry {
        id: HOME,
        key: "nav.hub_home",
        icon: "HM",
        group: None,
        kind: NavKind::Page(home),
    },
    NavEntry {
        id: NEXUS,
        key: "nav.nexus_dashboard",
        icon: "NX",
        group: Some(GROUP_MODS),
        kind: NavKind::Page(nexus),
    },
    NavEntry {
        id: REMOTE,
        key: "nav.remote_mods",
        icon: "RM",
        group: Some(GROUP_MODS),
        kind: NavKind::Page(remote),
    },
    NavEntry {
        id: LOCAL,
        key: "nav.local_mods",
        icon: "LC",
        group: Some(GROUP_MODS),
        kind: NavKind::Page(local),
    },
    NavEntry {
        id: MULTIPLAYER,
        key: "nav.multiplayer",
        icon: "MP",
        group: None,
        kind: NavKind::Page(multiplayer),
    },
    NavEntry {
        id: SKINS,
        key: "nav.skins_backups",
        icon: "SK",
        group: None,
        kind: NavKind::Page(skins),
    },
    NavEntry {
        id: SETTINGS,
        key: "nav.settings",
        icon: "ST",
        group: None,
        kind: NavKind::Page(settings),
    },
    NavEntry {
        id: ABOUT,
        key: "nav.about",
        icon: "AB",
        group: None,
        kind: NavKind::Page(about),
    },
];

pub static LAUNCH_ENTRY: NavEntry = NavEntry {
    id: LAUNCH,
    key: "topbar.launch",
    icon: "GO",
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
