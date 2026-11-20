//! OIDC (SSO) support: client, token verification, and PKCE helpers.

pub mod client;
pub mod idtoken;
pub mod pkce;
pub mod rate_limit;
pub mod routes;
pub mod runtime;

pub use client::{Discovery, OidcCache, OidcClient, OidcError, TokenResponse};
pub use idtoken::VerifiedIdentity;
pub use runtime::{SsoRuntime, now_secs};
