//! OIDC/OAuth2 login, session JWTs, and role mapping (ADR-002).

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use kubeweave_core::{Provider, Role};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const SESSION_COOKIE: &str = "kw_session";
pub const OAUTH_COOKIE: &str = "kw_oauth";
pub const SESSION_MAX_LIFETIME_SECS: i64 = 8 * 60 * 60;
pub const OAUTH_COOKIE_LIFETIME_SECS: i64 = 5 * 60;
pub const MAX_GROUPS: usize = 100;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("token expired or invalid signature")]
    InvalidToken,
    #[error("token is denylisted")]
    Denylisted,
}

/// Claims embedded in the `kw_session` cookie (SPEC-001 Data Model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionClaims {
    pub sub: String,
    pub email: String,
    pub provider: Provider,
    pub groups: Vec<String>,
    pub roles: Vec<Role>,
    pub jti: String,
    pub iat: i64,
    pub exp: i64,
}

impl SessionClaims {
    pub fn new(
        provider: Provider,
        idp_subject: &str,
        email: String,
        mut groups: Vec<String>,
        roles: Vec<Role>,
    ) -> Self {
        groups.truncate(MAX_GROUPS);
        let now = chrono::Utc::now().timestamp();
        Self {
            sub: format!("{}:{}", provider_tag(provider), idp_subject),
            email,
            provider,
            groups,
            roles,
            jti: new_id(),
            iat: now,
            exp: now + SESSION_MAX_LIFETIME_SECS,
        }
    }

    pub fn has_role(&self, role: Role) -> bool {
        self.roles.contains(&role)
    }
}

fn provider_tag(p: Provider) -> &'static str {
    match p {
        Provider::Github => "github",
        Provider::Entraid => "entraid",
        Provider::Keycloak => "keycloak",
        Provider::Rhsso => "rhsso",
    }
}

/// Transient data carried in the signed `kw_oauth` cookie between login and callback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCookieClaims {
    pub provider: Provider,
    pub state: String,
    pub nonce: Option<String>,
    pub pkce_verifier: String,
    pub redirect_to: String,
    pub iat: i64,
    pub exp: i64,
}

impl OAuthCookieClaims {
    pub fn new(
        provider: Provider,
        state: String,
        nonce: Option<String>,
        pkce_verifier: String,
        redirect_to: String,
    ) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            provider,
            state,
            nonce,
            pkce_verifier,
            redirect_to,
            iat: now,
            exp: now + OAUTH_COOKIE_LIFETIME_SECS,
        }
    }
}

/// Keypair used to sign/verify both cookies. EdDSA (Ed25519) per ADR-002.
pub struct SigningKeys {
    pub encoding: EncodingKey,
    pub decoding: DecodingKey,
}

impl SigningKeys {
    pub fn from_ed25519_pem(
        private_pem: &[u8],
        public_pem: &[u8],
    ) -> Result<Self, jsonwebtoken::errors::Error> {
        Ok(Self {
            encoding: EncodingKey::from_ed_pem(private_pem)?,
            decoding: DecodingKey::from_ed_pem(public_pem)?,
        })
    }
}

fn header() -> Header {
    Header::new(Algorithm::EdDSA)
}

fn validation() -> Validation {
    let mut v = Validation::new(Algorithm::EdDSA);
    v.validate_exp = true;
    v
}

pub fn encode_session(
    claims: &SessionClaims,
    keys: &SigningKeys,
) -> Result<String, jsonwebtoken::errors::Error> {
    encode(&header(), claims, &keys.encoding)
}

pub fn decode_session(token: &str, keys: &SigningKeys) -> Result<SessionClaims, AuthError> {
    decode::<SessionClaims>(token, &keys.decoding, &validation())
        .map(|data| data.claims)
        .map_err(|_| AuthError::InvalidToken)
}

pub fn encode_oauth_cookie(
    claims: &OAuthCookieClaims,
    keys: &SigningKeys,
) -> Result<String, jsonwebtoken::errors::Error> {
    encode(&header(), claims, &keys.encoding)
}

pub fn decode_oauth_cookie(
    token: &str,
    keys: &SigningKeys,
) -> Result<OAuthCookieClaims, AuthError> {
    decode::<OAuthCookieClaims>(token, &keys.decoding, &validation())
        .map(|data| data.claims)
        .map_err(|_| AuthError::InvalidToken)
}

/// Checks whether a session's `jti` has been revoked. Implemented by the caller against the
/// Kubernetes `Lease`-backed denylist (SPEC-001 Resolved Decisions); kept as a trait so
/// `kubeweave-auth` does not depend on `kube`.
pub trait Denylist {
    fn is_denylisted(&self, jti: &str) -> bool;
}

/// Generates a URL-safe random string suitable for `state`/`nonce` values.
pub fn new_id() -> String {
    let mut bytes = [0u8; 24];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// PKCE (RFC 7636) S256 verifier/challenge pair.
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

pub fn generate_pkce() -> Pkce {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());
    Pkce {
        verifier,
        challenge,
    }
}

/// Maps an IdP group to a KubeWeave role (control plane config, SPEC-001 Data Model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleMapping {
    pub group: String,
    pub role: Role,
    #[serde(default)]
    pub namespaces: Vec<String>,
}

/// Resolves the highest-privilege roles for a user from their IdP groups. Users whose groups
/// map to nothing get no roles, and are rejected with `403 forbidden` at the API layer.
pub fn resolve_roles(groups: &[String], mappings: &[RoleMapping]) -> Vec<Role> {
    let mut roles: BTreeSet<Role> = BTreeSet::new();
    for mapping in mappings {
        if groups.iter().any(|g| g == &mapping.group) {
            roles.insert(mapping.role);
        }
    }
    roles.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_deterministic_for_verifier() {
        let pkce = generate_pkce();
        let mut hasher = Sha256::new();
        hasher.update(pkce.verifier.as_bytes());
        let expected = URL_SAFE_NO_PAD.encode(hasher.finalize());
        assert_eq!(pkce.challenge, expected);
    }

    #[test]
    fn resolve_roles_dedups_and_matches_groups() {
        let mappings = vec![
            RoleMapping {
                group: "platform-admin".into(),
                role: Role::PlatformAdmin,
                namespaces: vec![],
            },
            RoleMapping {
                group: "dev-team-a".into(),
                role: Role::Developer,
                namespaces: vec!["team-a".into()],
            },
        ];
        let roles = resolve_roles(&["dev-team-a".to_string(), "other".to_string()], &mappings);
        assert_eq!(roles, vec![Role::Developer]);
    }

    #[test]
    fn unmapped_groups_yield_no_roles() {
        let roles = resolve_roles(&["random-group".to_string()], &[]);
        assert!(roles.is_empty());
    }

    /// Generates a throwaway Ed25519 keypair with the `openssl` CLI (a documented project
    /// prerequisite — see README "Prerequisites" and `make keys`) so session/OAuth-cookie JWT
    /// round trips can be tested without committing a fixture keypair.
    fn test_signing_keys() -> SigningKeys {
        let dir = std::env::temp_dir().join(format!("kubeweave-auth-test-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let private_path = dir.join("private.pem");
        let public_path = dir.join("public.pem");

        let status = std::process::Command::new("openssl")
            .args(["genpkey", "-algorithm", "ed25519", "-out"])
            .arg(&private_path)
            .status()
            .expect("openssl must be installed (see README Prerequisites)");
        assert!(status.success());

        let status = std::process::Command::new("openssl")
            .args(["pkey", "-in"])
            .arg(&private_path)
            .args(["-pubout", "-out"])
            .arg(&public_path)
            .status()
            .unwrap();
        assert!(status.success());

        let keys = SigningKeys::from_ed25519_pem(
            &std::fs::read(&private_path).unwrap(),
            &std::fs::read(&public_path).unwrap(),
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        keys
    }

    #[test]
    fn session_jwt_round_trips_through_encode_and_decode() {
        let keys = test_signing_keys();
        let claims = SessionClaims::new(
            Provider::Rhsso,
            "usr_90210",
            "dev@company.com".to_string(),
            vec!["platform-admin".to_string()],
            vec![Role::PlatformAdmin],
        );
        let jti = claims.jti.clone();

        let token = encode_session(&claims, &keys).unwrap();
        let decoded = decode_session(&token, &keys).unwrap();

        assert_eq!(decoded.sub, "rhsso:usr_90210");
        assert_eq!(decoded.jti, jti);
        assert_eq!(decoded.roles, vec![Role::PlatformAdmin]);
    }

    #[test]
    fn tampered_session_jwt_fails_to_decode() {
        let keys = test_signing_keys();
        let claims = SessionClaims::new(
            Provider::Github,
            "usr_1",
            "a@b.com".to_string(),
            vec![],
            vec![Role::Viewer],
        );
        let mut token = encode_session(&claims, &keys).unwrap();
        token.push('x'); // corrupt the signature
        assert!(decode_session(&token, &keys).is_err());
    }

    #[test]
    fn oauth_cookie_claims_round_trip() {
        let keys = test_signing_keys();
        let claims = OAuthCookieClaims::new(
            Provider::Keycloak,
            "state-123".to_string(),
            Some("nonce-456".to_string()),
            "verifier".to_string(),
            "/".to_string(),
        );
        let token = encode_oauth_cookie(&claims, &keys).unwrap();
        let decoded = decode_oauth_cookie(&token, &keys).unwrap();
        assert_eq!(decoded.state, "state-123");
        assert_eq!(decoded.nonce.as_deref(), Some("nonce-456"));
    }

    #[test]
    fn new_id_produces_unique_url_safe_values() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b);
        assert!(a
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
    }
}
