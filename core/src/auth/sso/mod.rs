//! OIDC (SSO) support: client, token verification, and PKCE helpers.

pub mod client;
pub mod idtoken;
pub mod pkce;

pub use client::{Discovery, OidcClient, OidcError, TokenResponse};
pub use idtoken::VerifiedIdentity;
