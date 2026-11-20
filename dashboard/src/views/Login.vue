<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { ArrowRight, Loader2, Lock, LogIn } from 'lucide-vue-next';
import apiClient from '@/api/client';
import { useAuthStore } from '@/stores/auth';
import { useCapabilitiesStore } from '@/stores/capabilities';
import UiButton from '@/components/ui/UiButton.vue';
import UiField from '@/components/ui/UiField.vue';
import UiInput from '@/components/ui/UiInput.vue';
import type { AuthStatusLite } from '@/types/sso';

const route = useRoute();
const router = useRouter();
const authStore = useAuthStore();

const status = ref<AuthStatusLite | null>(null);
const mode = computed(() => status.value?.mode ?? 'secret');
const ssoEnabled = computed(() => status.value?.sso?.enabled === true);

const token = ref('');
const loading = ref(true);
const error = ref('');

const AUTH_ERRORS: Record<string, string> = {
  state: 'The sign-in request expired or was already used. Please try again.',
  idp: 'The identity provider rejected the sign-in. Check the superd logs for details.',
  claim:
    'The provider returned a token this server cannot accept. Check the issuer and client configuration.',
  denied: 'Your account is not mapped to a role. Ask an administrator to add a role rule.',
  exchange: 'The sign-in could not be completed. Please try again.',
  server: 'Authentication is temporarily unavailable. Please try again.',
};

/** Only same-site absolute paths, the same rule the server applies. */
function safePath(value: unknown, fallback = '/'): string {
  if (typeof value !== 'string') return fallback;
  if (!value.startsWith('/') || value.startsWith('//') || value.includes('\\')) return fallback;
  return value;
}

const redirectTarget = computed(() => safePath(route.query.redirect));
const ssoHref = computed(
  () => `/api/v1/auth/sso/login?return=${encodeURIComponent(redirectTarget.value)}`,
);

async function finishSsoLogin() {
  loading.value = true;
  try {
    const res = await apiClient.post<{ token: string; return_to: string }>(
      '/api/v1/auth/sso/exchange',
    );
    localStorage.setItem('super_token', res.data.token);
    // Re-discover capabilities with the new session, then resolve the profile
    // from `/auth/me` (role-aware, unlike the admin token list).
    await useCapabilitiesStore().refresh();
    await authStore.fetchProfile();
    await router.replace(safePath(res.data.return_to, redirectTarget.value));
  } catch {
    localStorage.removeItem('super_token');
    error.value = 'The SSO sign-in could not be completed. Please try again.';
    // Drop ?sso=ok: the exchange cookie is single-use, so a refresh cannot retry it.
    await router.replace({
      path: '/login',
      query: route.query.redirect ? { redirect: route.query.redirect } : {},
    });
  } finally {
    loading.value = false;
  }
}

async function handleLogin() {
  if (!token.value) return;
  loading.value = true;
  error.value = '';
  try {
    await apiClient.post(
      '/api/v1/auth/login',
      {},
      { headers: { Authorization: `Bearer ${token.value}` } },
    );
    localStorage.setItem('super_token', token.value);
    await useCapabilitiesStore().refresh();
    await authStore.fetchProfile();
    await router.push(redirectTarget.value);
  } catch (e: any) {
    error.value = e.response?.data?.message || e.response?.data?.error || 'Invalid credential';
    localStorage.removeItem('super_token');
  } finally {
    loading.value = false;
  }
}

onMounted(async () => {
  try {
    const res = await apiClient.get<AuthStatusLite>('/api/v1/auth/status');
    status.value = res.data;
  } catch {
    // Status is auth-exempt; a failure means the daemon is unreachable and the
    // secret form is the only thing left to try.
  }

  const authError = route.query.auth_error;
  if (typeof authError === 'string') {
    error.value = AUTH_ERRORS[authError] ?? 'Sign-in failed. Please try again.';
  }

  if (route.query.sso === 'ok') {
    await finishSsoLogin();
    return;
  }

  if (mode.value === 'none') {
    await router.replace(redirectTarget.value);
    return;
  }

  loading.value = false;
});
</script>

<template>
  <div class="flex min-h-screen items-center justify-center bg-background p-4">
    <div
      class="w-full max-w-sm overflow-hidden rounded-2xl bg-card ring-1 ring-border/60 shadow-[0_1px_2px_hsl(220_20%_20%/0.04),0_2px_8px_hsl(220_20%_20%/0.03)]"
    >
      <div class="p-8">
        <div class="mb-6 flex justify-center">
          <div
            class="flex h-14 w-14 items-center justify-center rounded-2xl bg-muted/40 text-muted-foreground"
          >
            <Lock class="h-7 w-7" />
          </div>
        </div>

        <p class="mb-6 text-center text-sm text-muted-foreground">
          {{
            mode === 'sso'
              ? 'Sign in with your identity provider'
              : 'Sign in to manage your services'
          }}
        </p>

        <div v-if="loading" class="flex justify-center py-6">
          <Loader2 class="h-5 w-5 animate-spin text-muted-foreground" />
        </div>

        <div v-else-if="mode === 'sso'" class="space-y-4">
          <a
            :href="ssoHref"
            class="flex w-full items-center justify-center gap-2 rounded-xl bg-primary px-4 py-3 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
          >
            <LogIn class="h-4 w-4" />
            Sign in with SSO
          </a>
          <p v-if="ssoEnabled" class="text-center text-[11px] text-muted-foreground">
            You will be redirected to your provider and back.
          </p>
          <p v-if="error" class="text-center text-xs font-medium text-destructive">{{ error }}</p>
        </div>

        <div v-else class="space-y-4">
          <UiField
            label="Bearer credential"
            hint="OSS: [server].auth_secret. Licensed: prefer an access token (sk-…)."
          >
            <UiInput
              v-model="token"
              type="password"
              placeholder="auth_secret or sk-…"
              autocomplete="current-password"
              @keyup.enter="handleLogin"
            />
          </UiField>
          <p v-if="error" class="text-xs font-medium text-destructive">{{ error }}</p>
          <UiButton class="w-full" :disabled="loading || !token" @click="handleLogin">
            <Loader2 v-if="loading" class="h-4 w-4 animate-spin" />
            <template v-else>
              <span>Sign In</span>
              <ArrowRight class="h-4 w-4 opacity-80" />
            </template>
          </UiButton>
        </div>
      </div>
      <div class="border-t border-border/40 py-4 text-center">
        <p class="text-xs text-muted-foreground">Secured by Super</p>
      </div>
    </div>
  </div>
</template>
