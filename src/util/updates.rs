use std::process::Command;

pub fn check_for_split_happens_update() -> bool {
    // Use the system curl binary so Steam Deck users do not need a native TLS stack
    if let Ok(output) = Command::new("curl")
        .args([
            "-sSf",
            "-H",
            "User-Agent: split-happens",
            "https://api.github.com/repos/blckink/suckmydeck/releases/latest",
        ])
        .output()
    {
        if output.status.success() {
            if let Ok(release) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                // Extract the tag name (vX.X.X format)
                if let Some(tag_name) = release["tag_name"].as_str() {
                    // Strip the 'v' prefix
                    let latest_version = tag_name.strip_prefix('v').unwrap_or(tag_name);

                    // Get current version from env!
                    let current_version = env!("CARGO_PKG_VERSION");

                    // Compare versions using semver
                    if let (Ok(latest_semver), Ok(current_semver)) = (
                        semver::Version::parse(latest_version),
                        semver::Version::parse(current_version),
                    ) {
                        return latest_semver > current_semver;
                    }
                }
            }
        } else if !output.stderr.is_empty() {
            // Surface curl's stderr when the request itself fails so developers can debug network issues locally
            eprintln!(
                "Split Happens update check failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    // Default to false if any part of the process fails
    false
}

// Self updater for portable version will eventually go here
