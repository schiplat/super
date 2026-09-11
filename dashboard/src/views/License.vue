<script setup lang="ts">
import { ref, onMounted, computed } from 'vue';
import {
  ShieldCheck, AlertTriangle, Lock, Bell, Activity, FileText,
  LayoutDashboard, ExternalLink, CheckCircle2, Server
} from 'lucide-vue-next';
import apiClient from '@/api/client';
import { API_PATHS } from '@/api/paths';
import { format } from 'date-fns';
import { getSuperConfig } from '@/lib/superConfig';
import { useCapabilitiesStore } from '@/stores/capabilities';

const config = getSuperConfig();
const version = config.version || 'Unknown';
const edition = String(config.edition || 'oss');
const caps = useCapabilitiesStore();
const isPremium =
  config.auth_required === true ||
  edition === 'licensed' ||
  edition.includes('premium');

const loading = ref(false);
const error = ref('');
const isOssNoLicense = ref(false);
const licenseData = ref<any>(null);

const UPGRADE_URL = 'https://super.docs.sconts.com/go/pro/';

const featureMap: Record<string, { label: string; icon: any }> = {
  audit: { label: 'Audit logging', icon: FileText },
  notify: { label: 'Notifications', icon: Bell },
  cgroups: { label: 'Resource limits', icon: Activity },
  rbac: { label: 'Access control', icon: Lock },
  dashboard: { label: 'Dashboard', icon: LayoutDashboard },
};

const subscriptionStatus = computed(() => licenseData.value?.subscription_status || 'active');
const isExpired = computed(() => subscriptionStatus.value === 'expired');
const isPerpetual = computed(() => subscriptionStatus.value === 'perpetual');

const issuedForVersion = computed(() => licenseData.value?.issued_for_version as string | undefined);
const maxSuperdVersion = computed(() =>
  licenseData.value?.max_superd_version || licenseData.value?.supported_super_version || '—'
);
const runningSuperd = computed(() => licenseData.value?.superd_version || version);
const versionInRange = computed(() => licenseData.value?.version_in_range !== false);
const upgradeUrl = computed(() => licenseData.value?.upgrade_url || UPGRADE_URL);

const statusTone = computed(() => {
  if (isExpired.value) return 'amber';
  return 'emerald';
});

const statusLabel = computed(() => {
  if (isExpired.value) return 'Expired — still works offline';
  if (isPerpetual.value) return 'Active · perpetual';
  return 'Active';
});

const versionHeadline = computed(() => `superd ≤ ${maxSuperdVersion.value}`);

const versionDetail = computed(() => {
  const issued = issuedForVersion.value ? `issued ${issuedForVersion.value}` : 'legacy key';
  const run = `running ${runningSuperd.value}`;
  if (versionInRange.value) return `${issued} · ${run}`;
  return `${issued} · ${run} · renew to upgrade`;
});

const expiryLabel = computed(() => {
  if (!licenseData.value?.expires_at) return 'No expiry date';
  const date = format(new Date(licenseData.value.expires_at * 1000), 'yyyy-MM-dd');
  if (isExpired.value) return `Ended ${date}`;
  return `Renews ${date}`;
});

const displayFeatures = computed(() => {
  if (!licenseData.value) return [];

  const rawFeatures = licenseData.value.features?.length
    ? [...licenseData.value.features]
    : (licenseData.value.grants || []).flatMap((plugin: string) => {
        if (plugin === 'security') return ['rbac', 'audit'];
        if (plugin === 'notify') return ['notify'];
        if (plugin === 'isolation') return ['cgroups'];
        if (plugin === 'ui') return ['dashboard'];
        return [plugin];
      });

  if (isPremium && !rawFeatures.includes('rbac')) rawFeatures.unshift('rbac');

  return rawFeatures.map((code: string) => {
    const map = featureMap[code];
    return {
      code,
      label: map?.label ?? code,
      icon: map?.icon ?? Server,
    };
  });
});

const pluginVersionEntries = computed(() => {
  const versions = licenseData.value?.plugin_versions;
  if (!versions || typeof versions !== 'object') return [];
  return Object.entries(versions).sort(([a], [b]) => a.localeCompare(b));
});

async function fetchLicense() {
  loading.value = true;
  error.value = '';
  isOssNoLicense.value = false;
  try {
    const res = await apiClient.get(API_PATHS.SYSTEM.LICENSE, { timeout: 8000 });
    licenseData.value = res.data;
  } catch (e: any) {
    console.error('license fetch failed', e);
    if (e.code === 'ECONNABORTED') {
      error.value = 'License API timed out. Rebuild superd and restart.';
    } else if (e.response?.status === 401) {
      error.value = 'Not authenticated. Log in with your auth_secret.';
    } else if (e.response?.status === 404) {
      // Compact OSS edition page — not an error state.
      isOssNoLicense.value = true;
      licenseData.value = null;
    } else {
      error.value = 'Could not load license info.';
    }
  } finally {
    loading.value = false;
  }
}

onMounted(fetchLicense);
</script>

<template>
  <div class="max-w-5xl mx-auto py-6 flex flex-col gap-5">
    <div class="flex items-start gap-4">
      <div
        class="w-12 h-12 rounded-xl flex items-center justify-center text-white shadow-sm shrink-0"
        :class="isOssNoLicense ? 'bg-primary' : (statusTone === 'amber' ? 'bg-amber-500' : 'bg-emerald-500')"
      >
        <ShieldCheck class="w-6 h-6" />
      </div>
      <div class="min-w-0 pt-0.5">
        <h1 class="text-xl font-bold tracking-tight">License</h1>
        <p class="text-sm text-foreground/60 mt-0.5">
          <template v-if="isOssNoLicense">Community edition · no subscription key</template>
          <template v-else>
            {{ licenseData?.issued_to || 'Super Process Manager' }}
            <span v-if="isPremium"> · Premium</span>
          </template>
        </p>
      </div>
    </div>

    <div v-if="loading" class="flex justify-center py-12">
      <span class="inline-block w-6 h-6 border-2 border-foreground/20 border-t-foreground/50 rounded-full animate-spin"></span>
    </div>

    <div v-else-if="isOssNoLicense" class="surface-card border border-border shadow-sm overflow-hidden">
      <div class="px-5 py-5 space-y-3">
        <p class="text-sm text-foreground/75 leading-relaxed">
          You are running the open-source single-host shell. Process control, stack apply,
          logs, and health checks work without a license.
        </p>
        <p class="text-sm text-foreground/60 leading-relaxed">
          Super Pro adds API auth &amp; RBAC, notifications, and Linux cgroup limits when you
          load the matching plugins with a subscription key.
        </p>
        <div class="pt-2 flex flex-wrap items-center gap-3">
          <a
            :href="UPGRADE_URL"
            target="_blank"
            rel="noopener noreferrer"
            class="inline-flex items-center gap-1.5 px-3 py-2 rounded-lg text-sm font-medium bg-primary text-primary-foreground hover:opacity-90 transition-opacity"
          >
            Get Super Pro
            <ExternalLink class="w-3.5 h-3.5" />
          </a>
          <span class="text-xs text-muted-foreground font-mono">superd {{ version }}</span>
          <span
            v-if="caps.security || caps.notify || caps.isolation"
            class="text-xs text-muted-foreground"
          >
            Detected:
            <span v-if="caps.security">security</span>
            <span v-if="caps.notify">{{ caps.security ? ' · ' : '' }}notify</span>
            <span v-if="caps.isolation">{{ (caps.security || caps.notify) ? ' · ' : '' }}isolation</span>
          </span>
        </div>
      </div>
    </div>

    <div v-else-if="error" class="alert alert-error text-sm">
      <AlertTriangle class="w-4 h-4" />
      <span>{{ error }}</span>
    </div>

    <template v-else-if="licenseData">
      <div class="surface-card border border-border shadow-sm overflow-hidden">
        <!-- status + meta -->
        <div class="px-5 py-4 border-b border-border flex flex-wrap items-center gap-x-4 gap-y-2">
          <span
            class="inline-flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide"
            :class="statusTone === 'amber' ? 'text-amber-700' : 'text-emerald-700'"
          >
            <span
              class="w-2 h-2 rounded-full"
              :class="[
                statusTone === 'amber' ? 'bg-amber-500' : 'bg-emerald-500',
                !isExpired && 'animate-pulse'
              ]"
            />
            {{ statusLabel }}
          </span>
          <span class="text-sm text-foreground/55">{{ expiryLabel }}</span>
        </div>

        <!-- version — the one thing users need -->
        <div class="px-5 py-5 border-b border-border bg-muted/20">
          <p class="text-xs font-medium text-foreground/45 uppercase tracking-wider mb-1">
            Paid features work on
          </p>
          <p class="text-2xl font-bold font-mono tracking-tight text-primary">
            {{ versionHeadline }}
          </p>
          <p class="text-sm text-foreground/65 mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1">
            <span>{{ versionDetail }}</span>
            <CheckCircle2
              v-if="versionInRange"
              class="w-4 h-4 text-emerald-500 shrink-0"
              aria-label="Version OK"
            />
          </p>
          <p v-if="isExpired" class="text-xs text-amber-800/90 mt-3 leading-relaxed">
            Subscription ended. Plugins on this version line keep working offline.
            <a
              :href="upgradeUrl"
              target="_blank"
              rel="noopener noreferrer"
              class="inline-flex items-center gap-0.5 ml-1 font-medium underline underline-offset-2"
            >
              Renew for newer superd
              <ExternalLink class="w-3 h-3" />
            </a>
          </p>
        </div>

        <!-- features + plugins -->
        <div class="px-5 py-4 grid grid-cols-1 sm:grid-cols-2 gap-5">
          <div>
            <p class="text-xs font-medium text-foreground/45 uppercase tracking-wider mb-2">Features</p>
            <ul class="space-y-2">
              <li
                v-for="feat in displayFeatures"
                :key="feat.code"
                class="flex items-center gap-2 text-sm text-foreground/80"
              >
                <component :is="feat.icon" class="w-3.5 h-3.5 text-primary/70 shrink-0" />
                {{ feat.label }}
              </li>
            </ul>
          </div>

          <div v-if="pluginVersionEntries.length">
            <p class="text-xs font-medium text-foreground/45 uppercase tracking-wider mb-2">Plugins</p>
            <div class="flex flex-wrap gap-2">
              <span
                v-for="[id, ver] in pluginVersionEntries"
                :key="id"
                class="inline-flex items-center px-2 py-0.5 rounded-md text-xs font-mono border border-border bg-transparent"
              >
                {{ id }}
                <span class="text-foreground/45">v{{ ver }}</span>
              </span>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>
