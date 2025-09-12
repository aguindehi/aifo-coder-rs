/*!
Authorization and protocol validation helpers for the proxy.
*/

use std::collections::HashMap;

/// Supported shim protocol versions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Proto {
    V1,
    V2,
}

#[cfg(test)]
mod tests {
    use super::authorization_value_matches;

    #[test]
    fn auth_bearer_basic() {
        assert!(authorization_value_matches("Bearer tok", "tok"));
    }

    #[test]
    fn auth_bearer_case_whitespace() {
        assert!(authorization_value_matches("bearer    tok", "tok"));
        assert!(authorization_value_matches("BEARER tok", "tok"));
    }

    #[test]
    fn auth_bearer_punct_rejected() {
        assert!(!authorization_value_matches("Bearer \"tok\"", "tok"));
        assert!(!authorization_value_matches("Bearer tok,", "tok"));
        assert!(!authorization_value_matches("'Bearer tok';", "tok"));
    }

    #[test]
    fn auth_bare_token_rejected() {
        assert!(!authorization_value_matches("tok", "tok"));
    }

    #[test]
    fn auth_wrong() {
        assert!(!authorization_value_matches("Bearer nope", "tok"));
        assert!(!authorization_value_matches("Basic tok", "tok"));
        assert!(!authorization_value_matches("nearlytok", "tok"));
    }
}

/// Result of validating Authorization and X-Aifo-Proto
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthResult {
    Authorized { proto: Proto },
    MissingOrInvalidAuth,
    MissingOrInvalidProto,
}

/// Return true when an Authorization header value authorizes the given token
/// using the standard Bearer scheme (RFC 6750).
/// Accepts:
/// - "Bearer <token>" (scheme case-insensitive; at least one ASCII whitespace
///   separating scheme and credentials)
pub(crate) fn authorization_value_matches(value: &str, token: &str) -> bool {
    let v = value.trim();
    // Split at the first ASCII whitespace to separate scheme and credentials
    if let Some(idx) = v.find(|c: char| c.is_ascii_whitespace()) {
        let (scheme, rest) = v.split_at(idx);
        if scheme.eq_ignore_ascii_case("bearer") {
            let cred = rest.trim();
            return !cred.is_empty() && cred == token;
        }
    }
    false
}

/// Validate Authorization and X-Aifo-Proto against expectations,
/// returning a tri-state indicating whether we are authorized and which proto applies.
pub(crate) fn validate_auth_and_proto(headers: &HashMap<String, String>, token: &str) -> AuthResult {
    let mut auth_ok = false;
    if let Some(v) = headers.get("authorization") {
        if authorization_value_matches(v, token) {
            auth_ok = true;
        }
    }
    if !auth_ok {
        return AuthResult::MissingOrInvalidAuth;
    }
    // Authorized: now require a valid proto header (1 or 2)
    let ver = headers.get("x-aifo-proto").map(|s| s.trim().to_string());
    match ver.as_deref() {
        Some("1") => AuthResult::Authorized { proto: Proto::V1 },
        Some("2") => AuthResult::Authorized { proto: Proto::V2 },
        _ => AuthResult::MissingOrInvalidProto,
    }
}
