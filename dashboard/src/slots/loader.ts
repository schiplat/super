/**
 * Plugin UI bundle loader.
 *
 * `superd` injects `window.__SUPER_PLUGINS__` (list of plugin ids with UI)
 * into index.html alongside __SUPER_CONFIG__. Each plugin serves its bundle
 * at /api/v1/plugins/{id}/ui.js; bundles self-register into slots via
 * window.__SUPER_CORE__.registerSlot. Failure of any single bundle must not
 * affect the shell or other plugins.
 */
import { populatedSlots } from './registry';

declare global {
  interface Window {
    __SUPER_PLUGINS__?: string[];
  }
}

export async function loadPluginUIs(): Promise<void> {
  const ids = window.__SUPER_PLUGINS__ ?? [];
  await Promise.allSettled(
    ids.map(async (id) => {
      try {
        await import(/* @vite-ignore */ `/api/v1/plugins/${encodeURIComponent(id)}/ui.js`);
      } catch (err) {
        console.warn(`[plugins] failed to load UI bundle for "${id}":`, err);
      }
    }),
  );
}

/** Dev-only hot path: load a bundle straight from another Vite dev server. */
export async function loadDevPluginUI(): Promise<void> {
  const url = import.meta.env.VITE_DEV_PLUGIN_URL;
  if (!import.meta.env.DEV || !url) return;
  try {
    await import(/* @vite-ignore */ url);
    console.info(`[plugins] dev bundle loaded from ${url}; slots active:`, populatedSlots());
  } catch (err) {
    console.warn(`[plugins] dev bundle failed to load from ${url}:`, err);
  }
}
