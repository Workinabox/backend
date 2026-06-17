//! OIDC relying-party adapter: the real [`OidcPort`] implementation over the
//! `openidconnect` crate.
//!
//! `begin` discovers the connection's issuer, builds the authorization URL with a fresh
//! PKCE challenge + nonce, and returns them (the app persists them keyed by `state`).
//! `complete` exchanges the code with the stored PKCE verifier and validates the returned
//! ID token (signature via the issuer's JWKS, plus `iss`/`aud`/`exp` and the stored nonce),
//! then maps the claims to [`VerifiedClaims`]. The library handles the security-critical
//! token validation; this adapter is just configuration + plumbing.

use std::collections::HashMap;

use authbox_core::auth::{AuthError, AuthRequest, FederationConnection, OidcPort, VerifiedClaims};
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};

pub struct OidcRelyingParty {
    connections: HashMap<String, FederationConnection>,
    http: reqwest::Client,
}

fn fail(message: impl std::fmt::Display) -> AuthError {
    AuthError::FederationFailed(message.to_string())
}

impl OidcRelyingParty {
    pub fn new(connections: Vec<FederationConnection>) -> Self {
        // Never auto-follow redirects: the IdP's auth endpoint 302s, and an SSRF-hardened
        // client should not chase those server-side.
        let http = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("build reqwest client");
        Self {
            connections: connections
                .into_iter()
                .map(|connection| (connection.slug.clone(), connection))
                .collect(),
            http,
        }
    }

    fn connection(&self, slug: &str) -> Result<FederationConnection, AuthError> {
        self.connections
            .get(slug)
            .cloned()
            .ok_or_else(|| fail(format!("unknown connection '{slug}'")))
    }

    /// Discover the issuer and build a relying-party client for the connection.
    async fn client(
        &self,
        connection: &FederationConnection,
    ) -> Result<
        CoreClient<
            openidconnect::EndpointSet,
            openidconnect::EndpointNotSet,
            openidconnect::EndpointNotSet,
            openidconnect::EndpointNotSet,
            openidconnect::EndpointMaybeSet,
            openidconnect::EndpointMaybeSet,
        >,
        AuthError,
    > {
        let issuer = IssuerUrl::new(connection.issuer.clone()).map_err(fail)?;
        let metadata = CoreProviderMetadata::discover_async(issuer, &self.http)
            .await
            .map_err(|error| fail(format!("OIDC discovery failed: {error}")))?;
        let redirect = RedirectUrl::new(connection.redirect_uri.clone()).map_err(fail)?;
        Ok(CoreClient::from_provider_metadata(
            metadata,
            ClientId::new(connection.client_id.clone()),
            Some(ClientSecret::new(connection.client_secret.clone())),
        )
        .set_redirect_uri(redirect))
    }
}

impl OidcPort for OidcRelyingParty {
    async fn begin(&self, connection: &str) -> Result<AuthRequest, AuthError> {
        let connection = self.connection(connection)?;
        let client = self.client(&connection).await?;

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let mut builder = client.authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        );
        for scope in &connection.scopes {
            builder = builder.add_scope(Scope::new(scope.clone()));
        }
        let (url, csrf_token, nonce) = builder.set_pkce_challenge(pkce_challenge).url();

        Ok(AuthRequest {
            authorize_url: url.to_string(),
            state: csrf_token.secret().clone(),
            nonce: nonce.secret().clone(),
            pkce_verifier: pkce_verifier.secret().clone(),
        })
    }

    async fn complete(
        &self,
        connection: &str,
        code: &str,
        pkce_verifier: &str,
        expected_nonce: &str,
    ) -> Result<VerifiedClaims, AuthError> {
        let connection = self.connection(connection)?;
        let client = self.client(&connection).await?;

        let token_response = client
            .exchange_code(AuthorizationCode::new(code.to_owned()))
            .map_err(fail)?
            .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier.to_owned()))
            .request_async(&self.http)
            .await
            .map_err(|error| fail(format!("code exchange failed: {error}")))?;

        let id_token = token_response
            .id_token()
            .ok_or_else(|| fail("the token response had no id_token"))?;
        let verifier = client.id_token_verifier();
        let claims = id_token
            .claims(&verifier, &Nonce::new(expected_nonce.to_owned()))
            .map_err(|error| fail(format!("id token validation failed: {error}")))?;

        Ok(VerifiedClaims {
            issuer: claims.issuer().to_string(),
            subject: claims.subject().to_string(),
            email: claims.email().map(|email| email.as_str().to_owned()),
            email_verified: claims.email_verified().unwrap_or(false),
            name: claims
                .name()
                .and_then(|name| name.get(None))
                .map(|name| name.as_str().to_owned()),
        })
    }
}
