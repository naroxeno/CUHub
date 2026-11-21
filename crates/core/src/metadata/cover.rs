//! Community catalogue cover cache and approved Nexus browser destinations.

use std::path::{Path, PathBuf};

use crate::network::{Fetch, Network};

pub fn nexus_public_page(value: &str) -> Option<reqwest::Url> {
    let mut url = reqwest::Url::parse(value).ok()?;
    if url.scheme() != "https"
        || !matches!(url.host_str(), Some("www.nexusmods.com" | "nexusmods.com"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    let mut segments = url.path_segments()?;
    if segments.next()? != "scavprototype"
        || segments.next()? != "mods"
        || !segments.next()?.parse::<u64>().is_ok_and(|id| id > 0)
        || segments.next().is_some_and(|segment| !segment.is_empty())
        || segments.next().is_some()
    {
        return None;
    }
    url.set_query(None);
    url.set_fragment(None);
    Some(url)
}

#[derive(Clone)]
pub struct CoverCache {
    root: PathBuf,
    network: Network,
}

impl CoverCache {
    pub(crate) fn new(app_data: &Path, network: Network) -> Self {
        Self {
            root: app_data.join("IconCache"),
            network,
        }
    }

    pub fn fetch(&self, source: &str) -> Option<String> {
        use sha2::{Digest, Sha256};

        let url = reqwest::Url::parse(source).ok()?;
        if !is_cover_url(&url) {
            return None;
        }
        let digest = format!("{:x}", Sha256::digest(url.as_str()));
        let path = self.root.join(format!("{}.img", &digest[..32]));
        if !std::fs::symlink_metadata(&path).is_ok_and(|entry| entry.is_file()) {
            let Fetch::Fetched { body, .. } = self.network.get_cover(url, 2 * 1024 * 1024).ok()?
            else {
                return None;
            };
            if !(body.starts_with(b"\x89PNG\r\n\x1a\n")
                || body.starts_with(b"\xff\xd8\xff")
                || (body.starts_with(b"RIFF") && body.get(8..12) == Some(b"WEBP")))
            {
                return None;
            }
            std::fs::create_dir_all(&self.root).ok()?;
            crate::atomic::replace(&path, &body).ok()?;
        }
        reqwest::Url::from_file_path(path).ok().map(String::from)
    }
}

pub(crate) fn is_cover_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("staticdelivery.nexusmods.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.path().starts_with("/mods/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_expected_cover_origin() {
        let allowed = reqwest::Url::parse(
            "https://staticdelivery.nexusmods.com/mods/8391/images/67/image.png",
        )
        .unwrap();
        let unsafe_host =
            reqwest::Url::parse("https://staticdelivery.nexusmods.com.invalid/mods/same.png")
                .unwrap();
        let other_path = reqwest::Url::parse(
            "https://staticdelivery.nexusmods.com/mods-not-a-directory/image.png",
        )
        .unwrap();
        assert!(is_cover_url(&allowed));
        assert!(!is_cover_url(&unsafe_host));
        assert!(!is_cover_url(&other_path));
    }

    #[test]
    fn strips_queries_from_approved_mod_pages() {
        let url = nexus_public_page(
            "https://www.nexusmods.com/scavprototype/mods/67?tab=files#description",
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://www.nexusmods.com/scavprototype/mods/67"
        );
        assert!(nexus_public_page("https://evil.example/scavprototype/mods/67").is_none());
        assert!(
            nexus_public_page("https://www.nexusmods.com/scavprototype/mods/67/other").is_none()
        );
    }
}
