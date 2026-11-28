use super::*;
use flate2::{Compression, write::GzEncoder};
use std::io::Write;

const FIRST: &str = "https://gcdn.thunderstore.io/live/blob-storage/sha256/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.sha256.blob";
const SECOND: &str = "https://ccdn.thunderstore.io/live/blob-storage/sha256/b4c78de96554057fa88cd1cac22cad2c92a835de752d470eab9bc5417580c698.sh_rIhIbFl.blob";

const INDEX_FIXTURE: &str = r#"[{
    "owner":"Example",
    "name":"TestMod",
    "full_name":"Example-TestMod",
    "has_nsfw_content":false,
    "versions":[
        {"version_number":"1.2.0","description":"older","dependencies":[],
         "file_size":500,"download_url":"https://thunderstore.io/package/download/Example/TestMod/1.2.0/"},
        {"version_number":"1.10.0","description":"newer","dependencies":["Other-Team-Library-1.0.0"],
         "file_size":500,"download_url":"https://thunderstore.io/package/download/Example/TestMod/1.10.0/"}
    ]
}]"#;

#[test]
fn caches_exact_package_versions_and_sorts_numerically() {
    let dir = crate::test_support::temp_dir("thunderstore-catalogue");
    let catalogue = Catalogue::new(&dir);
    crate::atomic::replace(&catalogue.path, INDEX_FIXTURE.as_bytes()).unwrap();
    let packages = catalogue.cached().unwrap();
    assert_eq!(packages[0].mod_id(), "thunderstore.example-testmod");
    assert_eq!(packages[0].versions[0].version_number, "1.10.0");
    assert_eq!(
        dependency("Other-Team-Library-1.0.0"),
        Some(("Other-Team-Library", "1.0.0"))
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rejects_a_foreign_download_and_an_incomplete_index() {
    let foreign = INDEX_FIXTURE.replace("thunderstore.io", "thunderstore.example");
    assert!(parse_index(foreign.as_bytes()).is_err());
    assert!(parse_index(&INDEX_FIXTURE.as_bytes()[..30]).is_err());
}

fn gzip(body: &[u8]) -> Vec<u8> {
    let mut writer = GzEncoder::new(Vec::new(), Compression::fast());
    writer.write_all(body).unwrap();
    writer.finish().unwrap()
}

#[test]
fn merges_gzip_and_http_decoded_chunks_before_publishing_the_catalogue() {
    let dir = crate::test_support::temp_dir("thunderstore-chunks");
    let catalogue = Catalogue::new(&dir);
    let packages = catalogue
        .refresh_with(|url, _| match url {
            INDEX => Ok(gzip(&serde_json::to_vec(&[FIRST, SECOND]).unwrap())),
            FIRST => Ok(gzip(INDEX_FIXTURE.as_bytes())),
            SECOND => Ok(INDEX_FIXTURE.replace("TestMod", "SecondMod").into_bytes()),
            _ => panic!("unexpected URL: {url}"),
        })
        .unwrap();
    assert_eq!(packages.len(), 2);
    assert_eq!(packages[0].full_name, "Example-SecondMod");
    assert_eq!(packages[1].full_name, "Example-TestMod");
    assert_eq!(catalogue.cached().unwrap(), packages);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_missing_invalid_or_repeated_package_chunk_keeps_the_last_complete_cache() {
    let dir = crate::test_support::temp_dir("thunderstore-stale-chunks");
    let catalogue = Catalogue::new(&dir);
    crate::atomic::replace(&catalogue.path, INDEX_FIXTURE.as_bytes()).unwrap();
    for second in [
        None,
        Some(vec![0x1f, 0x8b]),
        Some(gzip(INDEX_FIXTURE.as_bytes())),
    ] {
        let result = catalogue.refresh_with(|url, _| match url {
            INDEX => Ok(serde_json::to_vec(&[FIRST, SECOND]).unwrap()),
            FIRST => Ok(gzip(INDEX_FIXTURE.as_bytes())),
            SECOND => second
                .clone()
                .ok_or_else(|| io::Error::other("unavailable chunk")),
            _ => panic!("unexpected URL: {url}"),
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&catalogue.path).unwrap(), INDEX_FIXTURE.as_bytes());
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rejects_foreign_or_non_blob_chunk_urls_and_bounds_gzip_output() {
    for invalid_url in [
        "http://gcdn.thunderstore.io/live/blob-storage/sha256/test.sha256.blob",
        "https://gcdn.thunderstore.io.example/live/blob-storage/sha256/test.sha256.blob",
        "https://gcdn.thunderstore.io/live/blob-storage/sha256/%2Ftest.sha256.blob",
        "https://gcdn.thunderstore.io/live/blob-storage/sha256/folder/test.sha256.blob",
        "https://gcdn.thunderstore.io/live/blob-storage/sha256/test.sha256.blob?unexpected=secret",
        "https://thunderstore.io/package/download/Example/TestMod/1.2.0/",
    ] {
        assert!(chunk_urls(&serde_json::to_vec(&[invalid_url]).unwrap()).is_err());
    }
    for invalid_url in [
        SECOND.replace("ccdn.thunderstore.io", "ccdn.thunderstore.io.example"),
        SECOND.replace("/sha256/", "/sha256/folder/"),
        SECOND.replace(".sh_rIhIbFl", "%2Fother"),
        format!("{SECOND}?unexpected=secret"),
    ] {
        assert!(chunk_urls(&serde_json::to_vec(&[invalid_url]).unwrap()).is_err());
    }
    assert!(chunk_urls(&serde_json::to_vec(&[FIRST, FIRST]).unwrap()).is_err());
    let error = chunk_urls(
        &serde_json::to_vec(&[FIRST, "https://gcdn.thunderstore.io/invalid?secret"]).unwrap(),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("chunk 2"));
    assert!(error.contains("gcdn.thunderstore.io"));
    assert!(!error.contains("secret"));
    assert!(decode_blob(&gzip(b"12345"), 4, FIRST).is_err());
    assert_eq!(decode_blob(&gzip(b"1234"), 4, FIRST).unwrap(), b"1234");
}
