<script setup lang="ts">
/**
 * <Slot name="process.actions" :context="{ process }" />
 *
 * Renders every extension registered for `name`. Each extension is isolated:
 * a throwing plugin renders as nothing (console.warn only) — the host view
 * must never break because of an extension. Renders no DOM when empty.
 */
import { computed, onErrorCaptured, ref } from 'vue';
import { slotExtensions, type SlotContext, type SlotExtension, type SlotName } from './registry';

const props = defineProps<{
  name: SlotName;
  context?: SlotContext;
}>();

/** Indices of extensions whose render threw; they stay hidden. */
const failed = ref<Set<number>>(new Set());

// Contain async/setup errors per-extension (defense in depth alongside the
// per-extension v-error boundaries below).
onErrorCaptured((err, _instance, info) => {
  console.warn(`[Slot:${props.name}] extension error (${info}):`, err);
  return false;
});

const extensions = computed<readonly SlotExtension[]>(() => {
  // Re-evaluate on registration changes: registry is a plain Map, so we key
  // on the joined ids to stay cheap while remaining reactive per render.
  void failed.value;
  return slotExtensions(props.name);
});

function onExtensionError(idx: number) {
  console.warn(`[Slot:${props.name}] extension #${idx} failed to render; hiding it.`);
  failed.value = new Set(failed.value).add(idx);
}
</script>

<template>
  <template v-for="(ext, idx) in extensions" :key="ext.id ?? idx">
    <component
      :is="ext.component"
      v-if="!failed.has(idx)"
      :context="context"
      @error="onExtensionError(idx)"
    />
  </template>
</template>
