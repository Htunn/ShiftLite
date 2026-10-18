//! `/api/v1/auth/*` handlers (ADR-002): OIDC/OAuth2 login, callback, logout, current user.

use crate::error::{ApiResult, AppError};
use crate::session::AuthSession;
use crate::state::{AppState, OAuthProviderConfig};
use crate::types::UserInfo;
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use jsonwebtoken::{decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use serde_json::Value;
use shiftlite_auth::{
    encode_oauth_cookie, encode_session, generate_pkce, new_id, resolve_roles, OAuthCookieClaims,
    SessionClaims, OAUTH_COOKIE, OAUTH_COOKIE_LIFETIME_SECS, SESSION_COOKIE,
    SESSION_MAX_LIFETIME_SECS,
};
use shiftlite_core::{ApiError, Provider};
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct LoginParams {
    pub provider: String,
}

fn parse_provider(s: &str) -> Result<Provider, AppError> {
    match s {
        "github" => Ok(Provider::Github),
        "entraid" => Ok(Provider::Entraid),
        "keycloak" => Ok(Provider::Keycloak),
        "rhsso" => Ok(Provider::Rhsso),
        _ => Err(AppError(ApiError::InvalidProvider)),
    }
}

fn session_cookie(value: String, max_age: i64) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, value))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(max_age))
        .build()
}

fn oauth_cookie(value: String) -> Cookie<'static> {
    Cookie::build((OAUTH_COOKIE, value))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(OAUTH_COOKIE_LIFETIME_SECS))
        .build()
}

/// `302 Found` redirect (api/openapi.yaml documents 302, not axum's default 303 See Other).
fn found(url: &str) -> Response {
    (axum::http::StatusCode::FOUND, [(header::LOCATION, url)]).into_response()
}

pub async fn login(
    State(state): State<AppState>,
    Query(params): Query<LoginParams>,
) -> ApiResult<impl IntoResponse> {
    let provider = parse_provider(&params.provider)?;
    let cfg = state
        .oauth_providers
        .get(&provider)
        .ok_or(AppError(ApiError::IdpUnavailable))?;

    let oauth_state = new_id();
    let nonce = cfg.issuer.as_ref().map(|_| new_id());
    let pkce = generate_pkce();

    let mut url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        cfg.authorize_url,
        urlencoding(&cfg.client_id),
        urlencoding(&cfg.redirect_uri),
        urlencoding(&cfg.scopes.join(" ")),
        urlencoding(&oauth_state),
        urlencoding(&pkce.challenge),
    );
    if let Some(n) = &nonce {
        url.push_str(&format!("&nonce={}", urlencoding(n)));
    }

    let cookie_claims =
        OAuthCookieClaims::new(provider, oauth_state, nonce, pkce.verifier, "/".to_string());
    let cookie_value = encode_oauth_cookie(&cookie_claims, &state.signing_keys)
        .map_err(|_| AppError(ApiError::Internal))?;
    let jar = CookieJar::new().add(oauth_cookie(cookie_value));

    Ok((jar, found(&url)))
}

#[derive(Deserialize)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

fn urlencoding(s: &str) -> String {
    url_escape::encode_component(s).into_owned()
}

mod url_escape {
    /// Minimal percent-encoding for query values (avoids pulling in a full `url` crate dependency
    /// beyond what `reqwest` already vendors transitively).
    pub fn encode_component(s: &str) -> std::borrow::Cow<'_, str> {
        let needs_escape = s
            .bytes()
            .any(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~')));
        if !needs_escape {
            return std::borrow::Cow::Borrowed(s);
        }
        let mut out = String::with_capacity(s.len() * 3);
        for b in s.bytes() {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                out.push(b as char);
            } else {
                out.push_str(&format!("%{b:02X}"));
            }
        }
        std::borrow::Cow::Owned(out)
    }
}

async fn validate_id_token(
    http: &reqwest::Client,
    cfg: &OAuthProviderConfig,
    id_token: &str,
    expected_nonce: Option<&str>,
) -> Result<(String, String, Vec<String>), AppError> {
    let jwks_url = cfg
        .jwks_url
        .as_ref()
        .ok_or(AppError(ApiError::IdpValidationFailed))?;
    let issuer = cfg
        .issuer
        .as_ref()
        .ok_or(AppError(ApiError::IdpValidationFailed))?;

    let header = decode_header(id_token).map_err(|_| AppError(ApiError::IdpValidationFailed))?;
    let kid = header.kid.ok_or(AppError(ApiError::IdpValidationFailed))?;

    let jwks: Value = http
        .get(jwks_url)
        .send()
        .await
        .map_err(|_| AppError(ApiError::IdpUnavailable))?
        .json()
        .await
        .map_err(|_| AppError(ApiError::IdpUnavailable))?;

    let key = jwks["keys"]
        .as_array()
        .and_then(|keys| keys.iter().find(|k| k["kid"] == kid))
        .ok_or(AppError(ApiError::IdpValidationFailed))?;
    let n = key["n"]
        .as_str()
        .ok_or(AppError(ApiError::IdpValidationFailed))?;
    let e = key["e"]
        .as_str()
        .ok_or(AppError(ApiError::IdpValidationFailed))?;
    let decoding_key = DecodingKey::from_rsa_components(n, e)
        .map_err(|_| AppError(ApiError::IdpValidationFailed))?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(std::slice::from_ref(&cfg.client_id));
    validation.set_issuer(std::slice::from_ref(issuer));

    let data = jsonwebtoken::decode::<Value>(id_token, &decoding_key, &validation)
        .map_err(|_| AppError(ApiError::IdpValidationFailed))?;
    let claims = data.claims;

    if let Some(expected) = expected_nonce {
        if claims.get("nonce").and_then(Value::as_str) != Some(expected) {
            return Err(AppError(ApiError::IdpValidationFailed));
        }
    }

    let sub = claims["sub"].as_str().unwrap_or_default().to_string();
    let email = claims["email"].as_str().unwrap_or_default().to_string();
    let groups = claims["groups"]
        .as_array()
        .map(|g| {
            g.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    Ok((sub, email, groups))
}

async fn fetch_github_identity(
    http: &reqwest::Client,
    access_token: &str,
) -> Result<(String, String, Vec<String>), AppError> {
    let user: Value = http
        .get("https://api.github.com/user")
        .bearer_auth(access_token)
        .header("User-Agent", "shiftlite")
        .send()
        .await
        .map_err(|_| AppError(ApiError::IdpUnavailable))?
        .json()
        .await
        .map_err(|_| AppError(ApiError::IdpExchangeFailed))?;
    let sub = user["id"]
        .as_u64()
        .map(|i| i.to_string())
        .unwrap_or_default();

    let emails: Value = http
        .get("https://api.github.com/user/emails")
        .bearer_auth(access_token)
        .header("User-Agent", "shiftlite")
        .send()
        .await
        .map_err(|_| AppError(ApiError::IdpUnavailable))?
        .json()
        .await
        .unwrap_or(Value::Array(vec![]));
    let email = emails
        .as_array()
        .and_then(|list| list.iter().find(|e| e["primary"] == true))
        .and_then(|e| e["email"].as_str())
        .unwrap_or_default()
        .to_string();

    let teams: Value = http
        .get("https://api.github.com/user/teams")
        .bearer_auth(access_token)
        .header("User-Agent", "shiftlite")
        .send()
        .await
        .map_err(|_| AppError(ApiError::IdpUnavailable))?
        .json()
        .await
        .unwrap_or(Value::Array(vec![]));
    let groups = teams
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|t| {
                    let org = t["organization"]["login"].as_str()?;
                    let slug = t["slug"].as_str()?;
                    Some(format!("{org}/{slug}"))
                })
                .collect()
        })
        .unwrap_or_default();

    Ok((sub, email, groups))
}

pub async fn callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
    jar: CookieJar,
) -> ApiResult<impl IntoResponse> {
    if let Some(err) = params.error {
        tracing::warn!(error = %err, "idp returned an error");
        let jar = jar.remove(Cookie::from(OAUTH_COOKIE));
        return Ok((jar, found(&format!("/login?error={err}"))));
    }
    let code = params.code.ok_or(AppError(ApiError::InvalidState))?;
    let returned_state = params.state.ok_or(AppError(ApiError::InvalidState))?;

    let oauth_cookie_value = jar
        .get(OAUTH_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or(AppError(ApiError::InvalidState))?;
    let cookie_claims =
        shiftlite_auth::decode_oauth_cookie(&oauth_cookie_value, &state.signing_keys)
            .map_err(|_| AppError(ApiError::InvalidState))?;
    if cookie_claims.state != returned_state {
        return Err(AppError(ApiError::InvalidState));
    }

    let cfg = state
        .oauth_providers
        .get(&cookie_claims.provider)
        .ok_or(AppError(ApiError::IdpUnavailable))?;

    let mut form = HashMap::new();
    form.insert("grant_type", "authorization_code");
    form.insert("code", code.as_str());
    form.insert("redirect_uri", cfg.redirect_uri.as_str());
    form.insert("client_id", cfg.client_id.as_str());
    form.insert("client_secret", cfg.client_secret.as_str());
    form.insert("code_verifier", cookie_claims.pkce_verifier.as_str());

    let body = form
        .iter()
        .map(|(k, v)| format!("{}={}", urlencoding(k), urlencoding(v)))
        .collect::<Vec<_>>()
        .join("&");

    let token_response: Value = state
        .http
        .post(&cfg.token_url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|_| AppError(ApiError::IdpExchangeFailed))?
        .json()
        .await
        .map_err(|_| AppError(ApiError::IdpExchangeFailed))?;

    let access_token = token_response["access_token"]
        .as_str()
        .ok_or(AppError(ApiError::IdpExchangeFailed))?;

    let (idp_subject, email, groups) = if cfg.issuer.is_some() {
        let id_token = token_response["id_token"]
            .as_str()
            .ok_or(AppError(ApiError::IdpValidationFailed))?;
        validate_id_token(&state.http, cfg, id_token, cookie_claims.nonce.as_deref()).await?
    } else {
        fetch_github_identity(&state.http, access_token).await?
    };

    let roles = resolve_roles(&groups, &state.role_mappings);
    let claims = SessionClaims::new(cookie_claims.provider, &idp_subject, email, groups, roles);
    let session_token =
        encode_session(&claims, &state.signing_keys).map_err(|_| AppError(ApiError::Internal))?;

    let jar = jar
        .remove(Cookie::from(OAUTH_COOKIE))
        .add(session_cookie(session_token, SESSION_MAX_LIFETIME_SECS));

    Ok((jar, found(&cookie_claims.redirect_to)))
}

pub async fn logout(
    State(state): State<AppState>,
    session: AuthSession,
    jar: CookieJar,
) -> ApiResult<impl IntoResponse> {
    let expires_at =
        chrono::DateTime::from_timestamp(session.0.exp, 0).unwrap_or_else(chrono::Utc::now);
    let _ = state.denylist.revoke(&session.0.jti, expires_at).await;
    let jar = jar.remove(Cookie::from(SESSION_COOKIE));
    Ok((jar, axum::http::StatusCode::NO_CONTENT))
}

pub async fn me(session: AuthSession) -> Json<UserInfo> {
    Json(UserInfo {
        sub: session.0.sub,
        email: session.0.email,
        provider: session.0.provider,
        groups: session.0.groups,
        roles: session.0.roles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_provider_accepts_all_four_providers() {
        assert!(matches!(parse_provider("github"), Ok(Provider::Github)));
        assert!(matches!(parse_provider("entraid"), Ok(Provider::Entraid)));
        assert!(matches!(parse_provider("keycloak"), Ok(Provider::Keycloak)));
        assert!(matches!(parse_provider("rhsso"), Ok(Provider::Rhsso)));
    }

    #[test]
    fn parse_provider_rejects_unknown_values() {
        let err = parse_provider("okta").unwrap_err();
        assert_eq!(err.0.code(), "invalid_provider");
    }

    #[test]
    fn urlencoding_leaves_unreserved_characters_untouched() {
        assert_eq!(urlencoding("abc-123_. ~"), "abc-123_.%20~");
    }

    #[test]
    fn urlencoding_escapes_redirect_uri_query_values() {
        let encoded = urlencoding("http://localhost:8080/api/v1/auth/callback");
        assert_eq!(
            encoded,
            "http%3A%2F%2Flocalhost%3A8080%2Fapi%2Fv1%2Fauth%2Fcallback"
        );
    }
}
