//! EasyTier release pinning, cached downloads and on-disk detection. No processes, no UI.

mod detect;
mod release;

pub use detect::{Detection, Status, detect_custom, detect_managed};
pub use release::{ReleaseError, VERSION, ensure_archive};
