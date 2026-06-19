//! Integration test: the real `OidcRelyingParty` performs a complete authorization-code +
//! PKCE round-trip against a real OIDC server (navikt/mock-oauth2-server), including
//! discovery, code exchange, and ID-token validation (signature via JWKS, plus nonce).
//!
//! Ignored by default — it needs the mock running. Locally:
//!   docker run -d --rm -p 9090:8080 ghcr.io/navikt/mock-oauth2-server:2.1.10
//!   cargo test -p authbox-inf --test oidc_mock -- --ignored
//! In CI the issuer is overridden via `OIDC_MOCK_ISSUER` (the `oidc-mock` service).

use authbox_core::auth::{FederationConnection, OidcPort};
use authbox_inf::OidcRelyingParty;

#[tokio::test]
#[ignore = "requires mock-oauth2-server (set OIDC_MOCK_ISSUER or run on localhost:9090)"]
async fn full_auth_code_flow_against_mock_oidc() {
    let issuer = std::env::var("OIDC_MOCK_ISSUER")
        .unwrap_or_else(|_| "http://localhost:9090/default".to_owned());
    let connection = FederationConnection {
        slug: "mock".to_owned(),
        issuer: issuer.clone(),
        client_id: "test-client".to_owned(),
        client_secret: "test-secret".to_owned(),
        scopes: vec!["openid".to_owned(), "email".to_owned()],
        redirect_uri: "http://localhost/cb".to_owned(),
        auto_link_verified_email: false,
        require_email_verified: true,
    };
    let relying_party = OidcRelyingParty::new(vec![connection]);

    // 1. Begin: discovery + authorization URL with a fresh PKCE challenge and nonce.
    let request = relying_party.begin("mock").await.expect("begin login");
    assert!(request.authorize_url.contains("code_challenge"));
    assert!(request.authorize_url.contains("state="));

    // 2. Drive the mock's login: POST a username (and the email claims to embed) to the
    //    authorize URL, which 302s back to the redirect URI with an authorization code.
    let http = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = http
        .post(&request.authorize_url)
        .form(&[
            ("username", "ada@example.com"),
            (
                "claims",
                r#"{"email":"ada@example.com","email_verified":true,"name":"Ada Lovelace"}"#,
            ),
        ])
        .send()
        .await
        .expect("drive mock login");
    let location = response
        .headers()
        .get("location")
        .expect("redirect with code")
        .to_str()
        .unwrap();
    let code = location
        .split("code=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .to_owned();

    // 3. Complete: exchange the code (with the PKCE verifier) and validate the ID token
    //    (signature via the mock's JWKS, plus the nonce). This is the security-critical path.
    let claims = relying_party
        .complete("mock", &code, &request.pkce_verifier, &request.nonce)
        .await
        .expect("complete login");

    assert_eq!(claims.issuer, issuer);
    assert_eq!(claims.subject, "ada@example.com");
    assert_eq!(claims.email.as_deref(), Some("ada@example.com"));
    assert!(claims.email_verified);

    // A tampered nonce must be rejected by the validator.
    let tampered = relying_party
        .complete("mock", &code, &request.pkce_verifier, "wrong-nonce")
        .await;
    assert!(tampered.is_err(), "a wrong nonce must fail validation");
}

#[tokio::test]
#[ignore = "requires mock-oauth2-server (set OIDC_MOCK_ISSUER or run on localhost:9090)"]
async fn entra_shaped_token_uses_preferred_username_as_email() {
    // Microsoft Entra commonly issues an ID token carrying the address in `preferred_username`
    // (the UPN) and omits the `email`/`email_verified` claims. The adapter must surface that
    // UPN as the email so the enterprise connection can match a pre-provisioned user.
    let issuer = std::env::var("OIDC_MOCK_ISSUER")
        .unwrap_or_else(|_| "http://localhost:9090/default".to_owned());
    let connection = FederationConnection {
        slug: "mock".to_owned(),
        issuer: issuer.clone(),
        client_id: "test-client".to_owned(),
        client_secret: "test-secret".to_owned(),
        scopes: vec!["openid".to_owned(), "email".to_owned()],
        redirect_uri: "http://localhost/cb".to_owned(),
        auto_link_verified_email: true,
        require_email_verified: false,
    };
    let relying_party = OidcRelyingParty::new(vec![connection]);

    let request = relying_party.begin("mock").await.expect("begin login");

    let http = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = http
        .post(&request.authorize_url)
        .form(&[
            ("username", "ada@corp.onmicrosoft.com"),
            // No `email`/`email_verified` — only the UPN, as Entra sends.
            (
                "claims",
                r#"{"preferred_username":"ada@corp.onmicrosoft.com","name":"Ada Lovelace"}"#,
            ),
        ])
        .send()
        .await
        .expect("drive mock login");
    let location = response
        .headers()
        .get("location")
        .expect("redirect with code")
        .to_str()
        .unwrap();
    let code = location
        .split("code=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .to_owned();

    let claims = relying_party
        .complete("mock", &code, &request.pkce_verifier, &request.nonce)
        .await
        .expect("complete login");

    assert_eq!(claims.email.as_deref(), Some("ada@corp.onmicrosoft.com"));
    assert!(!claims.email_verified, "Entra omits email_verified");
}
