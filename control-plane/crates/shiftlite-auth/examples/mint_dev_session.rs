//! Dev-only utility: mints a signed ShiftLite session JWT without a real IdP, for local
//! end-to-end testing (see docs/TESTING.md). Not used by the production binary.
//!
//! Usage: cargo run -p shiftlite-auth --example mint_dev_session -- <private.pem> <public.pem> [viewer|developer|platform-admin]

use shiftlite_auth::{encode_session, SessionClaims, SigningKeys};
use shiftlite_core::{Provider, Role};
use std::fs;

fn main() {
    let mut args = std::env::args().skip(1);
    let private_path = args
        .next()
        .expect("usage: mint_dev_session <private.pem> <public.pem> [role]");
    let public_path = args
        .next()
        .expect("usage: mint_dev_session <private.pem> <public.pem> [role]");
    let role = match args.next().as_deref() {
        Some("viewer") => Role::Viewer,
        Some("developer") => Role::Developer,
        _ => Role::PlatformAdmin,
    };

    let private_pem = fs::read(&private_path).expect("reading private key");
    let public_pem = fs::read(&public_path).expect("reading public key");
    let keys =
        SigningKeys::from_ed25519_pem(&private_pem, &public_pem).expect("loading signing keys");

    let claims = SessionClaims::new(
        Provider::Keycloak,
        "dev-local",
        "dev@localhost".to_string(),
        vec!["platform-admin".to_string(), "dev-team-a".to_string()],
        vec![role],
    );
    let token = encode_session(&claims, &keys).expect("signing session token");
    println!("{token}");
}
