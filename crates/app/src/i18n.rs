//! Page keys, language codes and the copy keys that do not sit next to a widget.

use cuhub_core::mods::ModStatus;
use nsh_core::platform::Locale;
use nsh_core::prelude::*;

pub const PAGE_KEYS: [&str; 7] = [
    "nav.nexus_dashboard",
    "nav.remote_mods",
    "nav.local_mods",
    "nav.multiplayer",
    "nav.skins_backups",
    "nav.hub_home",
    "nav.settings",
];

pub const MODS_PAGE_COUNT: usize = 3;

pub fn page_label(index: usize) -> String {
    let key = PAGE_KEYS.get(index).copied().unwrap_or("nav.settings");
    if index < MODS_PAGE_COUNT {
        t!("nav.breadcrumb", parent = t!("nav.mods"), child = t!(key))
    } else {
        t!(key)
    }
}

pub const FOLLOW_SYSTEM: &str = "system";

pub const LANGUAGES: [(&str, &str); 4] = [
    (FOLLOW_SYSTEM, "System"),
    ("en", "English"),
    ("ja", "日本語"),
    ("zh-CN", "简体中文"),
];

pub fn is_known_language(code: &str) -> bool {
    LANGUAGES.iter().any(|(id, _)| *id == code)
}

pub fn language_name(code: &str) -> &'static str {
    LANGUAGES
        .iter()
        .find(|(id, _)| *id == code)
        .map(|(_, name)| *name)
        .unwrap_or("System")
}

pub fn language_label(code: &str) -> String {
    if code == FOLLOW_SYSTEM {
        t!("settings.system")
    } else {
        language_name(code).to_owned()
    }
}

pub fn set_language(code: &str) {
    match Locale::parse(code) {
        Some(locale) if is_known_language(code) => set_locale(Some(locale)),
        _ => set_locale(None),
    }
}

pub fn status_key(status: ModStatus) -> &'static str {
    match status {
        ModStatus::Disabled => "status.disabled",
        ModStatus::Unlisted => "status.unlisted",
        ModStatus::UpToDate => "status.up_to_date",
        ModStatus::OutOfDate => "status.update_available",
        ModStatus::Unknown => "status.update_unknown",
    }
}

pub fn status_detail_key(status: ModStatus) -> &'static str {
    match status {
        ModStatus::Disabled => "status_detail.disabled",
        ModStatus::Unlisted => "status_detail.unlisted",
        ModStatus::UpToDate => "status_detail.up_to_date",
        ModStatus::OutOfDate => "status_detail.out_of_date",
        ModStatus::Unknown => "status_detail.unknown",
    }
}
