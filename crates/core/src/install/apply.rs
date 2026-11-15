//! Applies an upgrade plan with backups and rollback. No UI and no network.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::paths;

use super::plan::{FileAction, PlannedFile, UpgradePlan};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyReport {
    pub backup: Option<PathBuf>,
    pub replaced: Vec<PathBuf>,
    pub created: Vec<PathBuf>,
}

#[derive(Debug)]
pub enum ApplyError {
    NotApplicable {
        conflicts: Vec<PathBuf>,
    },
    NoFiles,
    Outside(PathBuf),
    Changed(PathBuf),
    MissingBackup(PathBuf),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Rollback {
        primary: Box<ApplyError>,
        problems: Vec<String>,
    },
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotApplicable { conflicts } => {
                write!(
                    formatter,
                    "{} existing file(s) would be overwritten",
                    conflicts.len()
                )
            }
            Self::NoFiles => formatter.write_str("upgrade plan has no files"),
            Self::Outside(path) => {
                write!(formatter, "{} is outside the target root", path.display())
            }
            Self::Changed(path) => write!(
                formatter,
                "{} changed while the upgrade was queued",
                path.display()
            ),
            Self::MissingBackup(path) => write!(formatter, "backup is missing: {}", path.display()),
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Rollback { primary, problems } => {
                write!(
                    formatter,
                    "{primary}; rollback had {} problem(s)",
                    problems.len()
                )
            }
        }
    }
}

impl std::error::Error for ApplyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Rollback { primary, .. } => Some(primary),
            _ => None,
        }
    }
}

struct Applied {
    target: PathBuf,
    backup: Option<PathBuf>,
}

pub fn apply_upgrade(plan: &UpgradePlan, backup_root: &Path) -> Result<ApplyReport, ApplyError> {
    if !plan.conflicts.is_empty() {
        return Err(ApplyError::NotApplicable {
            conflicts: plan.conflicts.clone(),
        });
    }
    if plan.files.is_empty() {
        return Err(ApplyError::NoFiles);
    }
    for file in &plan.files {
        if !paths::is_inside(&file.target, &plan.target_root) {
            return Err(ApplyError::Outside(file.target.clone()));
        }
    }
    for file in &plan.files {
        check_target(file)?;
    }
    let backup = if plan
        .files
        .iter()
        .any(|file| file.action == FileAction::Replace)
    {
        Some(create_backup(backup_root)?)
    } else {
        None
    };
    if let Some(directory) = &backup {
        for file in &plan.files {
            if file.action == FileAction::Replace {
                backup_one(file, directory, &plan.target_root)?;
            }
        }
    }
    let mut applied = Vec::new();
    for file in &plan.files {
        if let Err(error) = copy_atomic(&file.staged, &file.target) {
            let problems = rollback(&applied, &plan.target_root);
            if problems.is_empty() {
                return Err(error);
            }
            return Err(ApplyError::Rollback {
                primary: Box::new(error),
                problems,
            });
        }
        applied.push(Applied {
            target: file.target.clone(),
            backup: backup.as_ref().and_then(|directory| {
                if file.action == FileAction::Replace {
                    Some(backup_path(directory, &file.target, &plan.target_root))
                } else {
                    None
                }
            }),
        });
    }
    Ok(ApplyReport {
        backup,
        replaced: plan
            .files
            .iter()
            .filter(|file| file.action == FileAction::Replace)
            .map(|file| file.target.clone())
            .collect(),
        created: plan
            .files
            .iter()
            .filter(|file| file.action == FileAction::Create)
            .map(|file| file.target.clone())
            .collect(),
    })
}

fn check_target(file: &PlannedFile) -> Result<(), ApplyError> {
    match fs::symlink_metadata(&file.target) {
        Ok(metadata) if metadata.file_type().is_symlink() || metadata.is_dir() => {
            Err(ApplyError::Changed(file.target.clone()))
        }
        Ok(_) if file.action == FileAction::Create => Err(ApplyError::Changed(file.target.clone())),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if file.action == FileAction::Replace {
                Err(ApplyError::Changed(file.target.clone()))
            } else {
                Ok(())
            }
        }
        Err(source) => Err(ApplyError::Io {
            path: file.target.clone(),
            source,
        }),
    }
}

fn create_backup(backup_root: &Path) -> Result<PathBuf, ApplyError> {
    fs::create_dir_all(backup_root).map_err(|source| ApplyError::Io {
        path: backup_root.to_path_buf(),
        source,
    })?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let mut directory = backup_root.join(&stamp);
    let mut suffix = 1;
    while directory.exists() {
        directory = backup_root.join(format!("{stamp}-{suffix}"));
        suffix += 1;
    }
    fs::create_dir(&directory).map_err(|source| ApplyError::Io {
        path: directory.clone(),
        source,
    })?;
    Ok(directory)
}

fn backup_one(file: &PlannedFile, directory: &Path, target_root: &Path) -> Result<(), ApplyError> {
    let destination = backup_path(directory, &file.target, target_root);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|source| ApplyError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::copy(&file.target, &destination).map_err(|source| ApplyError::Io {
        path: file.target.clone(),
        source,
    })?;
    Ok(())
}

fn backup_path(directory: &Path, target: &Path, target_root: &Path) -> PathBuf {
    let relative = target.strip_prefix(target_root).unwrap_or(target);
    directory.join(relative)
}

fn copy_atomic(source: &Path, target: &Path) -> Result<(), ApplyError> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| ApplyError::Io {
            path: parent.to_path_buf(),
            source: error,
        })?;
    }
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let temporary = target.with_file_name(format!(
        ".{name}.cuhub-{}-{counter}.tmp",
        std::process::id()
    ));
    let result = (|| -> Result<(), ApplyError> {
        let mut input = File::open(source).map_err(|error| ApplyError::Io {
            path: source.to_path_buf(),
            source: error,
        })?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| ApplyError::Io {
                path: temporary.clone(),
                source: error,
            })?;
        io::copy(&mut input, &mut output).map_err(|error| ApplyError::Io {
            path: temporary.clone(),
            source: error,
        })?;
        output.sync_all().map_err(|error| ApplyError::Io {
            path: temporary.clone(),
            source: error,
        })?;
        drop(output);
        fs::rename(&temporary, target).map_err(|error| ApplyError::Io {
            path: target.to_path_buf(),
            source: error,
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn rollback(applied: &[Applied], target_root: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    for item in applied.iter().rev() {
        match &item.backup {
            Some(backup) => {
                if let Err(error) = fs::copy(backup, &item.target) {
                    problems.push(format!("restore {}: {error}", item.target.display()));
                }
            }
            None => {
                match fs::remove_file(&item.target) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => {
                        problems.push(format!("remove {}: {error}", item.target.display()))
                    }
                }
                remove_empty_parents(&item.target, target_root);
            }
        }
    }
    problems
}

fn remove_empty_parents(target: &Path, target_root: &Path) {
    let Some(mut directory) = target.parent() else {
        return;
    };
    while directory != target_root && paths::is_inside(directory, target_root) {
        if fs::remove_dir(directory).is_err() {
            break;
        }
        let Some(parent) = directory.parent() else {
            break;
        };
        directory = parent;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::test_support::temp_dir;

    use super::*;
    use crate::install::plan::{FileAction, Layout, PlannedFile, UpgradePlan};

    fn plan(root: &Path, files: Vec<PlannedFile>) -> UpgradePlan {
        UpgradePlan {
            layout: Layout::Mod,
            target_root: root.to_path_buf(),
            files,
            conflicts: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn applies_create_and_replace_and_keeps_a_backup() {
        let dir = temp_dir("apply-basic");
        let game = dir.join("game");
        let staging = dir.join("staging");
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::write(game.join("Old.dll"), b"old").unwrap();
        fs::write(staging.join("Old.dll"), b"new").unwrap();
        fs::write(staging.join("New.dll"), b"added").unwrap();
        let report = apply_upgrade(
            &plan(
                &game,
                vec![
                    PlannedFile {
                        staged: staging.join("Old.dll"),
                        target: game.join("Old.dll"),
                        action: FileAction::Replace,
                        size: 3,
                    },
                    PlannedFile {
                        staged: staging.join("New.dll"),
                        target: game.join("New.dll"),
                        action: FileAction::Create,
                        size: 5,
                    },
                ],
            ),
            &dir.join("Backups"),
        )
        .unwrap();
        assert_eq!(fs::read(game.join("Old.dll")).unwrap(), b"new");
        assert_eq!(fs::read(game.join("New.dll")).unwrap(), b"added");
        let backup = report.backup.unwrap();
        assert_eq!(fs::read(backup.join("Old.dll")).unwrap(), b"old");
        assert_eq!(report.replaced, vec![game.join("Old.dll")]);
        assert_eq!(report.created, vec![game.join("New.dll")]);
    }

    #[test]
    fn conflicts_are_refused_without_writing() {
        let dir = temp_dir("apply-conflict");
        let game = dir.join("game");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join("Old.dll"), b"old").unwrap();
        let mut value = plan(&game, Vec::new());
        value.conflicts.push(game.join("Old.dll"));
        let error = apply_upgrade(&value, &dir.join("Backups")).unwrap_err();
        assert!(matches!(error, ApplyError::NotApplicable { .. }));
        assert_eq!(fs::read(game.join("Old.dll")).unwrap(), b"old");
    }

    #[test]
    fn a_failed_file_rolls_back_the_earlier_replacement() {
        let dir = temp_dir("apply-rollback");
        let game = dir.join("game");
        let staging = dir.join("staging");
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::write(game.join("First.dll"), b"first-old").unwrap();
        fs::write(game.join("Second.dll"), b"second-old").unwrap();
        fs::write(staging.join("First.dll"), b"first-new").unwrap();
        let error = apply_upgrade(
            &plan(
                &game,
                vec![
                    PlannedFile {
                        staged: staging.join("First.dll"),
                        target: game.join("First.dll"),
                        action: FileAction::Replace,
                        size: 9,
                    },
                    PlannedFile {
                        staged: staging.join("Missing.dll"),
                        target: game.join("Second.dll"),
                        action: FileAction::Replace,
                        size: 9,
                    },
                ],
            ),
            &dir.join("Backups"),
        )
        .unwrap_err();
        assert!(matches!(error, ApplyError::Io { .. }));
        assert_eq!(fs::read(game.join("First.dll")).unwrap(), b"first-old");
        assert_eq!(fs::read(game.join("Second.dll")).unwrap(), b"second-old");
    }
}
