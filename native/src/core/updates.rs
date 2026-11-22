//! Safe release metadata and APK integrity checks for the Rust-native updater.
use sha2::{Digest, Sha256};
use serde_json::Value;

const MAX_RELEASE_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_RELEASE_NOTES_BYTES: usize = 20 * 1024;
pub const MAX_UPDATE_APK_BYTES: usize = 1_500_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    pub download_url: String,
    pub sha256: String,
    pub size: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Version {
    core: [u64; 3],
    prerelease: Option<Vec<String>>,
}

fn parse_version(value: &str) -> Result<Version, &'static str> {
    let value = value.strip_prefix('v').unwrap_or(value);
    if value.len() > 128 || value.is_empty() { return Err("Version identifier is invalid"); }
    let without_build = value.split_once('+').map_or(value, |(head, _)| head);
    let (core, prerelease) = without_build.split_once('-').map_or((without_build, None), |(a, b)| (a, Some(b)));
    let parts = core.split('.').collect::<Vec<_>>();
    if parts.len() != 3 { return Err("Version must use major.minor.patch format"); }
    let mut numbers = [0u64; 3];
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() || (part.len() > 1 && part.starts_with('0')) || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("Version numeric component is invalid");
        }
        numbers[index] = part.parse().map_err(|_| "Version numeric component is out of range")?;
    }
    let prerelease = match prerelease {
        Some(value) => {
            if value.is_empty() { return Err("Version prerelease is empty"); }
            let ids = value.split('.').map(str::to_owned).collect::<Vec<_>>();
            if ids.iter().any(|id| id.is_empty() || id.len() > 32 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                || ids.iter().any(|id| id.len() > 1 && id.starts_with('0') && id.bytes().all(|b| b.is_ascii_digit())) {
                return Err("Version prerelease identifier is invalid");
            }
            Some(ids)
        }
        None => None,
    };
    Ok(Version { core: numbers, prerelease })
}

fn compare_versions(left: &Version, right: &Version) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match left.core.cmp(&right.core) {
        Ordering::Equal => {}, other => return other,
    }
    match (&left.prerelease, &right.prerelease) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => {
            for (x, y) in a.iter().zip(b.iter()) {
                let xn = x.parse::<u64>().ok();
                let yn = y.parse::<u64>().ok();
                let order = match (xn, yn) {
                    (Some(x), Some(y)) => x.cmp(&y),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => x.cmp(y),
                };
                if order != Ordering::Equal { return order; }
            }
            a.len().cmp(&b.len())
        }
    }
}

fn valid_release_download_url(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else { return false; };
    url.scheme() == "https" && url.host_str() == Some("github.com")
        && url.path().starts_with("/C4RL-DANIEL/CenAi/releases/download/")
        && url.username().is_empty() && url.password().is_none()
        && url.query().is_none() && url.fragment().is_none()
}

pub fn parse_latest_release(body: &str, current_version: &str, include_prerelease: bool) -> Result<Option<UpdateInfo>, &'static str> {
    if body.len() > MAX_RELEASE_RESPONSE_BYTES { return Err("Release metadata exceeds the size limit"); }
    let current = parse_version(current_version)?;
    let releases: Value = serde_json::from_str(body).map_err(|_| "GitHub release metadata is invalid")?;
    let list = releases.as_array().ok_or("GitHub release metadata must be an array")?;
    if list.len() > 30 { return Err("GitHub returned too many release records"); }
    for release in list {
        if release.get("draft").and_then(Value::as_bool) != Some(false) { continue; }
        if !include_prerelease && release.get("prerelease").and_then(Value::as_bool) == Some(true) { continue; }
        let tag = release.get("tag_name").and_then(Value::as_str).ok_or("Release tag is missing")?;
        let version = parse_version(tag)?;
        if compare_versions(&version, &current) != std::cmp::Ordering::Greater { continue; }
        let assets = release.get("assets").and_then(Value::as_array).ok_or("Release assets are missing")?;
        let apk = assets.iter().find(|asset| asset.get("name").and_then(Value::as_str)
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".apk")));
        let Some(apk) = apk else { continue; };
        let digest = apk.get("digest").and_then(Value::as_str).unwrap_or_default().strip_prefix("sha256:").unwrap_or_default().to_ascii_lowercase();
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Release APK has no valid SHA-256 digest; installation is blocked");
        }
        let url = apk.get("browser_download_url").and_then(Value::as_str).ok_or("Release APK download URL is missing")?;
        if !valid_release_download_url(url) { return Err("Release APK URL is outside the official CenAI GitHub release path"); }
        let notes = release.get("body").and_then(Value::as_str).unwrap_or_default();
        if notes.len() > MAX_RELEASE_NOTES_BYTES { return Err("Release notes exceed the supported size limit"); }
        let size = apk.get("size").and_then(Value::as_u64);
        if size.is_some_and(|size| size == 0 || size > MAX_UPDATE_APK_BYTES) {
            return Err("Release APK size is outside the supported range");
        }
        return Ok(Some(UpdateInfo { version: tag.trim_start_matches('v').to_owned(), notes: notes.to_owned(), download_url: url.to_owned(), sha256: digest, size }));
    }
    Ok(None)
}

pub fn verify_apk_sha256(bytes: &[u8], expected: &str) -> Result<(), &'static str> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_UPDATE_APK_BYTES { return Err("Downloaded APK size is invalid"); }
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Expected APK SHA-256 digest is invalid");
    }
    let actual = Sha256::digest(bytes);
    let mut mismatch = 0u8;
    for (actual, expected) in actual.iter().zip(expected.as_bytes().chunks_exact(2)) {
        let high = (expected[0] as char).to_digit(16).ok_or("Expected APK SHA-256 digest is invalid")? as u8;
        let low = (expected[1] as char).to_digit(16).ok_or("Expected APK SHA-256 digest is invalid")? as u8;
        mismatch |= *actual ^ ((high << 4) | low);
    }
    if mismatch != 0 { return Err("APK SHA-256 verification failed"); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_versions_order_prereleases_before_stable() {
        let alpha = parse_version("v1.2.3-alpha.2").unwrap();
        let beta = parse_version("1.2.3-beta.1").unwrap();
        let stable = parse_version("1.2.3").unwrap();
        let next = parse_version("1.2.4").unwrap();
        assert!(compare_versions(&alpha, &beta).is_lt());
        assert!(compare_versions(&beta, &stable).is_lt());
        assert!(compare_versions(&stable, &next).is_lt());
    }

    #[test]
    fn release_metadata_requires_official_path_and_digest() {
        let valid = r#"[{"draft":false,"prerelease":false,"tag_name":"v1.2.0","body":"Notes","assets":[{"name":"cenai.apk","digest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","browser_download_url":"https://github.com/C4RL-DANIEL/CenAi/releases/download/v1.2.0/cenai.apk","size":100}]}]"#;
        let update = parse_latest_release(valid, "1.1.0", false).unwrap().unwrap();
        assert_eq!(update.version, "1.2.0");
        let unsafe_url = valid.replace("https://github.com/C4RL-DANIEL/CenAi", "https://attacker.example/C4RL-DANIEL/CenAi");
        assert!(parse_latest_release(&unsafe_url, "1.1.0", false).is_err());
        let missing_digest = valid.replace("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "");
        assert!(parse_latest_release(&missing_digest, "1.1.0", false).is_err());
    }

    #[test]
    fn apk_digest_check_accepts_only_matching_bytes() {
        let bytes = b"test apk bytes";
        let digest = Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        assert!(verify_apk_sha256(bytes, &digest).is_ok());
        assert!(verify_apk_sha256(bytes, &"0".repeat(64)).is_err());
        assert!(verify_apk_sha256(&[], &digest).is_err());
    }
}
