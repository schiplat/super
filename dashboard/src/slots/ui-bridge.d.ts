/**
 * @super/ui-bridge — shared types between the OSS shell and plugin UI bundles.
 *
 * Zero runtime code: both sides `import type` from here so a plugin can be
 * type-checked against the exact contract the shell serves. Keep this file
 * dependency-free; it is published verbatim.
 */

export type SlotName =
  | 'process.actions'
  | 'process.detail.tabs';

export interface ProcessContext {
  id: string;
  name: string;
  status: string;
  pid?: number;
  [key: string]: unknown;
}

export interface SlotContext {
  process?: ProcessContext;
  [key: string]: unknown;
}

/** Component type is opaque here; plugins implement `defineComponent`/setup fns. */
export interface SlotExtension {
  id?: string;
  component: unknown;
}

export interface UiComponents {
  UiButton: unknown;
  UiBadge: unknown;
  UiDialog: unknown;
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
  registerSlot(name: SlotName, ext: SlotExtension): () => void;
  unregisterSlot(name: SlotName, ext: SlotExtension): void;
}

export declare global {
  interface Window {
    __SUPER_CORE__?: SuperCoreBridge;
    /** Injected by superd: plugin ids whose UI bundles are available. */
    __SUPER_PLUGINS__?: string[];
  }
}
