//! Single-mod upgrade: staging, ownership checks, backup and replacement. No UI.

mod apply;
mod archive;
mod plan;
mod process;

pub use apply::{ApplyError, ApplyReport, apply_upgrade};
pub use archive::{
    ArchiveError, ArchiveFormat, MAX_ARCHIVE_BYTES, StagedArchive, StagedFile, stage_archive,
};
pub use plan::{FileAction, Layout, PlanError, PlanTarget, PlannedFile, UpgradePlan, plan_upgrade};
pub use process::game_running;
