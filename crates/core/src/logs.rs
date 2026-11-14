//! Session log files, in-memory tail and crash reports. I/O failures are swallowed.

use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Local;

const MAX_ENTRIES: usize = 500;
const KEPT_FILES: usize = 5;

pub struct Logs {
    crash_dir: PathBuf,
    session: PathBuf,
    entries: Mutex<VecDeque<String>>,
}

impl Logs {
    pub fn start(app_data: &Path) -> Self {
        let log_dir = app_data.join("Logs");
        let crash_dir = app_data.join("Crash Reports");
        let _ = fs::create_dir_all(&log_dir);
        let _ = fs::create_dir_all(&crash_dir);
        Self::prune(&log_dir);
        Self::prune(&crash_dir);
        let session = log_dir.join(format!(
            "Log {}.log",
            Local::now().format("%Y-%m-%d_%H-%M-%S")
        ));
        Self {
            crash_dir,
            session,
            entries: Mutex::new(VecDeque::new()),
        }
    }

    pub fn session_path(&self) -> &Path {
        &self.session
    }

    pub fn entries(&self) -> Vec<String> {
        self.lock().iter().cloned().collect()
    }

    pub fn info(&self, message: &str) {
        self.write("INFO", message);
    }

    pub fn activity(&self, area: &str, message: &str) {
        self.write("ACTIVITY", &format!("[{area}] {message}"));
    }

    pub fn error(&self, message: &str, error: &dyn std::error::Error) {
        self.write("ERROR", &format!("{message}: {error}"));
    }

    pub fn create_crash_report(&self, game_snapshot: &str, panic: Option<&str>) -> Option<PathBuf> {
        fs::create_dir_all(&self.crash_dir).ok()?;
        let file = self.crash_dir.join(format!(
            "Crash Report {}.log",
            Local::now().format("%Y-%m-%d_%H-%M-%S")
        ));
        let mut report = String::new();
        report.push_str("CUHub crash report\n");
        report.push_str(&format!("Created: {}\n", Local::now().to_rfc3339()));
        report.push_str(&format!("Version: {}\n", env!("CARGO_PKG_VERSION")));
        report.push_str(&format!(
            "OS: {} {}\n",
            std::env::consts::OS,
            std::env::consts::ARCH
        ));
        report.push_str("\nGame and mods:\n");
        report.push_str(game_snapshot);
        if !game_snapshot.ends_with('\n') {
            report.push('\n');
        }
        report.push_str("\nSession log:\n");
        for line in self.entries().into_iter().rev() {
            report.push_str(&line);
            report.push('\n');
        }
        if let Some(panic) = panic {
            report.push_str("\nPanic:\n");
            report.push_str(panic);
            if !panic.ends_with('\n') {
                report.push('\n');
            }
        }
        fs::write(&file, report).ok()?;
        Self::prune(&self.crash_dir);
        Some(file)
    }

    fn write(&self, level: &str, message: &str) {
        let line = format!("[{}] {level}  {message}", Local::now().format("%H:%M:%S"));
        {
            let mut entries = self.lock();
            if entries.len() == MAX_ENTRIES {
                entries.pop_back();
            }
            entries.push_front(line.clone());
        }
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.session)
        {
            let _ = writeln!(file, "{line}");
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<String>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn prune(dir: &Path) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
            .flatten()
            .filter(|entry| {
                entry
                    .file_type()
                    .map(|kind| kind.is_file())
                    .unwrap_or(false)
            })
            .filter_map(|entry| {
                let modified = entry.metadata().ok()?.modified().ok()?;
                Some((modified, entry.path()))
            })
            .collect();
        if files.len() <= KEPT_FILES {
            return;
        }
        files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
        for (_, path) in files.split_off(KEPT_FILES) {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    #[test]
    fn writes_session_lines_and_keeps_newest_first() {
        let dir = temp_dir("logs-write");
        let logs = Logs::start(&dir);
        logs.info("hello");
        logs.activity("mods", "loaded");
        let entries = logs.entries();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].contains("ACTIVITY  [mods] loaded"));
        assert!(entries[1].contains("INFO  hello"));
        let raw = fs::read_to_string(logs.session_path()).unwrap();
        assert!(raw.contains("INFO  hello"));
        assert!(raw.contains("ACTIVITY  [mods] loaded"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn error_includes_the_source_message() {
        let dir = temp_dir("logs-error");
        let logs = Logs::start(&dir);
        logs.error("save failed", &std::io::Error::other("denied"));
        assert!(logs.entries()[0].contains("ERROR  save failed: denied"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn session_logs_are_pruned_to_five() {
        let dir = temp_dir("logs-prune");
        let logs_dir = dir.join("Logs");
        fs::create_dir_all(&logs_dir).unwrap();
        for day in 1..=7 {
            fs::write(
                logs_dir.join(format!("Log 2020-01-0{day}_00-00-00.log")),
                "old",
            )
            .unwrap();
        }
        let _ = Logs::start(&dir);
        let count = fs::read_dir(&logs_dir)
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .map(|ext| ext == "log")
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(count, KEPT_FILES);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn crash_report_contains_snapshot_and_log() {
        let dir = temp_dir("logs-crash");
        let logs = Logs::start(&dir);
        logs.info("ready");
        let path = logs
            .create_crash_report("Game version 1.0", Some("boom"))
            .unwrap();
        let raw = fs::read_to_string(path).unwrap();
        assert!(raw.contains("Game version 1.0"));
        assert!(raw.contains("INFO  ready"));
        assert!(raw.contains("boom"));
        let _ = fs::remove_dir_all(dir);
    }
}
