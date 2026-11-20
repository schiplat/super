<script setup lang="ts">
import { computed } from 'vue'

/**
 * Semantic notice: one tinted surface, no border (see the surface system).
 * Tones are state, never decoration.
 */
const props = withDefaults(
  defineProps<{
    tone?: 'info' | 'success' | 'warning' | 'danger'
    title?: string
  }>(),
  { tone: 'info' },
)

const toneClass = computed(
  () =>
    ({
      info: 'bg-primary/10 text-primary',
      success: 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300',
      warning: 'bg-amber-500/10 text-amber-700 dark:text-amber-300',
      danger: 'bg-destructive/10 text-destructive',
    })[props.tone],
)
</script>

<template>
  <div class="rounded-xl px-3 py-2 text-sm" :class="toneClass" role="status">
    <p v-if="title" class="font-semibold">{{ title }}</p>
    <div :class="title ? 'mt-1' : ''" class="text-sm leading-snug">
      <slot />
    </div>
  </div>
</template>
