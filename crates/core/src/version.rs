//! Minimal SemVer reader matching the Hub's published ordering rules.

use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone)]
pub struct HubVersion {
    major: u32,
    minor: u32,
    patch: u32,
    prerelease: Option<String>,
    original: String,
}

impl HubVersion {
    pub fn parse(value: &str) -> Option<Self> {
        let clean = value.split('+').next()?.trim();
        let rest = clean
            .strip_prefix('v')
            .or_else(|| clean.strip_prefix('V'))
            .unwrap_or(clean);
        let (numeric, prerelease) = match rest.split_once('-') {
            Some((numeric, prerelease)) => (numeric, Some(prerelease)),
            None => (rest, None),
        };
        if let Some(prerelease) = prerelease
            && (prerelease.is_empty()
                || !prerelease.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-')
                }))
        {
            return None;
        }
        let mut parts = numeric.split('.');
        let major = parse_component(parts.next()?)?;
        let minor = parse_component(parts.next()?)?;
        let patch = parse_component(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
            prerelease: prerelease.map(str::to_string),
            original: clean.trim_start_matches(['v', 'V']).to_string(),
        })
    }

    pub fn major(&self) -> u32 {
        self.major
    }

    pub fn minor(&self) -> u32 {
        self.minor
    }

    pub fn patch(&self) -> u32 {
        self.patch
    }

    pub fn prerelease(&self) -> Option<&str> {
        self.prerelease.as_deref()
    }

    pub fn is_prerelease(&self) -> bool {
        self.prerelease.is_some()
    }

    pub fn original(&self) -> &str {
        &self.original
    }

    fn compare_prerelease(left: &str, right: &str) -> Ordering {
        let left_parts: Vec<&str> = left.split('.').collect();
        let right_parts: Vec<&str> = right.split('.').collect();
        for index in 0..left_parts.len().max(right_parts.len()) {
            let Some(left_part) = left_parts.get(index) else {
                return Ordering::Less;
            };
            let Some(right_part) = right_parts.get(index) else {
                return Ordering::Greater;
            };
            let left_number = parse_prerelease_number(left_part);
            let right_number = parse_prerelease_number(right_part);
            let ordering = match (left_number, right_number) {
                (Some(left), Some(right)) => left.cmp(&right),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => left_part
                    .to_ascii_lowercase()
                    .cmp(&right_part.to_ascii_lowercase()),
            };
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        Ordering::Equal
    }
}

fn parse_component(value: &str) -> Option<u32> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

fn parse_prerelease_number(value: &str) -> Option<u32> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

impl Ord for HubVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            .then_with(|| match (&self.prerelease, &other.prerelease) {
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (Some(left), Some(right)) => Self::compare_prerelease(left, right),
                (None, None) => Ordering::Equal,
            })
    }
}

impl PartialOrd for HubVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for HubVersion {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for HubVersion {}

impl fmt::Display for HubVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.original)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> HubVersion {
        HubVersion::parse(text).expect("version should parse")
    }

    #[test]
    fn parses_optional_prefix_and_ignores_build_metadata() {
        let parsed = version("v1.2.3+build.7");
        assert_eq!((parsed.major(), parsed.minor(), parsed.patch()), (1, 2, 3));
        assert_eq!(parsed.to_string(), "1.2.3");
        assert_eq!(version("V0.0.8-pre.6.1").to_string(), "0.0.8-pre.6.1");
        assert_eq!(
            version(" 1.0.0 ").prerelease(),
            None,
            "surrounding whitespace is trimmed"
        );
    }

    #[test]
    fn rejects_malformed_versions() {
        for text in [
            "",
            "   ",
            "1.0",
            "1.0.0.0",
            "v1.0",
            "vv1.0.0",
            "1.0.0-",
            "1.0.0-alpha_beta",
            "1.0.0 beta",
            "a1.0.0",
        ] {
            assert!(
                HubVersion::parse(text).is_none(),
                "{text} should be rejected"
            );
        }
    }

    #[test]
    fn orders_numeric_components() {
        assert!(version("1.2.3") > version("1.2.2"));
        assert!(version("1.3.0") > version("1.2.9"));
        assert!(version("2.0.0") > version("1.99.99"));
        assert_eq!(version("1.0.0"), version("v1.0.0"));
    }

    #[test]
    fn release_outranks_same_number_prerelease() {
        assert!(version("1.0.0") > version("1.0.0-rc.1"));
        assert!(version("1.0.0-rc.1") < version("1.0.0"));
    }

    #[test]
    fn orders_prerelease_parts_like_the_hub() {
        assert!(version("1.0.0-alpha") < version("1.0.0-alpha.1"));
        assert!(version("1.0.0-alpha.1") < version("1.0.0-alpha.beta"));
        assert!(version("1.0.0-alpha") < version("1.0.0-beta"));
        assert!(version("1.0.0-1") < version("1.0.0-alpha"));
        assert!(version("1.0.0-10") > version("1.0.0-2"));
        assert_eq!(version("1.0.0-RC.1"), version("1.0.0-rc.1"));
        assert!(version("1.0.0-rc.1") < version("1.0.0-rc.2"));
    }

    #[test]
    fn metadata_comparisons_are_stable() {
        let mut items = vec!["1.0.0", "1.0.0-rc.1", "0.9.9", "1.0.0-alpha", "1.10.0"];
        items.sort_by_key(|text| version(text));
        assert_eq!(
            items,
            vec!["0.9.9", "1.0.0-alpha", "1.0.0-rc.1", "1.0.0", "1.10.0"]
        );
    }
}
