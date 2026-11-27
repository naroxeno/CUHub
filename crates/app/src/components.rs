//! Shared pixel controls, typography and motion.
mod clipboard;
mod fx;
mod widgets;

pub(crate) use clipboard::copy_button;
pub(crate) use fx::{PRESS_SHIFT, STATE_MS, states};
pub(crate) use widgets::{
    PIXEL_FONT, button, choice_button, confirmation_dialog, danger_button, fade_in, focus_ring,
    panel, pix, pix_c, pixel_icon, rise_in, scaled, spinner, text_input,
};
