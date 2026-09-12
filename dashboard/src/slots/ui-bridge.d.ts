/**
 * @super/ui-bridge — shared types between the OSS shell and plugin UI bundles.
 *
 * ⚠ STABLE PLUGIN CONTRACT — do not change casually.
 * Paid / third-party UI plugins type-check and register against this file.
 * Renaming slots, dropping ProcessContext fields (e.g. `pid`), or altering
 * SuperCoreBridge methods will break those plugins without a compile error in
 * this repo. Extend only; removals need an intentional ABI/docs bump.
 * CI: `dashboard` vitest (`slots.spec.ts`) asserts this surface — do not delete
 * or skip those tests to “make the PR green”.
 *
 * Zero runtime code: both sides `import type` from here so a plugin can be
 * type-checked against the exact contract the shell serves. Keep this file
 * dependency-free; it is published verbatim.
 */

export type SlotName =
  | 'process.actions'
  | 'process.detail.tabs'
  | 'nav.account'
  | 'nav.manage'
  | 'nav.mobile';

export interface ProcessContext {
  id: string;
  name: string;
  status: string;
  /** Optional at runtime (stopped programs); still part of the contract shape. */
  pid?: number;
  [key: string]: unknown;
}

export interface SlotContext {
  process?: ProcessContext;
  /** Nav slots: push a path and optionally close the open menu. */
  navigate?: (path: string) => void;
  close?: () => void;
  /** process.detail.tabs: register a named tab + panel component. */
  registerTab?: (tab: {
    id: string;
    label: string;
    component: unknown;
  }) => () => void;
  activeTab?: string;
  [key: string]: unknown;
}

/** Component type is opaque here; plugins implement `defineComponent`/setup fns. */
export interface SlotExtension {
  id?: string;
  component: unknown;
}

export interface PluginRoute {
  path: string;
  name?: string;
  component: unknown;
  meta?: Record<string, unknown>;
  children?: PluginRoute[];
  parent?: string;
  redirect?: string;
}

export interface UiComponents {
  UiButton: unknown;
  UiBadge: unknown;
  UiDialog: unknown;
  UiInput: unknown;
  UiSwitch: unknown;
  UiField: unknown;
  RouterLink: unknown;
  RouterView: unknown;
}

export interface SuperCoreBridge {
  version: string;
  app: unknown;
  /** Axios instance with the auth interceptor already wired. */
  http: {
    get(url: string, config?: unknown): Promise<unknown>;
    post(url: string, data?: unknown, config?: unknown): Promise<unknown>;
    put(url: string, data?: unknown, config?: unknown): Promise<unknown>;
    delete(url: string, config?: unknown): Promise<unknown>;
  };
  components: UiComponents;
  dialogs: {
    alert(message: string, opts?: unknown): Promise<void>;
    confirm(message: string, opts?: unknown): Promise<boolean>;
  };
  registerSlot(name: SlotName, ext: SlotExtension): () => void;
  unregisterSlot(name: SlotName, ext: SlotExtension): void;
  registerRoute(route: PluginRoute): () => void;
  stores: {
    auth(): unknown;
    capabilities(): unknown;
  };
  router: unknown;
}

export declare global {
  interface Window {
    __SUPER_CORE__?: SuperCoreBridge;
    /** Full Vue namespace (IIFE external `vue` → `__SUPER_VUE__`). */
    __SUPER_VUE__?: unknown;
    /** Full vue-router namespace (IIFE external → `__SUPER_VUE_ROUTER__`). */
    __SUPER_VUE_ROUTER__?: unknown;
    /** Injected by superd: plugin ids whose UI bundles are available. */
    __SUPER_PLUGINS__?: string[];
  }
}
