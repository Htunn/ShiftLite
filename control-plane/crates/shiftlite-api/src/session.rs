//! `AuthSession` extractor: validates the `sl_session` cookie on every protected request.

use crate::error::AppError;
use crate::state::AppState;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use shiftlite_auth::{decode_session, Denylist, SessionClaims, SESSION_COOKIE};
use shiftlite_core::{ApiError, Role};

pub struct AuthSession(pub SessionClaims);

impl AuthSession {
    /// Enforces a minimum role (SPEC-001: `viewer < developer < platform-admin`).
    pub fn require_role(&self, min: Role) -> Result<(), AppError> {
        if self.0.roles.iter().any(|r| *r >= min) {
            Ok(())
        } else {
            Err(AppError(ApiError::Forbidden))
        }
    }
}

impl<S> FromRequestParts<S> for AuthSession
where
    AppState: axum::extract::FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .map(|c| c.value().to_string())
            .ok_or(AppError(ApiError::Unauthenticated))?;

        let claims = decode_session(&token, &app_state.signing_keys)
            .map_err(|_| AppError(ApiError::Unauthenticated))?;

        if app_state.denylist.is_denylisted(&claims.jti) {
            return Err(AppError(ApiError::Unauthenticated));
        }
        Ok(AuthSession(claims))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiftlite_core::Provider;

    fn session_with_roles(roles: Vec<Role>) -> AuthSession {
        AuthSession(SessionClaims::new(
            Provider::Keycloak,
            "usr_1",
            "dev@example.com".to_string(),
            vec![],
            roles,
        ))
    }

    #[test]
    fn developer_can_satisfy_a_viewer_requirement() {
        let session = session_with_roles(vec![Role::Developer]);
        assert!(session.require_role(Role::Viewer).is_ok());
    }

    #[test]
    fn viewer_cannot_satisfy_a_developer_requirement() {
        let session = session_with_roles(vec![Role::Viewer]);
        let err = session.require_role(Role::Developer).unwrap_err();
        assert_eq!(err.0.status_code(), 403);
    }

    #[test]
    fn a_user_with_no_roles_is_forbidden_everywhere() {
        let session = session_with_roles(vec![]);
        assert!(session.require_role(Role::Viewer).is_err());
    }

    #[test]
    fn platform_admin_satisfies_every_requirement() {
        let session = session_with_roles(vec![Role::PlatformAdmin]);
        assert!(session.require_role(Role::Viewer).is_ok());
        assert!(session.require_role(Role::Developer).is_ok());
        assert!(session.require_role(Role::PlatformAdmin).is_ok());
    }
}
