//! Nexus metadata refresh, downloads and API key handling.

use super::state::Ui;
use cuprum_core::hub::{HubError, MetadataRefresh};
use nsh_core::prelude::*;

impl Ui {
    pub fn refresh_metadata(&self, force: bool) {
        if self.nexus_refreshing.get() {
            return;
        }
        self.nexus_refreshing.set(true);
        self.nexus_error.set(String::new());
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let refresh = hub.refresh_metadata(force)?;
                let metadata = hub.metadata().to_vec();
                let mods = hub.scan_mods().ok();
                let synced = hub.metadata_last_synced_label();
                Ok::<_, HubError>((refresh, metadata, mods, synced))
            })
            .await;
            match result {
                Ok(Ok((refresh, metadata, mods, synced))) => {
                    if let Some(mods) = mods {
                        ui.mods.set(mods);
                    }
                    ui.rebuild_nexus(&metadata);
                    ui.rebuild_local_rows();
                    ui.metadata_synced
                        .set(synced.unwrap_or_else(|| t!("common.never")));
                    ui.nexus_refreshing.set(false);
                    match refresh {
                        MetadataRefresh::Updated(count) => ui
                            .notice
                            .set(t!("notice.catalogue_refreshed", count = count)),
                        MetadataRefresh::NotModified => {
                            ui.notice.set(t!("notice.catalogue_current"))
                        }
                    }
                }
                Ok(Err(error)) => {
                    ui.nexus_error.set(error.to_string());
                    ui.nexus_refreshing.set(false);
                    ui.notice
                        .set(t!("notice.catalogue_refresh_failed", error = error));
                }
                Err(error) => {
                    ui.nexus_error.set(t!("notice.catalogue_worker_stopped"));
                    ui.nexus_refreshing.set(false);
                    ui.notice
                        .set(t!("notice.catalogue_refresh_failed", error = error));
                }
            }
        });
    }

    pub fn open_nexus_download(&self, mod_id: String) {
        let result = {
            let hub = self.hub();
            hub.open_nexus_download_page(&mod_id)
        };
        match result {
            Ok(()) => self.notice.set(t!("notice.opened_nexus")),
            Err(error) => self
                .notice
                .set(t!("notice.open_nexus_failed", error = error)),
        }
    }

    pub fn save_nexus_key(&self) {
        let key = self.nexus_key_draft.get();
        if key.trim().is_empty() {
            self.nexus_key_check.set(t!("notice.paste_nexus_key"));
            self.notice.set(t!("notice.paste_nexus_key"));
            return;
        }
        let result = {
            let mut hub = self.hub();
            hub.save_nexus_key(&key)
        };
        match result {
            Ok(()) => {
                self.nexus_key_saved.set(true);
                self.nexus_key_draft.set(String::new());
                self.nexus_key_check.set(t!("notice.nexus_key_saved"));
                self.notice.set(t!("notice.nexus_key_saved"));
            }
            Err(error) => {
                let message = t!("notice.save_nexus_key_failed", error = error);
                self.nexus_key_check.set(message.clone());
                self.notice.set(message);
            }
        }
    }

    pub fn validate_nexus_key(&self) {
        if !self.nexus_key_saved.get() {
            self.nexus_key_check.set(t!("notice.no_key_saved"));
            self.notice.set(t!("notice.save_nexus_key_first"));
            return;
        }
        self.nexus_key_check.set(t!("notice.checking_nexus"));
        let hub = self.hub.clone();
        let saved = self.nexus_key_saved;
        let check = self.nexus_key_check;
        let notice = self.notice;
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.validate_nexus_key()
            })
            .await;
            match result {
                Ok(Ok(account)) => {
                    saved.set(true);
                    if account.is_premium {
                        check.set(t!("notice.nexus_premium"));
                        notice.set(t!("notice.nexus_accepted_premium"));
                    } else {
                        check.set(t!("notice.nexus_standard"));
                        notice.set(t!("notice.nexus_accepted_standard"));
                    }
                }
                Ok(Err(error)) => {
                    check.set(t!("notice.nexus_check_failed", error = error));
                    notice.set(t!("notice.check_nexus_key_failed", error = error));
                }
                Err(error) => {
                    check.set(t!("notice.nexus_check_unfinished"));
                    notice.set(t!("notice.check_nexus_key_failed", error = error));
                }
            }
        });
    }

    pub fn clear_nexus_key(&self) {
        let result = {
            let mut hub = self.hub();
            hub.clear_nexus_key()
        };
        match result {
            Ok(()) => {
                self.nexus_key_saved.set(false);
                self.nexus_key_draft.set(String::new());
                self.nexus_key_check.set(t!("notice.nexus_key_cleared"));
                self.notice.set(t!("notice.nexus_key_cleared"));
            }
            Err(error) => {
                let message = t!("notice.clear_nexus_key_failed", error = error);
                self.nexus_key_check.set(message.clone());
                self.notice.set(message);
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct NexusCovers {
    requested: Signal<Vec<String>>,
    paths: Signal<std::collections::BTreeMap<String, Option<String>>>,
    busy: Signal<bool>,
}

impl NexusCovers {
    pub(super) fn new() -> Self {
        Self {
            requested: signal(Vec::new()),
            paths: signal(std::collections::BTreeMap::new()),
            busy: signal(false),
        }
    }
}

impl Ui {
    pub fn nexus_cover_path(&self, source: &str) -> Option<String> {
        self.nexus_covers.paths.get().get(source).cloned().flatten()
    }

    pub fn request_nexus_covers(&self, sources: Vec<String>) {
        let covers = self.nexus_covers;
        let page = self.page;
        covers.requested.set(sources);
        if covers.busy.get_untracked() {
            return;
        }
        let Some(cache) = self.hub().nexus_covers() else {
            return;
        };
        covers.busy.set(true);
        self.spawn_worker(async move {
            while page.get_untracked() == crate::shell::nav::NEXUS {
                let paths = covers.paths.get_untracked();
                let mut unique = std::collections::BTreeSet::new();
                let pending: Vec<_> = covers
                    .requested
                    .get_untracked()
                    .into_iter()
                    .filter(|url| !paths.contains_key(url) && unique.insert(url.clone()))
                    .take(4)
                    .collect();
                if pending.is_empty() {
                    break;
                }
                let worker_cache = cache.clone();
                let batch = pending.clone();
                let results = blocking(move || {
                    std::thread::scope(|scope| {
                        batch
                            .into_iter()
                            .map(|url| {
                                let cache = &worker_cache;
                                scope.spawn(move || {
                                    let path = cache.fetch(&url);
                                    (url, path)
                                })
                            })
                            .collect::<Vec<_>>()
                            .into_iter()
                            .filter_map(|worker| worker.join().ok())
                            .collect::<Vec<_>>()
                    })
                })
                .await;
                let mut paths = paths;
                for url in pending {
                    paths.insert(url, None);
                }
                if let Ok(results) = results {
                    paths.extend(results);
                }
                covers.paths.set(paths);
            }
            covers.busy.set(false);
        });
    }

    pub fn set_nexus_dashboard_preferences(&self, list: bool, adult: bool) {
        let result = {
            let mut hub = self.hub();
            let mut settings = hub.settings().clone();
            settings.dashboard_list_view = list;
            settings.dashboard_show_adult_mods = adult;
            hub.save_settings(&settings).map(|()| settings)
        };
        match result {
            Ok(settings) => {
                self.settings.set(settings);
                self.refresh_remote_view();
            }
            Err(error) => self
                .notice
                .set(t!("notice.save_settings_failed", error = error)),
        }
    }

    pub fn nexus_dependencies(&self, name: &str) -> Vec<String> {
        self.hub().nexus_dependencies(name)
    }
}

pub(super) fn description_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    let mut source_break = false;
    while let Some(first) = rest.chars().next() {
        let marker = match first {
            '[' => ']',
            '<' => '>',
            _ => '\0',
        };
        if marker != '\0'
            && let Some(end) = rest.find(marker).filter(|end| *end < 512)
        {
            let tag = rest[1..end].trim().trim_end_matches('/').trim();
            if tag.eq_ignore_ascii_case("br") {
                if source_break && output.ends_with('\n') {
                    output.pop();
                }
                if !output.ends_with("\n\n") {
                    output.push('\n');
                }
                source_break = false;
                rest = rest[end + 1..]
                    .strip_prefix("\r\n")
                    .or_else(|| rest[end + 1..].strip_prefix('\n'))
                    .unwrap_or(&rest[end + 1..]);
                continue;
            }
            let name = tag
                .trim_start_matches('/')
                .split(['=', ' '])
                .next()
                .unwrap_or("");
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "b" | "i"
                    | "u"
                    | "s"
                    | "center"
                    | "left"
                    | "right"
                    | "quote"
                    | "code"
                    | "list"
                    | "*"
                    | "color"
                    | "size"
                    | "font"
                    | "url"
                    | "img"
                    | "spoiler"
                    | "p"
                    | "a"
            ) {
                rest = &rest[end + 1..];
                continue;
            }
        }
        source_break = first == '\n';
        if first != '\r' && !(first == '\n' && output.ends_with("\n\n")) {
            output.push(first);
        }
        rest = &rest[first.len_utf8()..];
    }
    output
        .trim()
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::description_text;

    #[test]
    fn description_preserves_author_paragraphs_as_plain_text() {
        assert_eq!(
            description_text(
                "[size=4][b]Title[/b][/size]\n<br />\n<br />Use [url=https://example.com]this guide[/url] &amp; keep [custom]."
            ),
            "Title\n\nUse this guide & keep [custom]."
        );
    }
}
