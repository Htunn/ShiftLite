//! Input validation helpers applied at the API boundary (SPEC-001 Acceptance Criteria).

pub fn is_dns1123_label(s: &str) -> bool {
    if s.is_empty() || s.len() > 63 {
        return false;
    }
    let bytes = s.as_bytes();
    let starts_ok = bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit();
    let ends_ok = {
        let last = bytes[bytes.len() - 1];
        last.is_ascii_lowercase() || last.is_ascii_digit()
    };
    let body_ok = bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-');
    starts_ok && ends_ok && body_ok
}

pub fn is_https_git_url(s: &str) -> bool {
    s.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_dns1123_labels() {
        assert!(is_dns1123_label("auth-service"));
        assert!(!is_dns1123_label("Auth_Service"));
        assert!(!is_dns1123_label(""));
        assert!(!is_dns1123_label("-leading-dash"));
    }

    #[test]
    fn validates_https_git_urls() {
        assert!(is_https_git_url("https://github.com/org/app.git"));
        assert!(!is_https_git_url("git@github.com:org/app.git"));
    }
}
