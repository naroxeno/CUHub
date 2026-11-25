//! Semantic colors, TOML palettes, and reactive bindings for the app surface.

use nsh_core::prelude::*;
use toml::Table;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorRole {
    Bg,
    Surface,
    Raised,
    Border,
    BorderDim,
    Text,
    TextMuted,
    TextDim,
    Accent,
    AccentSurface,
    AccentPress,
    Warn,
    Danger,
    Link,
    Request,
}

pub(crate) use ColorRole::{
    Accent as ACCENT, AccentPress as ACCENT_PRESS, AccentSurface as ACCENT_SURFACE, Bg as BG,
    Border as BORDER, BorderDim as BORDER_DIM, Danger as DANGER, Link as LINK,
    Raised as SURFACE_RAISED, Request as REQUEST, Surface as SURFACE, Text as TEXT,
    TextDim as TEXT_DIM, TextMuted as TEXT_MUTED, Warn as WARN,
};

const COLOR_KEYS: [&str; 15] = [
    "bg",
    "surface",
    "surface_raised",
    "border",
    "border_dim",
    "text",
    "text_muted",
    "text_dim",
    "accent",
    "accent_surface",
    "accent_press",
    "warn",
    "danger",
    "link",
    "request",
];

#[derive(Clone, Copy)]
pub(crate) struct Palette([u32; COLOR_KEYS.len()]);

impl Default for Palette {
    fn default() -> Self {
        Self::built_in(include_str!("../assets/themes/copper.toml"))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Theme {
    Copper,
    Console,
    OneDark,
    TokyoNight,
    Custom,
}

impl Theme {
    pub(crate) const BUILT_INS: [Self; 4] =
        [Self::Copper, Self::Console, Self::OneDark, Self::TokyoNight];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Copper => "copper",
            Self::Console => "console",
            Self::OneDark => "one-dark",
            Self::TokyoNight => "tokyonight",
            Self::Custom => "custom",
        }
    }

    pub(crate) fn label(self) -> String {
        match self {
            Self::Copper => "Copper".into(),
            Self::Console => "Console".into(),
            Self::OneDark => "One Dark".into(),
            Self::TokyoNight => "Tokyo Night".into(),
            Self::Custom => t!("settings.theme_custom"),
        }
    }

    pub(crate) fn available(custom: Option<Palette>) -> impl Iterator<Item = Self> {
        Self::BUILT_INS
            .into_iter()
            .chain(custom.map(|_| Self::Custom))
    }

    pub(crate) fn selected(id: &str, custom: Option<Palette>) -> Self {
        Self::available(custom)
            .find(|theme| theme.id() == id)
            .unwrap_or(Self::Copper)
    }

    pub(crate) fn palette(self, custom: Option<Palette>) -> Palette {
        match self {
            Self::Copper => Palette::default(),
            Self::Console => Palette::built_in(include_str!("../assets/themes/console.toml")),
            Self::OneDark => Palette::built_in(include_str!("../assets/themes/one-dark.toml")),
            Self::TokyoNight => Palette::built_in(include_str!("../assets/themes/tokyonight.toml")),
            Self::Custom => custom.unwrap_or_default(),
        }
    }
}

impl Palette {
    pub(crate) fn custom(raw: &str) -> Result<Self, String> {
        Self::parse(raw, Some(Self::default()))
    }

    fn built_in(raw: &str) -> Self {
        Self::parse(raw, None).expect("embedded theme contains a complete palette")
    }

    fn parse(raw: &str, fallback: Option<Self>) -> Result<Self, String> {
        let document = raw.parse::<Table>().map_err(|error| error.to_string())?;
        let colors = document
            .get("colors")
            .and_then(|value| value.as_table())
            .filter(|_| document.len() == 1)
            .ok_or_else(|| "Expected a single [colors] table.".to_owned())?;
        let mut palette = fallback.map_or([0; COLOR_KEYS.len()], |palette| palette.0);
        for (key, value) in colors {
            let index = COLOR_KEYS
                .iter()
                .position(|name| name == key)
                .ok_or_else(|| format!("Unknown color colors.{key}."))?;
            let hex = value
                .as_str()
                .and_then(|value| value.strip_prefix('#'))
                .filter(|hex| hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .ok_or_else(|| format!("Expected #RRGGBB for colors.{key}."))?;
            palette[index] = u32::from_str_radix(hex, 16).map_err(|error| error.to_string())?;
        }
        if fallback.is_none() && colors.len() != COLOR_KEYS.len() {
            return Err("A built-in theme must define every color.".to_owned());
        }
        Ok(Self(palette))
    }
}

pub(crate) fn apply_theme(palette: Palette) {
    global::<Palette>().set(palette);
}

pub(crate) fn theme_color(role: ColorRole) -> impl Fn() -> AdaptiveColor + Clone {
    theme_color_with(move || role)
}

pub(crate) fn theme_color_with(
    role: impl Fn() -> ColorRole + Clone + 'static,
) -> impl Fn() -> AdaptiveColor + Clone {
    let palette = global::<Palette>();
    move || rgb(palette.get().0[role() as usize]).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_palettes_have_distinct_backgrounds() {
        let backgrounds = Theme::BUILT_INS.map(|theme| theme.palette(None).0[BG as usize]);
        assert_eq!(backgrounds, [0x131415, 0, 0x282C34, 0x1A1B26]);
    }

    #[test]
    fn custom_overrides_are_validated_and_inherit_copper() {
        let palette = Palette::custom("[colors]\naccent = \"#abcdef\"").unwrap();
        assert_eq!(palette.0[ACCENT as usize], 0xABCDEF);
        assert_eq!(palette.0[BG as usize], Palette::default().0[BG as usize]);
        assert!(Palette::custom("[colors]\naccnet = \"#abcdef\"").is_err());
        assert!(Palette::custom("[colors]\naccent = \"#+fffff\"").is_err());
        assert!(Theme::selected("custom", None) == Theme::Copper);
    }

    #[test]
    fn existing_color_bindings_read_the_new_theme() {
        let runtime = nsh_core::element::reactive::Runtime::default();
        let _entered = runtime.enter();
        let background = theme_color(BG);
        let (_owner, selected) = nsh_core::element::reactive::create_root(|| signal(false));
        let control = theme_color_with(move || if selected.get() { ACCENT } else { TEXT });
        assert_eq!(background(), AdaptiveColor::from(rgb(0x131415)));
        apply_theme(Theme::TokyoNight.palette(None));
        assert_eq!(background(), AdaptiveColor::from(rgb(0x1A1B26)));
        assert_eq!(control(), AdaptiveColor::from(rgb(0xC0CAF5)));
        selected.set(true);
        assert_eq!(control(), AdaptiveColor::from(rgb(0x7AA2F7)));
    }
}
