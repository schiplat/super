import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { getSuperConfig } from '@/lib/superConfig';

export interface LicenseSnapshot {
  grants: string[];
  plugin_versions?: Record<string, string>;
  [key: string]: unknown;
}

interface AuthStatusBody {
  token_management?: boolean;
  auth_secret_login_allowed?: boolean;
}

/** Probe without axios interceptors (avoids 401 → /login during discovery). */
async function probeStatus(path: string): Promise<number> {
  try {
    const headers: Record<string, string> = { Accept: 'application/json' };
    const token = localStorage.getItem('super_token');
    if (token) headers.Authorization = `Bearer ${token}`;
    const res = await fetch(path, { method: 'GET', headers, credentials: 'same-origin' });
    return res.status;
  } catch {
    return 0;
  }
}

async function probeAuthStatus(): Promise<{ status: number; body: AuthStatusBody | null }> {
  try {
    const headers: Record<string, string> = { Accept: 'application/json' };
    const token = localStorage.getItem('super_token');
    if (token) headers.Authorization = `Bearer ${token}`;
    const res = await fetch('/api/v1/auth/status', {
      method: 'GET',
      headers,
      credentials: 'same-origin',
    });
    if (res.status === 200) {
      try {
        return { status: 200, body: (await res.json()) as AuthStatusBody };
      } catch {
        return { status: 200, body: null };
      }
    }
    return { status: res.status, body: null };
  } catch {
    return { status: 0, body: null };
  }
}

async function probeLicense(): Promise<LicenseSnapshot | null> {
  try {
    const headers: Record<string, string> = { Accept: 'application/json' };
    const token = localStorage.getItem('super_token');
    if (token) headers.Authorization = `Bearer ${token}`;
    const res = await fetch('/api/v1/system/license', {
      method: 'GET',
      headers,
      credentials: 'same-origin',
    });
    if (!res.ok) return null;
    return (await res.json()) as LicenseSnapshot;
  } catch {
    return null;
  }
}

/**
 * Runtime feature flags for the Dashboard shell.
 * Licensed UI entries must stay hidden when the matching plugin is not loaded.
 *
 * `auth` = login gate (OSS core secret or security plugin).
 * `security` = multi-user token CRUD (security plugin only).
 */
export const useCapabilitiesStore = defineStore('capabilities', () => {
  const ready = ref(false);
  const auth = ref(false);
  const security = ref(false);
  const notify = ref(false);
  const isolation = ref(false);
  const licensed = ref(false);
  const license = ref<LicenseSnapshot | null>(null);

  let discoverPromise: Promise<void> | null = null;

  const isOssShell = computed(() => !licensed.value);

  async function discover() {
    const cfg = getSuperConfig();
    const edition = cfg.edition.toLowerCase();

    licensed.value =
      edition === 'licensed' || edition.includes('premium') || edition.includes('pro');

    const authProbe = await probeAuthStatus();

    // Auth gate vs token management (see AuthStatusResponse.token_management).
    if (authProbe.status === 0) {
      auth.value = cfg.auth_required === true;
      security.value = false;
    } else if (authProbe.status === 404) {
      auth.value = false;
      security.value = false;
    } else if (authProbe.status === 401) {
      // Older security plugin builds: status not whitelisted, session missing.
      auth.value = true;
      security.value = true;
    } else if (authProbe.status === 200) {
      auth.value = true;
      security.value = authProbe.body?.token_management === true;
    } else {
      auth.value = cfg.auth_required === true;
      security.value = false;
    }

    // Not signed in yet: skip the license/notify probes. With the auth gate on
    // they would all answer 401 and feed the daemon's per-IP failed-auth
    // throttle (lockout after ~20 attempts). Pro capabilities appear after
    // login via refresh().
    if (auth.value && !localStorage.getItem('super_token')) {
      security.value = false;
      notify.value = false;
      isolation.value = false;
      license.value = null;
      ready.value = true;
      return;
    }

    const [notifyStatus, lic] = await Promise.all([
      probeStatus('/api/v1/system/notify'),
      probeLicense(),
    ]);

    if (notifyStatus === 0) {
      notify.value = false;
    } else {
      notify.value = notifyStatus !== 404;
    }

    if (lic) {
      license.value = lic;
      licensed.value = true;
      // Capability = plugin actually LOADED (plugin_versions), not merely
      // granted: a grant without its plugin would render half-broken UI
      // (e.g. isolation granted but shipped only on Linux).
      const versions = lic.plugin_versions || {};
      if (versions.security) {
        auth.value = true;
        security.value = true;
      }
      if (versions.notify) notify.value = true;
      isolation.value = !!versions.isolation;
    } else {
      license.value = null;
      // Without a license, isolation plugin cannot load.
      isolation.value = false;
    }

    ready.value = true;
  }

  async function ensureDiscovered() {
    if (ready.value) return;
    if (!discoverPromise) {
      discoverPromise = discover().finally(() => {
        discoverPromise = null;
      });
    }
    await discoverPromise;
  }

  /** Force re-discovery (after login/logout) so capabilities match the new session. */
  async function refresh() {
    ready.value = false;
    await ensureDiscovered();
  }

  return {
    ready,
    auth,
    security,
    notify,
    isolation,
    licensed,
    license,
    isOssShell,
    discover,
    ensureDiscovered,
    refresh,
  };
});
