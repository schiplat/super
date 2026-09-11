/**
 * Plugin UI bundle loader.
 *
 * `superd` injects `window.__SUPER_PLUGINS__` (list of plugin ids with UI)
 * into index.html alongside __SUPER_CONFIG__. Each plugin serves its bundle
 * at /api/v1/plugins/{id}/ui.js (+ optional ui.css); bundles self-register
 * into slots/routes via window.__SUPER_CORE__. Failure of any single bundle
 * must not affect the shell or other plugins.
 */
import { populatedSlots } from './registry';

declare global {
  interface Window {
    __SUPER_PLUGINS__?: string[];
  }
}

function injectStylesheet(href: string): void {
  if (document.querySelector(`link[data-super-plugin-css="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  link.dataset.superPluginCss = href;
  document.head.appendChild(link);
}

export async function loadPluginUIs(): Promise<void> {
  const ids = window.__SUPER_PLUGINS__ ?? [];
  await Promise.allSettled(
    ids.map(async (id) => {
      const enc = encodeURIComponent(id);
      // CSS first so first paint of plugin views has utilities available.
      injectStylesheet(`/api/v1/plugins/${enc}/ui.css`);
      try {
        await import(/* @vite-ignore */ `/api/v1/plugins/${enc}/ui.js`);
      } catch (err) {
        console.warn(`[plugins] failed to load UI bundle for "${id}":`, err);
      }
    }),
  );
}

/** Dev-only hot path: load a bundle straight from another Vite dev server. */
export async function loadDevPluginUI(): Promise<void> {
  const url = import.meta.env.VITE_DEV_PLUGIN_URL as string | undefined;
  const cssUrl = import.meta.env.VITE_DEV_PLUGIN_CSS_URL as string | undefined;
  if (!import.meta.env.DEV || !url) return;
  if (cssUrl) injectStylesheet(cssUrl);
  try {
    await import(/* @vite-ignore */ url);
    console.info(`[plugins] dev bundle loaded from ${url}; slots active:`, populatedSlots());
  } catch (err) {
    console.warn(`[plugins] dev bundle failed to load from ${url}:`, err);
  }
}
