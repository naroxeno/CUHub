//! The fixed pixel palette from FEATURES O1.

#![allow(dead_code)] // Token table mirrors FEATURES O1; tokens are consumed as pages land.

pub const BG: u32 = 0x000000;
pub const SURFACE: u32 = 0x0B0B0B;
pub const SURFACE_RAISED: u32 = 0x141414;
pub const BORDER: u32 = 0xFFFFFF;
pub const BORDER_DIM: u32 = 0x3A3A3A;
pub const TEXT: u32 = 0xF2F2F2;
pub const TEXT_MUTED: u32 = 0xA0A0A0;
pub const TEXT_DIM: u32 = 0x6D6D6D;
pub const ACCENT: u32 = 0x39DE82;
pub const ACCENT_PRESS: u32 = 0x168301;
pub const WARN: u32 = 0xFFF622;
pub const DANGER: u32 = 0xFF0000;
pub const LINK: u32 = 0x00FFFF;
pub const REQUEST: u32 = 0xFF66FF;
