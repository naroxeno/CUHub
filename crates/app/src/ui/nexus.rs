//! Nexus metadata refresh, downloads and API key handling.

use super::state::Ui;
use cuhub_core::hub::{HubError, MetadataRefresh};
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

    pub fn download_nexus(&self, mod_id: String) {
        if self.nexus_downloading.get().is_some() {
            self.notice.set(t!("notice.nexus_download_running"));
            return;
        }
        if mod_id.trim().is_empty() {
            self.notice.set(t!("notice.no_nexus_id"));
            return;
        }
        self.nexus_downloading.set(Some(mod_id.clone()));
        self.notice.set(t!("notice.nexus_downloading"));
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.download_nexus_latest(&mod_id)
            })
            .await;
            ui.nexus_downloading.set(None);
            match result {
                Ok(Ok(download)) => ui.notice.set(t!(
                    "notice.downloaded",
                    file = download.file_name,
                    path = download.path.display().to_string()
                )),
                Ok(Err(error)) => ui
                    .notice
                    .set(t!("notice.nexus_download_failed", error = error)),
                Err(error) => ui
                    .notice
                    .set(t!("notice.nexus_download_failed", error = error)),
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
                self.notice.set(t!("notice.nexus_key_saved"));
            }
            Err(error) => self
                .notice
                .set(t!("notice.save_nexus_key_failed", error = error)),
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
                self.notice.set(t!("notice.nexus_key_cleared"));
            }
            Err(error) => self
                .notice
                .set(t!("notice.clear_nexus_key_failed", error = error)),
        }
    }
}
