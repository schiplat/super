/**
 * Plugin route registry — plugins call registerRoute(); the shell applies
 * them via vue-router addRoute before first navigation.
 */
import type { Component } from 'vue';
import type { Router, RouteRecordRaw } from 'vue-router';

export interface PluginRoute {
  /** Path relative to the shell parent (e.g. `tokens`, `settings/notify`). */
  path: string;
  name?: string;
  component?: Component;
  meta?: Record<string, unknown>;
  children?: PluginRoute[];
  /** Parent route name; default `shell` (MainLayout). */
  parent?: string;
  redirect?: string;
}

type Entry = { route: PluginRoute; recordName: string };

const pending: Entry[] = [];
let routerRef: Router | null = null;
let seq = 0;

function toRaw(route: PluginRoute): RouteRecordRaw {
  const raw: RouteRecordRaw = {
    path: route.path,
    name: route.name,
    component: route.component,
    meta: route.meta,
    redirect: route.redirect,
  } as RouteRecordRaw;
  if (route.children?.length) {
    (raw as { children: RouteRecordRaw[] }).children = route.children.map(toRaw);
  }
  return raw;
}

/** Bind the live router (call once from main.ts before loading plugins). */
export function bindRouter(router: Router): void {
  routerRef = router;
  // Flush anything registered before bind (dev HMR edge case).
  for (const entry of [...pending]) {
    applyEntry(entry);
  }
}

function applyEntry(entry: Entry): void {
  if (!routerRef) return;
  const parent = entry.route.parent ?? 'shell';
  try {
    routerRef.addRoute(parent, toRaw(entry.route));
  } catch (err) {
    console.warn(`[routes] failed to add route "${entry.route.path}":`, err);
  }
}

/** Register a route under the shell layout. Returns an unregister function. */
export function registerRoute(route: PluginRoute): () => void {
  const recordName = route.name ?? `__plugin_route_${++seq}`;
  const entry: Entry = {
    route: { ...route, name: recordName },
    recordName,
  };
  pending.push(entry);
  if (routerRef) applyEntry(entry);

  return () => {
    const idx = pending.indexOf(entry);
    if (idx >= 0) pending.splice(idx, 1);
    if (routerRef && entry.recordName) {
      try {
        routerRef.removeRoute(entry.recordName);
      } catch {
        /* already gone */
      }
    }
  };
}
