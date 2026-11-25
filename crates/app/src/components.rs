//! Shared pixel controls, typography and motion.
mod fx;
mod widgets;

pub(crate) use fx::{PRESS_SHIFT, STATE_MS, states};
pub(crate) use widgets::{
    PIXEL_FONT, button, choice_button, danger_button, fade_in, panel, pix, pix_c, pixel_icon,
    rise_in, scaled, spinner, text_input,
};
