/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly DEV: boolean;
  /** Dev-only: URL of a plugin UI bundle served outside this shell. */
  readonly VITE_DEV_PLUGIN_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

interface Window {
  /** Injected by superd: plugin ids whose UI bundles are available. */
  __SUPER_PLUGINS__?: string[];
  /** Host bridge (installed by src/slots/bridge.ts before plugin load). */
  __SUPER_CORE__?: unknown;
  /** Shared Vue runtime primitives for plugin bundles. */
  __SUPER_VUE__?: unknown;
}
