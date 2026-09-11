import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { getSuperConfig } from '@/lib/superConfig';

export interface LicenseSnapshot {
  grants: string[];
  plugin_versions?: Record<string, string>;
  [key: string]: unknown;
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
 */
export const useCapabilitiesStore = defineStore('capabilities', () => {
  const ready = ref(false);
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

    const [authStatus, notifyStatus, lic] = await Promise.all([
      probeStatus('/api/v1/auth/status'),
      probeStatus('/api/v1/system/notify'),
      probeLicense(),
    ]);

    // Prefer live probes when the API is reachable; else fall back to injected config.
    if (authStatus === 0) {
      security.value = cfg.auth_required === true;
    } else {
      security.value = authStatus !== 404;
    }

    if (notifyStatus === 0) {
      notify.value = false;
    } else {
      notify.value = notifyStatus !== 404;
    }

    if (lic) {
      license.value = lic;
      licensed.value = true;
      const grants = Array.isArray(lic.grants) ? lic.grants : [];
      const versions = lic.plugin_versions || {};
      if (grants.includes('security') || versions.security) security.value = true;
      if (grants.includes('notify') || versions.notify) notify.value = true;
      isolation.value = !!(grants.includes('isolation') || versions.isolation);
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

  return {
    ready,
    security,
    notify,
    isolation,
    licensed,
    license,
    isOssShell,
    discover,
    ensureDiscovered,
  };
});
