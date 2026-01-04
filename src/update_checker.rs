//! Update checker using GitHub Releases API

use anyhow::Result;
use log::{debug, info, warn};
use serde::Deserialize;

const GITHUB_API_URL: &str = "https://api.github.com/repos/ssut/sukusho/releases/latest";
const RELEASES_PAGE_URL: &str = "https://github.com/ssut/sukusho/releases";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    #[allow(dead_code)]
    html_url: String,
}

/// Check for updates from GitHub Releases
/// Returns true if a new version is available
pub fn check_for_updates() -> Result<bool> {
    info!("Checking for updates...");
    debug!("Current version: {}", CURRENT_VERSION);

    // Make request to GitHub API
    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("sukusho/{}", CURRENT_VERSION))
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let response = client.get(GITHUB_API_URL).send()?;

    if !response.status().is_success() {
        warn!("GitHub API returned status: {}", response.status());
        anyhow::bail!("Failed to fetch release information");
    }

    let release: GitHubRelease = response.json()?;
    debug!("Latest release: {}", release.tag_name);

    let latest_version = release.tag_name.trim_start_matches('v');
    let has_update = is_newer_version(CURRENT_VERSION, latest_version);

    if has_update {
        info!("New version available: {} -> {}", CURRENT_VERSION, latest_version);
    } else {
        info!("Already on the latest version");
    }

    Ok(has_update)
}

/// Open the releases page in the default browser
pub fn open_releases_page() {
    info!("Opening releases page: {}", RELEASES_PAGE_URL);
    if let Err(e) = open::that(RELEASES_PAGE_URL) {
        warn!("Failed to open releases page: {}", e);
    }
}

/// Compare two semantic version strings
/// Returns true if `latest` is newer than `current`
fn is_newer_version(current: &str, latest: &str) -> bool {
    let current_parts: Vec<u32> = current
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect();

    let latest_parts: Vec<u32> = latest
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect();

    // Compare each part (major, minor, patch)
    for i in 0..3 {
        let current_part = current_parts.get(i).copied().unwrap_or(0);
        let latest_part = latest_parts.get(i).copied().unwrap_or(0);

        if latest_part > current_part {
            return true;
        } else if latest_part < current_part {
            return false;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison() {
        assert!(is_newer_version("0.1.0", "0.2.0"));
        assert!(is_newer_version("0.1.0", "0.1.1"));
        assert!(is_newer_version("0.1.0", "1.0.0"));
        assert!(!is_newer_version("0.2.0", "0.1.0"));
        assert!(!is_newer_version("0.1.1", "0.1.0"));
        assert!(!is_newer_version("1.0.0", "0.9.9"));
        assert!(!is_newer_version("0.1.0", "0.1.0"));
    }
}
