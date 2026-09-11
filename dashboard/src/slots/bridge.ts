/**
 * Host bridge exposed to plugin UI bundles as `window.__SUPER_CORE__`.
 *
 * Plugins must not bundle Vue, host components, or an HTTP stack: they build
 * against this bridge (typed by @super/ui-bridge) and stay a few KB. Only the
 * pieces declared here are a stable contract; everything else in the shell
 * can change freely.
 */
import type { App, Component } from 'vue';
import {
  computed,
  defineComponent,
  h,
  onMounted,
  onUnmounted,
  ref,
  watch,
} from 'vue';
import apiClient from '@/api/client';
import { useAuthStore } from '@/stores/auth';
import { useCapabilitiesStore } from '@/stores/capabilities';
import {
  registerSlot,
  unregisterSlot,
  slotExtensions,
  type SlotContext,
  type SlotExtension,
  type SlotName,
} from './registry';
import UiButton from '@/components/ui/UiButton.vue';
import UiBadge from '@/components/ui/UiBadge.vue';
import UiDialog from '@/components/ui/UiDialog.vue';

export interface SuperCoreBridge {
  version: string;
  /** Mount the plugin's root component (rendered invisibly; gives app context). */
  app: App | null;
  http: typeof apiClient;
  components: {
    UiButton: Component;
    UiBadge: Component;
    UiDialog: Component;
  };
  registerSlot: typeof registerSlot;
  unregisterSlot: typeof unregisterSlot;
  slotExtensions: typeof slotExtensions;
  stores: {
    auth: () => ReturnType<typeof useAuthStore>;
    capabilities: () => ReturnType<typeof useCapabilitiesStore>;
  };
}

let bridge: SuperCoreBridge | null = null;

/** Install the bridge exactly once, before plugin bundles load. */
export function installBridge(app: App, version: string): SuperCoreBridge {
  if (bridge) return bridge;
  bridge = {
    version,
    app,
    http: apiClient,
    components: { UiButton, UiBadge, UiDialog },
    registerSlot: registerSlot as (name: string, ext: SlotExtension) => () => void,
    unregisterSlot: unregisterSlot as (name: string, ext: SlotExtension) => void,
    slotExtensions: slotExtensions as (name: string) => readonly SlotExtension[],
    stores: {
      auth: () => useAuthStore(),
      capabilities: () => useCapabilitiesStore(),
    },
  };
  const w = window as unknown as Record<string, unknown>;
  w.__SUPER_CORE__ = bridge;
  // Shared Vue runtime for plugin bundles (they must not bundle their own).
  w.__SUPER_VUE__ = { defineComponent, h, ref, computed, watch, onMounted, onUnmounted };
  return bridge;
}

export function getBridge(): SuperCoreBridge | null {
  return bridge;
}

export type { SlotContext, SlotExtension, SlotName };
