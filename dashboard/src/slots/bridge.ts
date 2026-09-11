/**
 * Host bridge exposed to plugin UI bundles as `window.__SUPER_CORE__`.
 *
 * Plugins must not bundle Vue, vue-router, host components, or an HTTP stack:
 * they build against this bridge and stay small. Only the pieces declared here
 * are a stable contract; everything else in the shell can change freely.
 */
import type { App, Component } from 'vue';
import * as Vue from 'vue';
import * as VueRouter from 'vue-router';
import type { Router } from 'vue-router';
import { RouterLink, RouterView } from 'vue-router';
import apiClient from '@/api/client';
import { useAuthStore } from '@/stores/auth';
import { useCapabilitiesStore } from '@/stores/capabilities';
import { alertDialog, confirmDialog } from '@/lib/app-dialog';
import {
  registerSlot,
  unregisterSlot,
  slotExtensions,
  type SlotContext,
  type SlotExtension,
  type SlotName,
} from './registry';
import { bindRouter, registerRoute, type PluginRoute } from './routes';
import UiButton from '@/components/ui/UiButton.vue';
import UiBadge from '@/components/ui/UiBadge.vue';
import UiDialog from '@/components/ui/UiDialog.vue';
import UiInput from '@/components/ui/UiInput.vue';
import UiSwitch from '@/components/ui/UiSwitch.vue';
import UiField from '@/components/ui/UiField.vue';

export interface SuperCoreBridge {
  version: string;
  /** Mount the plugin's root component (rendered invisibly; gives app context). */
  app: App | null;
  http: typeof apiClient;
  components: {
    UiButton: Component;
    UiBadge: Component;
    UiDialog: Component;
    UiInput: Component;
    UiSwitch: Component;
    UiField: Component;
    RouterLink: Component;
    RouterView: Component;
  };
  dialogs: {
    alert: typeof alertDialog;
    confirm: typeof confirmDialog;
  };
  registerSlot: typeof registerSlot;
  unregisterSlot: typeof unregisterSlot;
  slotExtensions: typeof slotExtensions;
  registerRoute: typeof registerRoute;
  stores: {
    auth: () => ReturnType<typeof useAuthStore>;
    capabilities: () => ReturnType<typeof useCapabilitiesStore>;
  };
  router: Router | null;
}

let bridge: SuperCoreBridge | null = null;

/** Install the bridge exactly once, before plugin bundles load. */
export function installBridge(app: App, version: string, router: Router): SuperCoreBridge {
  if (bridge) return bridge;
  bindRouter(router);
  bridge = {
    version,
    app,
    http: apiClient,
    components: {
      UiButton,
      UiBadge,
      UiDialog,
      UiInput,
      UiSwitch,
      UiField,
      RouterLink,
      RouterView,
    },
    dialogs: { alert: alertDialog, confirm: confirmDialog },
    registerSlot: registerSlot as (name: string, ext: SlotExtension) => () => void,
    unregisterSlot: unregisterSlot as (name: string, ext: SlotExtension) => void,
    slotExtensions: slotExtensions as (name: string) => readonly SlotExtension[],
    registerRoute: registerRoute as (route: PluginRoute) => () => void,
    stores: {
      auth: () => useAuthStore(),
      capabilities: () => useCapabilitiesStore(),
    },
    router,
  };
  const w = window as unknown as Record<string, unknown>;
  w.__SUPER_CORE__ = bridge;
  // Full Vue / vue-router namespaces for plugin IIFE externals (globals).
  w.__SUPER_VUE__ = Vue;
  w.__SUPER_VUE_ROUTER__ = VueRouter;
  return bridge;
}

export function getBridge(): SuperCoreBridge | null {
  return bridge;
}

export type { SlotContext, SlotExtension, SlotName, PluginRoute };
