//! Cross-platform safe filename slugification.
//!
//! Ensures group names and connection names map to valid directory and file
//! names on Windows, macOS, and Linux without path traversal or reserved names.

/// Windows reserved device filenames (case-insensitive) that cannot be used
/// as bare file or folder names on NTFS/FAT32.
const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Converts an arbitrary user-supplied name into a filesystem-safe slug.
///
/// - Replaces any non-ASCII-alphanumeric character with `_` (matching v1's
///   `str.replace(/[^a-z0-9]/gi, '_')` behavior for normal names).
/// - Strips trailing dots and spaces (forbidden on Windows).
/// - Prefixes `_` if the result matches a Windows reserved device name.
/// - Falls back to `"unnamed"` if the resulting slug is empty.
pub fn slugify(input: &str) -> String {
    let mut out: String = input
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();

    // Windows forbids trailing dots or spaces in filenames.
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }

    if out.is_empty() {
        return "unnamed".to_string();
    }

    // Check against Windows reserved device names (case-insensitive).
    let upper = out.to_ascii_uppercase();
    if WINDOWS_RESERVED.contains(&upper.as_str()) {
        return format!("_{out}");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_matches_v1_for_standard_names() {
        assert_eq!(
            slugify("Finxam - Finance Management"),
            "Finxam___Finance_Management"
        );
        assert_eq!(slugify("MySaas"), "MySaas");
    }

    #[test]
    fn slugify_prevents_path_traversal_and_reserved_names() {
        assert_eq!(slugify("../../etc/passwd"), "______etc_passwd");
        assert_eq!(slugify("CON"), "_CON");
        assert_eq!(slugify("nul"), "_nul");
        assert_eq!(slugify("LPT1"), "_LPT1");
        assert_eq!(slugify("   "), "___");
        assert_eq!(slugify(""), "unnamed");
    }
}
