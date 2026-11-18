//! Motion tokens and press feedback shared by the shell's controls.

use nsh_core::prelude::*;

/// How long a control takes to reach its hover or pressed look.
pub const STATE_MS: f32 = 120.0;

/// How far a pressed control sinks, in unscaled pixels.
pub const PRESS_SHIFT: f32 = 1.0;

/// Lets a control's state styles (colors, transform) glide between variants.
pub fn states<S: Styled>(s: S) -> S {
    s.transition().duration(STATE_MS)
}
