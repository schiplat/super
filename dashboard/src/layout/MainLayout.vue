<script setup lang="ts">
import { RouterView, RouterLink, useRoute, useRouter } from 'vue-router';
import {
  BadgeCheck,
  Boxes,
  Layers,
  Menu,
  Moon,
  PanelLeftClose,
  PanelLeftOpen,
  RefreshCw,
  Sun,
} from 'lucide-vue-next';
import { useProgramStore } from '@/stores/program';
import { useAuthStore } from '@/stores/auth';
import { computed, onMounted, ref } from 'vue';
import { useDark, useToggle, useWindowSize } from '@vueuse/core';
import UiButton from '@/components/ui/UiButton.vue';
import UiDialog from '@/components/ui/UiDialog.vue';
import NavbarRight from '@extensions/NavbarRight.vue';
import Slot from '@/slots/Slot.vue';
import { getSuperConfig } from '@/lib/superConfig';

const store = useProgramStore();
const authStore = useAuthStore();
const route = useRoute();
const router = useRouter();

const appVersion = getSuperConfig().version || 'dev';

const showReloadModal = ref(false);

const isDark = useDark({
  selector: 'html',
  attribute: 'class',
  valueDark: 'dark',
  valueLight: 'light',
});
const toggleDark = useToggle(isDark);

// ---------------------------------------------------------------- navigation

/**
 * Sidebar geometry: 248px expanded / 68px collapsed on desktop. Below 768px
 * the sidebar becomes an off-canvas drawer so nav labels (including plugin
 * items) stay readable on narrow screens.
 */
const COLLAPSE_KEY = 'super_sidebar_collapsed';
const collapsed = ref(localStorage.getItem(COLLAPSE_KEY) === '1');
const mobileOpen = ref(false);
const { width } = useWindowSize();
const isNarrow = computed(() => width.value < 768);
const effectiveCollapsed = computed(() => collapsed.value && !isNarrow.value);

const sidebarClass = computed(() =>
  isNarrow.value
    ? [
        'fixed inset-y-0 left-0 z-50 w-[248px] transition-transform duration-150',
        mobileOpen.value ? 'translate-x-0' : '-translate-x-full',
      ]
    : [
        'shrink-0 transition-[width] duration-150',
        effectiveCollapsed.value ? 'w-[68px]' : 'w-[248px]',
      ],
);

function toggleCollapsed() {
  collapsed.value = !collapsed.value;
  localStorage.setItem(COLLAPSE_KEY, collapsed.value ? '1' : '0');
}

const mainNav = [
  { to: '/', label: 'Programs', icon: Boxes },
  { to: '/stack', label: 'Stacks', icon: Layers },
];

const settingsNav = [{ to: '/license', label: 'License', icon: BadgeCheck }];

const TITLES: Record<string, string> = {
  '/': 'Programs',
  '/stack': 'Stacks',
  '/license': 'License',
  '/programs/new': 'New Program',
};

const pageTitle = computed(() => {
  if (route.name === 'ProgramEdit') return 'Edit Program';
  return TITLES[route.path] ?? (route.meta.title as string | undefined) ?? '';
});

function navigateTo(path: string) {
  void router.push(path);
}

/** Kept for the `nav.manage` / `nav.mobile` slot contract (plugins call it). */
function closeNav() {
  mobileOpen.value = false;
}

function requestReload() {
  showReloadModal.value = true;
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
  <div class="flex h-screen w-full overflow-hidden bg-background text-foreground font-sans">
    <!-- Off-canvas backdrop (narrow screens only) -->
    <div
      v-if="isNarrow && mobileOpen"
      class="fixed inset-0 z-40 bg-foreground/20"
      @click="mobileOpen = false"
    ></div>

    <!-- Sidebar: its own surface, no border/ring/shadow (separation comes from
         the gutter and the background change). -->
    <aside
      class="flex h-full flex-col bg-sidebar text-sidebar-foreground"
      :class="sidebarClass"
    >
      <!-- Brand row -->
      <div
        class="flex h-16 shrink-0 items-center px-3"
        :class="effectiveCollapsed ? 'justify-center' : 'justify-between'"
      >
        <a
          href="/"
          class="flex items-center gap-3 rounded-lg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
          @click.prevent="navigateTo('/'); closeNav()"
        >
          <img src="/super.png" alt="Super Process Manager" class="h-8 w-auto object-contain" />
          <span v-if="!effectiveCollapsed" class="text-sm font-semibold tracking-tight">
            Super
          </span>
        </a>
        <button
          v-if="!isNarrow"
          type="button"
          class="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
          :title="collapsed ? 'Expand sidebar' : 'Collapse sidebar'"
          @click="toggleCollapsed"
        >
          <component :is="collapsed ? PanelLeftOpen : PanelLeftClose" class="h-4 w-4" />
        </button>
      </div>

      <!-- Nav: two levels only — group label then items -->
      <nav class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto px-2.5 pt-6 pb-3">
        <RouterLink
          v-for="item in mainNav"
          :key="item.to"
          :to="item.to"
          custom
          v-slot="{ href, navigate, isActive, isExactActive }"
        >
          <a
            :href="href"
            :title="effectiveCollapsed ? item.label : undefined"
            class="flex items-center gap-3 rounded-xl px-2.5 py-2 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
            :class="[
              (item.to === '/' ? isExactActive : isActive)
                ? 'bg-sidebar-accent text-foreground'
                : 'text-muted-foreground hover:bg-sidebar-accent hover:text-foreground',
              effectiveCollapsed ? 'justify-center px-0' : '',
            ]"
            @click="navigate(); closeNav()"
          >
            <component :is="item.icon" class="h-4 w-4 shrink-0" />
            <span v-if="!effectiveCollapsed" class="truncate">{{ item.label }}</span>
          </a>
        </RouterLink>

        <div class="mt-6">
          <p
            v-if="!effectiveCollapsed"
            class="type-eyebrow mb-2.5 px-2.5 text-muted-foreground"
          >
            Settings
          </p>
          <div v-else class="mb-2.5 h-px bg-sidebar-border" />
          <div class="flex flex-col gap-1">
            <RouterLink
              v-for="item in settingsNav"
              :key="item.to"
              :to="item.to"
              custom
              v-slot="{ href, navigate, isActive }"
            >
              <a
                :href="href"
                :title="effectiveCollapsed ? item.label : undefined"
                class="flex items-center gap-3 rounded-xl px-2.5 py-2 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
                :class="[
                  isActive
                    ? 'bg-sidebar-accent text-foreground'
                    : 'text-muted-foreground hover:bg-sidebar-accent hover:text-foreground',
                  effectiveCollapsed ? 'justify-center px-0' : '',
                ]"
                @click="navigate(); closeNav()"
              >
                <component :is="item.icon" class="h-4 w-4 shrink-0" />
                <span v-if="!effectiveCollapsed" class="truncate">{{ item.label }}</span>
              </a>
            </RouterLink>

            <!-- STABLE: nav.manage — plugins need navigate/close/routePath. Do not change :context without tests/slots.spec.ts review. -->
            <Slot
              v-if="!isNarrow"
              name="nav.manage"
              :context="{ navigate: navigateTo, close: closeNav, routePath: route.path }"
            />
            <!-- STABLE: nav.mobile — plugins need navigate/close. Do not change :context without tests/slots.spec.ts review. -->
            <Slot
              v-else
              name="nav.mobile"
              :context="{ navigate: navigateTo, close: closeNav }"
            />
          </div>
        </div>
      </nav>

      <!-- Footer: brand row, then meta row. Never three items on one line. -->
      <div class="px-4 pb-4">
        <div class="border-t border-sidebar-border pt-3">
          <template v-if="!effectiveCollapsed">
            <div class="text-[11px] leading-4 text-muted-foreground">
              <span>Powered by </span>
              <a
                href="https://github.com/schiplat/super"
                target="_blank"
                rel="noopener noreferrer"
                class="rounded text-foreground/70 transition-colors hover:text-foreground"
              >schiplat/super&nbsp;↗</a>
            </div>
            <div class="mt-1 text-[11px] text-muted-foreground">
              <a
                href="https://super.docs.sconts.com/"
                target="_blank"
                rel="noopener noreferrer"
                class="transition-colors hover:text-foreground"
              >Docs&nbsp;↗</a>
              <span class="mx-1 text-muted-foreground/50">·</span>
              <span class="tabular-nums text-muted-foreground/70">v{{ appVersion }}</span>
            </div>
          </template>
          <div
            v-else
            class="text-center text-[10px] tabular-nums text-muted-foreground/70"
            :title="`v${appVersion}`"
          >
            v{{ appVersion.split('.')[0] }}
          </div>
        </div>
      </div>
    </aside>

    <!-- Main column: top bar (56px) + scrolling content -->
    <div class="flex min-w-0 flex-1 flex-col">
      <header
        class="flex h-14 shrink-0 items-center justify-between gap-3 border-b border-border/40 px-4 lg:px-6"
      >
        <div class="flex min-w-0 items-center gap-2">
          <button
            type="button"
            class="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 md:hidden"
            title="Open navigation"
            @click="mobileOpen = true"
          >
            <Menu class="h-4 w-4" />
          </button>
          <h1 class="truncate text-sm font-semibold">{{ pageTitle }}</h1>
        </div>
        <div class="flex items-center gap-2">
          <button
            v-if="authStore.canManage"
            type="button"
            class="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
            title="Reload configuration"
            @click="requestReload"
          >
            <RefreshCw class="h-4 w-4" />
          </button>
          <button
            type="button"
            class="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
            title="Toggle theme"
            @click="toggleDark()"
          >
            <component :is="isDark ? Moon : Sun" class="h-4 w-4" />
          </button>
          <div class="h-6 w-px bg-border/60"></div>
          <NavbarRight />
        </div>
      </header>

      <main class="flex-1 overflow-y-auto px-4 py-6 lg:px-6 scroll-smooth">
        <div class="mx-auto flex min-h-full w-full max-w-[1600px] flex-col">
          <div class="flex-1">
            <RouterView />
          </div>
        </div>
      </main>
    </div>

    <!-- Reload Confirmation Dialog -->
    <UiDialog
      :open="showReloadModal"
      @update:open="val => (showReloadModal = val)"
      title="Reload Configuration?"
    >
      <template #description>
        This will reload the configuration from disk. Any removed programs will be stopped,
        and new ones will be added. Running programs with changed configurations may not
        restart automatically.
      </template>
      <div class="mt-2 flex items-center gap-3">
        <UiButton variant="secondary" @click="showReloadModal = false">Cancel</UiButton>
        <UiButton variant="destructive" @click="confirmReload">
          <RefreshCw class="h-4 w-4" />
          Reload Config
        </UiButton>
      </div>
    </UiDialog>
  </div>
</template>
