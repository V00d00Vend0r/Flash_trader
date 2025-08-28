use lazy_static::lazy_static;
use regex::Regex;

/// Sanitize an input string by removing potentially dangerous characters.
///
/// This function performs conservative filtering on untrusted strings
/// before they are used in contexts such as logging, file names, or
/// command execution. It removes any characters that are not
/// alphanumeric, whitespace, hyphen, underscore, or period.
/// Consecutive whitespace characters are collapsed into a single space.
///
/// # Examples
///
/// ```
/// use raveslinger::utils::security::sanitize;
/// let input = "<script>alert('x')</script> /etc/passwd";
/// let out = sanitize(input);
/// assert_eq!(out, "script alert x script etc passwd");
/// ```
pub fn sanitize(s: &str) -> String {
    lazy_static! {
        static ref DISALLOWED_CHARS: Regex = Regex::new(r"[^A-Za-z0-9\s._-]").unwrap();
        static ref MULTISPACE: Regex = Regex::new(r"\s+").unwrap();
    }

    let cleaned = DISALLOWED_CHARS.replace_all(s, " ");
    let normalized = MULTISPACE.replace_all(cleaned.trim(), " ");
    normalized.to_string()
}