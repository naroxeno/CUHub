//! Single-mod upgrade: staging, ownership checks, backup and replacement. No UI.

mod apply;
mod archive;
mod bepinex;
mod plan;
mod process;

pub use apply::{ApplyError, ApplyReport, apply_upgrade};
pub use archive::{
    ArchiveError, ArchiveFormat, MAX_ARCHIVE_BYTES, StagedArchive, StagedFile, is_dll_file,
    stage_archive, stage_dll,
};
pub use bepinex::{
    EXPERIMENTAL, Flavor, HashError, LTS, Mode, Release, Status, detect, ensure_plugins,
    plan_install, release_for, releases, verify_sha256,
};
pub use plan::{
    FileAction, Layout, PlanError, PlanTarget, PlannedFile, UpgradePlan, normalize_install_name,
    plan_first_install, plan_payload, plan_upgrade,
};
pub use process::game_running;
