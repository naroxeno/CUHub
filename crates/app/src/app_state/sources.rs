//! Remote GitHub source manifest refresh.

use super::state::Ui;
use cuprum_core::hub::{HubError, SourcesRefresh};
use nsh_core::prelude::*;

impl Ui {
    pub fn refresh_sources(&self) {
        self.refresh_sources_with(false);
    }

    pub fn refresh_sources_now(&self) {
        self.refresh_sources_with(true);
    }

    fn refresh_sources_with(&self, force: bool) {
        if self.sources_refreshing.get() {
            return;
        }
        self.sources_refreshing.set(true);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let refresh = hub.refresh_sources(force)?;
                Ok::<_, HubError>((refresh, hub.sources().is_some()))
            })
            .await;
            match result {
                Ok(Ok((refresh, has_sources))) => {
                    ui.sources_ready.set(has_sources);
                    ui.rebuild_local_rows();
                    ui.rebuild_remote();
                    match refresh {
                        SourcesRefresh::Updated { count, .. } => {
                            ui.notice.set(t!("notice.sources_refreshed", count = count))
                        }
                        SourcesRefresh::NotModified => ui.notice.set(t!("notice.sources_current")),
                    }
                }
                Ok(Err(error)) => ui
                    .notice
                    .set(t!("notice.sources_refresh_failed", error = error)),
                Err(error) => ui
                    .notice
                    .set(t!("notice.sources_refresh_failed", error = error)),
            }
            ui.sources_refreshing.set(false);
        });
    }
}
