<script setup lang="ts">
import { computed } from 'vue'

/**
 * Status chip: one radius for every rectangular chip (8px), one tint, no
 * border. Colour means state only.
 */
const props = withDefaults(
  defineProps<{
    tone?: 'neutral' | 'info' | 'success' | 'warning' | 'danger'
  }>(),
  { tone: 'neutral' },
)

const toneClass = computed(
  () =>
    ({
      neutral: 'bg-muted/40 text-muted-foreground',
      info: 'bg-primary/10 text-primary',
      success: 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300',
      warning: 'bg-amber-500/10 text-amber-700 dark:text-amber-300',
      danger: 'bg-destructive/10 text-destructive',
    })[props.tone],
)
</script>

<template>
  <span
    class="inline-flex items-center gap-1 rounded-lg px-1.5 py-0.5 text-[11px] font-semibold uppercase tracking-wide"
    :class="toneClass"
  >
    <slot />
  </span>
</template>
