/**
 * Slot registry — OSS-side extension points for plugin-injected UI.
 *
 * A slot is a named anchor rendered inside OSS views (e.g. process action
 * bars, detail tabs). Plugins register render functions against a slot;
 * the <Slot> component renders all registered extensions and contains
 * errors to that slot only (a broken plugin must never white-screen the
 * shell). With no registrations a slot renders nothing and costs nothing.
 */

export interface ProcessContext {
  /** Program id (UUID) as used by the REST API. */
  id: string;
  name: string;
  status: string;
  pid?: number;
}

export interface SlotContext {
  process?: ProcessContext;
  [key: string]: unknown;
}

export type SlotComponent = import('vue').Component;

export interface SlotExtension {
  /** Registration order tiebreaker; extensions render in join order. */
  id?: string;
  /** Functional component receiving the slot context. */
  component: SlotComponent;
}

type Registry = Map<string, SlotExtension[]>;

const registry: Registry = new Map();

/** Register a component into a slot. Returns an unregister function. */
export function registerSlot(
  name: SlotName,
  ext: SlotExtension,
): () => void {
  const list = registry.get(name) ?? [];
  list.push(ext);
  registry.set(name, list);
  return () => unregisterSlot(name, ext);
}

export function unregisterSlot(name: SlotName, ext: SlotExtension): void {
  const list = registry.get(name);
  if (!list) return;
  const idx = list.indexOf(ext);
  if (idx >= 0) list.splice(idx, 1);
  if (list.length === 0) registry.delete(name);
}

export function slotExtensions(name: SlotName): readonly SlotExtension[] {
  return registry.get(name) ?? [];
}

/** Open slot names (kept in one place; extended as new anchors land). */
export type SlotName =
  | 'process.actions'
  | 'process.detail.tabs';

/** Names of all slots that currently have at least one extension. */
export function populatedSlots(): SlotName[] {
  return [...registry.keys()].map((k) => k as SlotName);
}
