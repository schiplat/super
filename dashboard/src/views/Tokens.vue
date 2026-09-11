<script setup lang="ts">
import { ref, onMounted, watch } from 'vue';
import { useAuthStore } from '@/stores/auth';
import apiClient from '@/api/client';
import { Shield, Key, Trash2, Plus, Copy, Check, Lock, AlertTriangle, RefreshCw } from 'lucide-vue-next';
import { useClipboard } from '@vueuse/core';
import { format } from 'date-fns';
import UiButton from '@/components/ui/UiButton.vue';
import UiBadge from '@/components/ui/UiBadge.vue';
import UiDialog from '@/components/ui/UiDialog.vue';
import UiInput from '@/components/ui/UiInput.vue';
import { alertDialog, confirmDialog } from '@/lib/app-dialog';

const authStore = useAuthStore();
const { copy, copied } = useClipboard();

const tokens = ref<any[]>([]);
const isLoading = ref(false);
const error = ref('');
const authStatus = ref<{ auth_secret_disabled: boolean; has_admin_token: boolean; can_disable_auth_secret: boolean; auth_secret_login_allowed: boolean } | null>(null);

const showCreateModal = ref(false);
const showRevokeModal = ref(false);
const showDisableSecretModal = ref(false);
const revokeTargetId = ref('');
const revealSecret = ref('');
const revealTitle = ref('Token secret');

const roles = ['Viewer', 'Operator', 'Admin'];
const newTokenName = ref('');
const newTokenRole = ref('Viewer');

function getCurrentPrefix(): string { const token = localStorage.getItem('super_token') || ''; return token ? token.substring(0, 6) : ''; }
function isCurrentSession(tokenPrefix: string) { return tokenPrefix === getCurrentPrefix(); }

async function fetchAuthStatus() { try { const res = await apiClient.get('/api/v1/auth/status'); authStatus.value = res.data; } catch { authStatus.value = null; } }

async function fetchTokens() {
  if (!authStore.user && localStorage.getItem('super_token')) return;
  isLoading.value = true; error.value = '';
  try { const res = await apiClient.get('/api/v1/auth/tokens'); tokens.value = res.data || []; }
  catch (e: any) { if (e.response?.status === 403) tokens.value = []; else error.value = 'Unable to load token list.'; }
  finally { isLoading.value = false; }
}

async function createToken() {
  if (!newTokenName.value.trim()) return;
  try {
    const res = await apiClient.post('/api/v1/auth/tokens', { name: newTokenName.value.trim(), role: newTokenRole.value.toLowerCase() });
    revealTitle.value = 'New token secret'; revealSecret.value = res.data.token as string;
    await Promise.all([fetchTokens(), fetchAuthStatus()]);
  } catch (e: any) {
    await alertDialog('Failed to create token: ' + (e.response?.data?.message || e.message), {
      title: 'Create failed',
      variant: 'destructive',
    });
  }
}

async function renewToken(id: string) {
  const ok = await confirmDialog('Rotate this token secret? The old secret stops working immediately.', {
    title: 'Rotate token',
    confirmLabel: 'Rotate',
    variant: 'destructive',
  });
  if (!ok) return;
  try {
    const res = await apiClient.post(`/api/v1/auth/tokens/${id}/renew`);
    revealTitle.value = 'Renewed token secret'; revealSecret.value = res.data.token as string; showCreateModal.value = true;
    if (id === authStore.user?.id) localStorage.setItem('super_token', res.data.token);
    await fetchTokens();
  } catch (e: any) {
    await alertDialog('Failed to renew: ' + (e.response?.data?.message || e.message), {
      title: 'Renew failed',
      variant: 'destructive',
    });
  }
}

function requestDisableAuthSecret() { if (!authStatus.value?.can_disable_auth_secret) return; showDisableSecretModal.value = true; }

async function confirmDisableAuthSecret() {
  showDisableSecretModal.value = false;
  try { await apiClient.post('/api/v1/auth/secret/disable'); await fetchAuthStatus(); }
  catch (e: any) {
    await alertDialog('Failed to disable auth_secret: ' + (e.response?.data?.message || e.message), {
      title: 'Disable failed',
      variant: 'destructive',
    });
  }
}

function requestRevoke(id: string) { revokeTargetId.value = id; showRevokeModal.value = true; }

async function confirmRevoke() {
  if (!revokeTargetId.value) return; const id = revokeTargetId.value; showRevokeModal.value = false;
  try {
    await apiClient.delete(`/api/v1/auth/tokens/${id}`); await Promise.all([fetchTokens(), fetchAuthStatus()]);
    if (id === authStore.user?.id) authStore.logout();
  } catch (e: any) {
    await alertDialog('Failed to revoke: ' + (e.response?.data?.message || e.message), {
      title: 'Revoke failed',
      variant: 'destructive',
    });
  }
  finally { revokeTargetId.value = ''; }
}

function openCreateModal() { showCreateModal.value = true; revealSecret.value = ''; newTokenName.value = ''; newTokenRole.value = authStore.user?.id === 'root' ? 'Admin' : 'Viewer'; }
function closeCreateModal() { showCreateModal.value = false; revealSecret.value = ''; newTokenName.value = ''; newTokenRole.value = 'Viewer'; }
function refreshAll() { fetchTokens(); fetchAuthStatus(); }

onMounted(() => {
  if (authStore.user) refreshAll();
  else {
    const unwatch = watch(() => authStore.user, (newUser) => { if (newUser) { refreshAll(); unwatch(); } });
  }
});
</script>

<template>
  <div class="flex flex-col gap-6 max-w-5xl mx-auto py-6">
    <div><h1 class="text-2xl font-bold text-foreground">Access Tokens</h1><p class="text-muted-foreground">Manage your session and API access credentials.</p></div>

    <!-- auth_secret banner -->
    <div v-if="authStore.isAdmin && authStatus && !authStatus.auth_secret_disabled" class="rounded-xl border border-warning/30 border-l-4 border-l-warning bg-warning/5 px-4 py-4 shadow-sm">
      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div class="flex items-start gap-3 min-w-0">
          <div class="shrink-0 mt-0.5 w-9 h-9 rounded-full bg-warning/15 text-warning flex items-center justify-center"><AlertTriangle class="w-5 h-5" /></div>
          <div class="min-w-0 space-y-1.5">
            <p class="font-bold text-foreground text-sm sm:text-base leading-snug">Config <code class="font-mono px-1 py-0.5 rounded bg-warning/10">auth_secret</code> login is still enabled</p>
            <p class="text-sm text-muted-foreground leading-relaxed">Anyone with the secret in <code class="font-mono text-xs">super.toml</code> can sign in as Admin. Create an Admin Access Token first, then disable the config secret.</p>
            <p v-if="!authStatus.can_disable_auth_secret" class="text-xs font-semibold text-warning/80">Create at least one Admin Access Token before you can disable it.</p>
          </div>
        </div>
        <UiButton :disabled="!authStatus.can_disable_auth_secret" @click="requestDisableAuthSecret"><Lock class="w-4 h-4" />Disable auth_secret</UiButton>
      </div>
    </div>
    <div v-else-if="authStore.isAdmin && authStatus?.auth_secret_disabled" class="rounded-xl border border-success/30 border-l-4 border-l-success bg-success/10 px-4 py-3 text-sm flex items-start gap-3">
      <Shield class="w-5 h-5 shrink-0 text-success mt-0.5" />
      <p class="leading-relaxed text-muted-foreground">Config <code class="font-mono">auth_secret</code> is disabled for login. Revoking all Admin tokens will re-enable it automatically.</p>
    </div>

    <!-- Current User Card -->
    <div class="surface-card border border-border shadow-sm p-6 flex items-center gap-6">
      <div class="bg-muted rounded-full w-16 h-16 flex items-center justify-center text-3xl shadow-sm"><span class="uppercase text-foreground/70">{{ authStore.user?.avatar || 'U' }}</span></div>
      <div>
        <h2 class="text-xl font-semibold text-foreground">{{ authStore.user?.name || 'Guest' }}</h2>
        <div class="flex items-center gap-2 mt-1">
          <UiBadge variant="default"><Shield class="w-3 h-3" />{{ authStore.user?.role || 'Unknown' }}</UiBadge>
          <span v-if="authStore.user?.id !== 'root'" class="text-xs text-muted-foreground font-mono select-all">ID: {{ authStore.user?.id }}</span>
          <span v-else class="text-xs text-muted-foreground font-mono">Root Session</span>
        </div>
      </div>
    </div>

    <!-- Token Management -->
    <div class="surface-card border border-border shadow-sm overflow-visible">
      <div class="p-0">
        <div class="p-5 border-b border-border flex justify-between items-center">
          <div>
            <h3 class="font-bold text-lg text-foreground flex items-center gap-2"><Key class="w-5 h-5 text-warning" />Access Tokens</h3>
            <p class="text-xs text-muted-foreground mt-1">{{ authStore.isAdmin ? 'System-wide token management.' : 'Your active session token.' }}</p>
          </div>
          <UiButton v-if="authStore.isAdmin" @click="openCreateModal"><Plus class="w-4 h-4" />Generate Token</UiButton>
        </div>
        <div class="data-table-scroll">
          <table class="data-table">
            <thead><tr><th class="pl-6">Name</th><th>Token Prefix</th><th>Role</th><th>Created At</th><th class="pr-6 text-right">Actions</th></tr></thead>
            <tbody>
              <tr v-if="isLoading"><td colspan="5" class="text-center py-8 text-muted-foreground">Loading tokens...</td></tr>
              <tr v-else-if="tokens.length === 0"><td colspan="5" class="text-center py-8 text-muted-foreground">No active tokens found.<span v-if="authStore.isAdmin && authStore.user?.id === 'root'" class="block text-xs mt-1">(You are logged in via Root Secret. Generate tokens for API access.)</span></td></tr>
              <tr v-else v-for="token in tokens" :key="token.id" :class="{ 'bg-muted/30': isCurrentSession(token.token_prefix) }">
                <td class="pl-6 font-medium text-foreground">
                  {{ token.name }}
                  <UiBadge v-if="isCurrentSession(token.token_prefix)" variant="info" size="sm">Current Session</UiBadge>
                </td>
                <td class="font-mono text-xs text-muted-foreground flex items-center gap-1 py-4"><Lock class="w-3 h-3 opacity-40" />{{ token.token_prefix }}&#x2022;&#x2022;&#x2022;&#x2022;&#x2022;&#x2022;&#x2022;&#x2022;&#x2022;&#x2022;</td>
                <td><UiBadge :variant="token.role === 'admin' || token.role === 'Admin' ? 'default' : token.role === 'operator' || token.role === 'Operator' ? 'notice' : 'default'">{{ token.role }}</UiBadge></td>
                <td class="text-xs text-muted-foreground">{{ format(new Date(token.created_at * 1000), 'yyyy-MM-dd HH:mm') }}</td>
                <td class="pr-6 text-right">
                  <div class="flex justify-end gap-2">
                    <UiButton v-if="authStore.isAdmin || isCurrentSession(token.token_prefix)" variant="ghost" size="sm" title="Renew" @click="renewToken(token.id)"><RefreshCw class="w-3 h-3" />Renew</UiButton>
                    <UiButton v-if="authStore.isAdmin" variant="destructive" size="sm" title="Revoke" @click="requestRevoke(token.id)"><Trash2 class="w-3 h-3" />Revoke</UiButton>
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>

    <!-- Create Modal -->
    <UiDialog :open="showCreateModal" @update:open="val => showCreateModal = val" title="Generate New Token">
      <template #description>Create a scoped API credential. The secret is shown only once.</template>
      <div v-if="!revealSecret" class="flex flex-col gap-4 mt-2">
        <label class="block space-y-1.5">
          <span class="text-sm font-semibold text-foreground">Description / Name</span>
          <UiInput v-model="newTokenName" placeholder="e.g. CI/CD Runner" @keyup.enter="createToken" />
        </label>
        <label class="block space-y-1.5">
          <span class="text-sm font-semibold text-foreground">Role / Permissions</span>
          <div class="grid grid-cols-3 gap-2">
            <button v-for="r in roles" :key="r" type="button" class="rounded-xl border px-2 py-2.5 text-sm font-medium transition-all" :class="newTokenRole === r ? 'border-foreground/20 bg-muted text-foreground' : 'border-border bg-card text-muted-foreground hover:border-border/80'" @click="newTokenRole = r">{{ r }}</button>
          </div>
          <p class="text-xs text-muted-foreground mt-2 leading-relaxed">
            <span v-if="newTokenRole === 'Viewer'">Read-only access (Dashboard viewing).</span>
            <span v-else-if="newTokenRole === 'Operator'">Can start/stop/restart programs. No config changes.</span>
            <span v-else>Full system access (Create, Delete, Update, Tokens).</span>
          </p>
        </label>
        <div class="flex justify-end gap-3 pt-2">
          <UiButton variant="secondary" @click="closeCreateModal">Cancel</UiButton>
          <UiButton :disabled="!newTokenName.trim()" @click="createToken">Generate</UiButton>
        </div>
      </div>
      <div v-else class="flex flex-col gap-4 mt-2">
        <div class="flex items-start gap-3 rounded-xl border border-success/30 bg-success/10 px-3 py-2.5 text-sm text-success"><Check class="w-5 h-5 shrink-0 mt-0.5" /><span>Token generated successfully. Copy it before closing this dialog.</span></div>
        <div class="rounded-xl border border-border bg-muted/60 p-3">
          <p class="text-xs font-semibold uppercase tracking-wide text-muted-foreground mb-2">{{ revealTitle }}</p>
          <div class="flex gap-2 items-stretch">
            <input type="text" :value="revealSecret" class="field-control font-mono text-xs !py-2.5 select-all" readonly @focus="($event.target as HTMLInputElement).select()" />
            <UiButton size="icon" :title="copied ? 'Copied' : 'Copy'" @click="copy(revealSecret)"><Check v-if="copied" class="w-4 h-4" /><Copy v-else class="w-4 h-4" /></UiButton>
          </div>
          <p class="text-xs text-destructive mt-2 font-medium">You will not be able to view this secret again.</p>
        </div>
        <div class="flex justify-end pt-1"><UiButton @click="closeCreateModal">Done</UiButton></div>
      </div>
    </UiDialog>

    <!-- Revoke Dialog -->
    <UiDialog :open="showRevokeModal" @update:open="val => showRevokeModal = val" title="Revoke Access Token?">
      <template #description>Applications using this token will lose access immediately. <strong>This cannot be undone.</strong></template>
      <div class="flex items-center gap-3 mt-4">
        <UiButton variant="secondary" @click="showRevokeModal = false">Cancel</UiButton>
        <UiButton variant="destructive" @click="confirmRevoke">Revoke</UiButton>
      </div>
    </UiDialog>

    <!-- Disable auth_secret Dialog -->
    <UiDialog :open="showDisableSecretModal" @update:open="val => showDisableSecretModal = val" title="Disable config auth_secret?">
      <template #description>Login with the secret in <code class="font-mono">super.toml</code> will stop working. Keep at least one Admin Access Token.</template>
      <div class="flex items-center gap-3 mt-4">
        <UiButton variant="secondary" @click="showDisableSecretModal = false">Cancel</UiButton>
        <UiButton variant="default" @click="confirmDisableAuthSecret">Disable auth_secret</UiButton>
      </div>
    </UiDialog>
  </div>
</template>
