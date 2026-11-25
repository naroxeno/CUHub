//! GitHub token handling.

use super::state::Ui;
use nsh_core::prelude::*;

impl Ui {
    pub fn save_github_token(&self) {
        let token = self.github_token_draft.get();
        if token.trim().is_empty() {
            self.github_token_check.set(t!("notice.paste_github_token"));
            self.notice.set(t!("notice.paste_github_token"));
            return;
        }
        let result = {
            let mut hub = self.hub();
            hub.save_github_token(&token)
        };
        match result {
            Ok(()) => {
                self.github_token_saved.set(true);
                self.github_token_draft.set(String::new());
                self.github_token_check.set(t!("notice.github_token_saved"));
                self.notice.set(t!("notice.github_token_saved"));
            }
            Err(error) => {
                let message = t!("notice.save_github_token_failed", error = error);
                self.github_token_check.set(message.clone());
                self.notice.set(message);
            }
        }
    }

    pub fn validate_github_token(&self) {
        if !self.github_token_saved.get() {
            self.github_token_check.set(t!("notice.no_token_saved"));
            self.notice.set(t!("notice.save_github_token_first"));
            return;
        }
        self.github_token_check.set(t!("notice.checking_github"));
        let hub = self.hub.clone();
        let saved = self.github_token_saved;
        let check = self.github_token_check;
        let notice = self.notice;
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.validate_github_token()
            })
            .await;
            match result {
                Ok(Ok(limit)) => {
                    saved.set(true);
                    check.set(t!(
                        "notice.github_token_accepted",
                        remaining = limit.remaining,
                        limit = limit.limit
                    ));
                    notice.set(t!(
                        "notice.github_token_accepted_notice",
                        remaining = limit.remaining,
                        limit = limit.limit
                    ));
                }
                Ok(Err(error)) => {
                    check.set(t!("notice.github_check_failed", error = error));
                    notice.set(t!("notice.check_github_token_failed", error = error));
                }
                Err(error) => {
                    check.set(t!("notice.github_check_unfinished"));
                    notice.set(t!("notice.check_github_token_failed", error = error));
                }
            }
        });
    }

    pub fn clear_github_token(&self) {
        let result = {
            let mut hub = self.hub();
            hub.clear_github_token()
        };
        match result {
            Ok(()) => {
                self.github_token_saved.set(false);
                self.github_token_draft.set(String::new());
                self.github_token_check
                    .set(t!("notice.github_token_cleared"));
                self.notice.set(t!("notice.github_token_cleared"));
            }
            Err(error) => {
                let message = t!("notice.clear_github_token_failed", error = error);
                self.github_token_check.set(message.clone());
                self.notice.set(message);
            }
        }
    }
}
