<script setup lang="ts">
import { useAuthStore } from '@/stores/auth';
import { useProgramStore } from '@/stores/program';
import { LogOut, ChevronDown, ShieldCheck, Wifi, WifiOff } from 'lucide-vue-next';
import { useRouter } from 'vue-router';
import { ref } from 'vue';
import apiClient from '@/api/client';
import Slot from '@/slots/Slot.vue';

const authStore = useAuthStore();
const programStore = useProgramStore();
const router = useRouter();

const userMenuOpen = ref(false);

function toggleUserMenu() {
  userMenuOpen.value = !userMenuOpen.value;
}

function navigateTo(path: string) {
  userMenuOpen.value = false;
  void router.push(path);
}

function closeUserMenu() {
  userMenuOpen.value = false;
}

function handleBlur() {
  window.setTimeout(() => { userMenuOpen.value = false; }, 200);
}

async function handleLogout() {
  userMenuOpen.value = false;
  // Ask the backend which gate is active; it decides the way out. SSO
  // deployments end the provider session too (RP-initiated logout via
  // `GET /auth/sso/logout`); a full navigation — the SPA session is over
  // either way.
  let mode: string | undefined;
  let ssoActive = false;
  try {
    const res = await apiClient.get('/api/v1/auth/status');
    mode = res.data?.mode;
    ssoActive = mode === 'sso';
  } catch {
    // Status is auth-exempt; a failure means fall back to a local sign-out.
  }
  await authStore.logout();
  if (ssoActive) {
    window.location.assign('/api/v1/auth/sso/logout');
  } else if (!mode || mode === 'none') {
    // No gate (loopback dev): there is no credential to revoke, but the menu
    // still offers Sign out — reset the SPA state instead of doing nothing.
    window.location.assign('/');
  }
}
</script>

<template>
  <div class="flex items-center gap-4">

    <!-- Live connection status -->
    <div
      class="flex items-center gap-2 px-2.5 py-1 rounded-full border transition-colors select-none"
      :class="programStore.isConnected
        ? 'bg-success/5 border-success/15 text-success'
        : 'bg-destructive/5 border-destructive/15 text-destructive'"
      :title="programStore.isConnected ? 'Connected' : 'Disconnected'"
    >
      <Wifi v-if="programStore.isConnected" class="w-3.5 h-3.5" />
      <WifiOff v-else class="w-3.5 h-3.5" />
      <span class="text-xs font-bold tracking-wider">
        {{ programStore.isConnected ? 'LIVE' : 'OFFLINE' }}
      </span>
    </div>

    <div class="h-6 w-px bg-border hidden sm:block"></div>

    <!-- User menu -->
    <div v-if="authStore.user" class="relative">
      <button
        class="flex items-center gap-3 pl-2 pr-1 py-1 rounded-[10px] hover:bg-muted/60 transition-colors cursor-pointer group"
        :class="userMenuOpen ? 'bg-muted' : ''"
        @click="toggleUserMenu"
        @blur="handleBlur"
      >
        <div class="hidden md:flex flex-col items-end gap-0.5">
          <span class="text-xs font-bold text-foreground leading-none">
            {{ authStore.user.name }}
          </span>
          <span class="text-xs font-medium text-muted-foreground uppercase tracking-wider leading-none">
            {{ authStore.user.role }}
          </span>
        </div>

        <div class="rounded-full w-8 h-8 flex items-center justify-center bg-muted ring-1 ring-border group-hover:ring-border transition-all">
          <span class="text-xs font-bold text-foreground/70">{{ authStore.user.avatar }}</span>
        </div>

        <ChevronDown class="w-3 h-3 text-muted-foreground/40 group-hover:text-muted-foreground transition-colors" />
      </button>

      <!-- Dropdown panel -->
      <div
        v-if="userMenuOpen"
        class="absolute top-full right-0 mt-1.5 w-56 rounded-xl bg-card border border-border shadow-[0_10px_40px_-10px_rgba(28,25,23,0.15)] p-1.5 flex flex-col gap-0.5"
      >
        <div class="px-3 py-2 text-xs uppercase tracking-wider font-bold text-muted-foreground/40 select-none">
          Account
        </div>

        <!-- STABLE: nav.account — plugins need navigate/close. Do not change :context without tests/slots.spec.ts review. -->
        <Slot
          name="nav.account"
          :context="{ navigate: navigateTo, close: closeUserMenu }"
        />

        <a
          href="/license"
          class="flex items-center gap-3 px-3 py-2.5 rounded-[10px] text-sm text-muted-foreground hover:text-foreground hover:bg-muted/80 font-medium transition-colors"
          @click.prevent="navigateTo('/license')"
        >
          <ShieldCheck class="w-4 h-4 opacity-50" />
          License
        </a>

        <div class="h-px bg-border my-1 mx-2"></div>

        <a
          @click="handleLogout"
          class="flex items-center gap-3 px-3 py-2.5 rounded-[10px] text-sm text-destructive hover:bg-destructive/10 font-medium transition-colors cursor-pointer"
        >
          <LogOut class="w-4 h-4 opacity-50" />
          Sign out
        </a>
      </div>
    </div>

    <div v-else class="w-8 h-8 rounded-full bg-muted animate-pulse"></div>

  </div>
</template>
