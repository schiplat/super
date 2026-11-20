//! In-memory view of the SSO configuration, with revision-based reload.
//!
//! The Web API writes the DB and updates this view in the same request, so a
//! config change applies immediately. A background poll (and SIGHUP) covers
//! out-of-process writers — the CLI's `--offline` mode or an operator editing
//! the database by hand.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, RwLock};

use anyhow::Context;
use common::SsoConfig;

use super::client::{OidcCache, OidcClient};
use crate::auth::store::AuthStore;

pub struct SsoRuntime {
    store: Arc<AuthStore>,
    config: RwLock<Option<Arc<SsoConfig>>>,
    revision: AtomicI64,
    /// Discovery/JWKS cache shared by every per-login client.
    cache: RwLock<Arc<OidcCache>>,
}

impl SsoRuntime {
    /// Load the configuration once. `Ok` with a `None` config means the store
    /// has no SSO row yet (SSO never enabled) — not an error.
    pub async fn load(store: Arc<AuthStore>) -> anyhow::Result<Self> {
        let stored = store
            .load_config()
            .await
            .context("load SSO configuration")?;
        let revision = stored.as_ref().map(|s| s.revision).unwrap_or(0);
        Ok(Self {
            store,
            config: RwLock::new(stored.map(|s| Arc::new(s.config))),
            revision: AtomicI64::new(revision),
            cache: RwLock::new(Arc::new(OidcCache::default())),
        })
    }

    pub fn store(&self) -> &AuthStore {
        self.store.as_ref()
    }

    pub fn config(&self) -> Option<Arc<SsoConfig>> {
        self.config
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn enabled(&self) -> bool {
        self.config().is_some_and(|config| config.enabled)
    }

    pub fn cache(&self) -> Arc<OidcCache> {
        self.cache.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Adopt a configuration written by this process (API path).
    pub fn set_config(&self, config: Arc<SsoConfig>, revision: i64) {
        self.adopt(config);
        self.revision.store(revision, Ordering::Relaxed);
    }

    /// Replace the in-memory config, discarding cached keys when the identity
    /// of the IdP changed (a new issuer's keys are not the old issuer's keys).
    fn adopt(&self, config: Arc<SsoConfig>) {
        let reset_cache = self
            .config()
            .is_none_or(|old| old.issuer != config.issuer || old.client_id != config.client_id);
        if reset_cache {
            *self.cache.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(OidcCache::default());
        }
        *self.config.write().unwrap_or_else(|p| p.into_inner()) = Some(config);
    }

    /// Re-read when another process bumped the revision. `Ok(false)` means
    /// nothing changed. On error the previous configuration is kept, so a
    /// transient read failure never degrades into "no rules".
    pub async fn reload(&self) -> anyhow::Result<bool> {
        self.reload_with(false).await
    }

    /// Unconditional re-read, for SIGHUP.
    pub async fn reload_force(&self) -> anyhow::Result<bool> {
        self.reload_with(true).await
    }

    async fn reload_with(&self, force: bool) -> anyhow::Result<bool> {
        let revision = self
            .store
            .config_revision()
            .await
            .context("read SSO config revision")?;
        if !force && revision == self.revision.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let stored = self
            .store
            .load_config()
            .await
            .context("reload SSO configuration")?;
        match stored {
            Some(stored) => self.adopt(Arc::new(stored.config)),
            None => {
                *self.config.write().unwrap_or_else(|p| p.into_inner()) = None;
            }
        }
        self.revision.store(revision, Ordering::Relaxed);
        Ok(true)
    }

    /// Build a client for the current config, loading the secret (and the
    /// previous one while its rotation window is open) from the store.
    pub async fn client(&self) -> anyhow::Result<OidcClient> {
        let config = self
            .config()
            .ok_or_else(|| anyhow::anyhow!("SSO is not configured"))?;
        let secret = self
            .store
            .load_secret()
            .await?
            .ok_or_else(|| anyhow::anyhow!("SSO client secret is not set"))?;
        let previous = secret.previous_if_valid(now_secs()).map(str::to_string);
        OidcClient::with_cache(config, secret.current().to_string(), previous, self.cache())
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn prune(&self) -> anyhow::Result<()> {
        self.store.prune_ephemeral(now_secs()).await
    }
}

pub fn now_secs() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}
