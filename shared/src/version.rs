/// Application version from Cargo.toml
/// This is set at compile time from the workspace version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub README URL for upgrade instructions
pub const GITHUB_README_URL: &str = "https://github.com/mikemiles-dev/rust_chat#readme";

/// Check if two versions are compatible
/// For now, versions must match exactly
pub fn versions_compatible(client_version: &str, server_version: &str) -> bool {
    client_version == server_version
}

/// Format version mismatch error message
pub fn version_mismatch_message(client_version: &str, server_version: &str) -> String {
    format!(
        "Version mismatch: client v{} != server v{}. Please upgrade your binary or Docker image. See: {}",
        client_version, server_version, GITHUB_README_URL
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_is_set() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_versions_compatible_same() {
        assert!(versions_compatible("0.1.8", "0.1.8"));
    }

    #[test]
    fn test_versions_compatible_different() {
        assert!(!versions_compatible("0.1.7", "0.1.8"));
    }

    #[test]
    fn test_version_mismatch_message() {
        let msg = version_mismatch_message("0.1.7", "0.1.8");
        assert!(msg.contains("0.1.7"));
        assert!(msg.contains("0.1.8"));
        assert!(msg.contains(GITHUB_README_URL));
    }
}
