<script setup lang="ts">
/** Empty state for list surfaces: never a bare "no data" line. */
defineProps<{
  title: string
  hint?: string
}>()
</script>

<template>
  <div class="px-5 py-12 text-center">
    <p class="text-sm font-medium text-foreground/80">{{ title }}</p>
    <p v-if="hint" class="mt-1 text-xs text-muted-foreground">{{ hint }}</p>
    <div v-if="$slots.default" class="mt-4 flex justify-center">
      <slot />
    </div>
  </div>
</template>
