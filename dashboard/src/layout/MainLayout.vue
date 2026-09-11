<script setup lang="ts">
import { RouterView, RouterLink, useRoute, useRouter } from 'vue-router';
import {
  LayoutDashboard, Settings, RefreshCw, ChevronDown,
  Menu, X, Sun, Moon, Bell, Layers
} from 'lucide-vue-next';
import { useProgramStore } from '@/stores/program';
import { useAuthStore } from '@/stores/auth';
import { useCapabilitiesStore } from '@/stores/capabilities';
import { computed, ref, onMounted } from 'vue';
import { useDark, useToggle } from '@vueuse/core';
import UiButton from '@/components/ui/UiButton.vue';
import UiDialog from '@/components/ui/UiDialog.vue';
import NavbarRight from '@extensions/NavbarRight.vue';
import AppFooter from '@extensions/AppFooter.vue';

const store = useProgramStore();
const authStore = useAuthStore();
const caps = useCapabilitiesStore();
const route = useRoute();
const router = useRouter();

const mobileMenuOpen = ref(false);
const manageDropdownOpen = ref(false);
const showReloadModal = ref(false);

/** Routes under the Manage dropdown — keep the parent nav item active. */
const manageRouteActive = computed(() => {
  const path = route.path;
  return path === '/stack' || path.startsWith('/settings/');
});

const isDark = useDark({
  selector: 'html',
  attribute: 'class',
  valueDark: 'dark',
  valueLight: 'light',
});
const toggleDark = useToggle(isDark);

function toggleMobileMenu() {
  mobileMenuOpen.value = !mobileMenuOpen.value;
}

function toggleManageDropdown() {
  manageDropdownOpen.value = !manageDropdownOpen.value;
}

function handleManageBlur() {
  window.setTimeout(() => { manageDropdownOpen.value = false; }, 200);
}

/** Keep the Manage menu open while focus moves into the dropdown panel. */
function onManageFocusOut(e: FocusEvent) {
  const root = e.currentTarget as HTMLElement | null;
  const next = e.relatedTarget as Node | null;
  if (root && next && root.contains(next)) return;
  handleManageBlur();
}

function navigateTo(path: string) {
  manageDropdownOpen.value = false;
  void router.push(path);
}

function requestReload() {
  showReloadModal.value = true;
  manageDropdownOpen.value = false;
  mobileMenuOpen.value = false;
}

function confirmReload() {
  showReloadModal.value = false;
  store.reloadSystem();
}

onMounted(() => {
  void authStore.ensureProfile();
});
</script>

<template>
  <div class="h-screen w-full bg-background text-foreground font-sans flex flex-col overflow-hidden">

    <!-- Navbar -->
    <nav class="relative h-16 shrink-0 z-50 bg-card border-b border-border flex items-center justify-between px-4 sm:px-6 lg:px-8">
      <!-- Left: Logo & Mobile Toggle -->
      <div class="flex items-center gap-3 z-10 w-[250px]">
        <button class="md:hidden inline-flex h-8 w-8 items-center justify-center rounded-xl text-muted-foreground hover:bg-muted hover:text-foreground transition-colors" @click="toggleMobileMenu">
          <Menu v-if="!mobileMenuOpen" class="w-5 h-5" />
          <X v-else class="w-5 h-5" />
        </button>
        <div class="flex items-center gap-3">
          <img
            src="/super.png"
            alt="Super Process Manager"
            class="h-10 w-auto object-contain cursor-pointer"
            @click="$router.push('/')"
          />
        </div>
      </div>

      <!-- Center: Navigation (desktop) -->
      <div class="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 hidden md:flex items-center gap-1.5">
        <RouterLink
          to="/"
          custom
          v-slot="{ href, navigate, isExactActive }"
        >
          <a
            :href="href"
            class="flex items-center gap-2.5 px-4 py-2 rounded-xl text-sm font-medium transition-all duration-200"
            :class="isExactActive
              ? 'bg-primary text-primary-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground hover:bg-muted'"
            @click="navigate"
          >
            <LayoutDashboard class="w-4 h-4" />
            <span>Overview</span>
          </a>
        </RouterLink>

        <!-- Manage dropdown -->
        <div class="relative" @focusout="onManageFocusOut">
          <button
            type="button"
            class="flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-medium transition-colors"
            :class="manageRouteActive || manageDropdownOpen
              ? 'bg-primary text-primary-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground hover:bg-muted'"
            @click="toggleManageDropdown"
          >
            <Settings class="w-4 h-4" />
            <span>Manage</span>
            <ChevronDown class="w-3 h-3 opacity-50" />
          </button>
          <div
            v-if="manageDropdownOpen"
            class="absolute top-full left-0 mt-1.5 w-56 rounded-xl bg-card border border-border shadow-[0_10px_40px_-10px_rgba(28,25,23,0.15)] p-1.5 flex flex-col gap-0.5"
          >
            <a
              v-if="caps.notify"
              href="/settings/notify"
              class="flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors"
              :class="route.path.startsWith('/settings/')
                ? 'bg-muted text-foreground'
                : 'text-muted-foreground hover:text-foreground hover:bg-muted/80'"
              @click.prevent="navigateTo('/settings/notify')"
            >
              <Bell class="w-4 h-4 opacity-50" />
              Notifications
            </a>
            <a
              href="/stack"
              class="flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors"
              :class="route.path === '/stack'
                ? 'bg-muted text-foreground'
                : 'text-muted-foreground hover:text-foreground hover:bg-muted/80'"
              @click.prevent="navigateTo('/stack')"
            >
              <Layers class="w-4 h-4 opacity-50" />
              Stack Editor
            </a>
            <div v-if="authStore.canManage" class="h-px bg-border my-1 mx-2"></div>
            <button
              v-if="authStore.canManage"
              type="button"
              class="flex w-full items-center gap-3 px-3 py-2.5 rounded-lg text-sm text-muted-foreground hover:text-destructive hover:bg-muted/80 font-medium transition-colors cursor-pointer text-left"
              @click="requestReload"
            >
              <RefreshCw class="w-4 h-4" />
              Reload System
            </button>
          </div>
        </div>
      </div>

      <!-- Right: Actions -->
      <div class="flex items-center justify-end gap-3 z-10 w-[250px]">
        <button
          class="inline-flex h-8 w-8 items-center justify-center rounded-xl text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          @click="toggleDark()"
          title="Toggle Theme"
        >
          <component :is="isDark ? Moon : Sun" class="w-5 h-5" />
        </button>

        <div class="h-6 w-px bg-border"></div>
        <NavbarRight />
      </div>
    </nav>

    <!-- Mobile Menu -->
    <div
      v-if="mobileMenuOpen"
      class="absolute top-16 left-0 w-full bg-card border-b border-border z-40 md:hidden flex flex-col p-4 gap-2 shadow-lg"
    >
      <RouterLink
        to="/"
        class="flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium text-foreground hover:bg-muted transition-colors"
        @click="mobileMenuOpen = false"
      >
        <LayoutDashboard class="w-4 h-4" />
        Overview
      </RouterLink>

      <button class="flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium text-muted-foreground hover:bg-muted transition-colors" @click="toggleDark()">
        <component :is="isDark ? Moon : Sun" class="w-4 h-4" />
        {{ isDark ? 'Light Mode' : 'Dark Mode' }}
      </button>

      <button
        v-if="authStore.canManage"
        type="button"
        class="flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium text-muted-foreground hover:text-destructive hover:bg-muted/80 transition-colors text-left"
        @click="requestReload"
      >
        <RefreshCw class="w-4 h-4" />
        Reload Config
      </button>

      <div class="h-px bg-border my-1"></div>

      <a
        v-if="caps.notify"
        href="/settings/notify"
        class="flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm text-muted-foreground hover:bg-muted transition-colors"
        @click.prevent="navigateTo('/settings/notify'); mobileMenuOpen = false"
      >
        <Bell class="w-4 h-4" /> Notifications
      </a>
      <a
        href="/stack"
        class="flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm text-muted-foreground hover:bg-muted transition-colors"
        @click.prevent="navigateTo('/stack'); mobileMenuOpen = false"
      >
        <Layers class="w-4 h-4" /> Stack Editor
      </a>
    </div>

    <!-- Main Content -->
    <main class="flex-1 overflow-y-auto p-4 sm:p-6 lg:p-8 scroll-smooth" @click="mobileMenuOpen = false">
      <div class="max-w-[1600px] mx-auto">
        <RouterView />
        <AppFooter />
      </div>
    </main>

    <!-- Reload Confirmation Dialog -->
    <UiDialog :open="showReloadModal" @update:open="val => showReloadModal = val" title="Reload Configuration?">
      <template #description>
        This will reload the configuration from disk. Any removed programs will be stopped, and new ones will be added. Running programs with changed configurations may not restart automatically.
      </template>
      <div class="flex items-center gap-3 mt-2">
        <UiButton variant="secondary" @click="showReloadModal = false">Cancel</UiButton>
        <UiButton variant="destructive" @click="confirmReload">
          <RefreshCw class="w-4 h-4" />
          Reload Config
        </UiButton>
      </div>
    </UiDialog>

  </div>
</template>
