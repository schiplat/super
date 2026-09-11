/** Injected by superd into index.html (see `inject_ui_config`). Vite uses `index.html` defaults. */
export interface SuperRuntimeConfig {
  edition: string;
  auth_required: boolean;
  version: string;
}

export function getSuperConfig(): SuperRuntimeConfig {
  const raw = (window as unknown as { __SUPER_CONFIG__?: Partial<SuperRuntimeConfig> })
    .__SUPER_CONFIG__;
  return {
    edition: String(raw?.edition ?? 'oss'),
    auth_required: raw?.auth_required === true,
    version: String(raw?.version ?? 'unknown'),
  };
}
