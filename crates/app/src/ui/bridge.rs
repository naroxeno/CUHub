//! Bridge polling worker: the host follows the game process and the UI reads its snapshot.

use std::collections::BTreeSet;
use std::time::Duration;

use cuhub_core::hub::BridgeSnapshot;
use nsh_core::prelude::*;

use super::state::Ui;

const POLL_MILLIS: u64 = 2_000;

impl Ui {
    pub(crate) fn watch_bridge(&self) {
        let ui = self.clone();
        let hub = self.hub.clone();
        self.spawn_worker(async move {
            let mut last = BridgeSnapshot::default();
            let mut loaded = BTreeSet::new();
            loop {
                let probe = hub.clone();
                let snapshot = blocking(move || {
                    let mut hub = probe
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    hub.bridge_snapshot()
                })
                .await;
                if let Ok(snapshot) = snapshot {
                    let current: BTreeSet<String> = snapshot
                        .status
                        .session
                        .as_ref()
                        .map(|session| {
                            session
                                .mods
                                .iter()
                                .map(|entry| entry.guid.clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    let session_changed = current != loaded;
                    if snapshot != last {
                        ui.bridge.set(snapshot.clone());
                        last = snapshot;
                    }
                    if session_changed {
                        loaded = current;
                        ui.rebuild_local_rows();
                    }
                }
                let _ = blocking(|| std::thread::sleep(Duration::from_millis(POLL_MILLIS))).await;
            }
        });
    }
}
