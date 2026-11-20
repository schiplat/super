<script setup lang="ts">
/**
 * L1 surface (card). One outline (ring), one soft shadow, 16px radius.
 * New pages use this instead of `surface-card` + an ad-hoc border/shadow.
 */
defineProps<{
  title?: string
  description?: string
}>()
</script>

<template>
  <section
    class="rounded-2xl bg-card ring-1 ring-border/60 shadow-[0_1px_2px_hsl(220_20%_20%/0.04),0_2px_8px_hsl(220_20%_20%/0.03)]"
  >
    <header
      v-if="title || description || $slots.actions"
      class="flex items-start justify-between gap-4 px-6 pt-6"
    >
      <div class="min-w-0">
        <h2 v-if="title" class="text-sm font-semibold tracking-tight">{{ title }}</h2>
        <p v-if="description" class="mt-1 text-sm text-muted-foreground">{{ description }}</p>
      </div>
      <div v-if="$slots.actions" class="flex shrink-0 items-center gap-2">
        <slot name="actions" />
      </div>
    </header>
    <div :class="title || description || $slots.actions ? 'px-6 pb-6 pt-4' : 'p-6'">
      <slot />
      <div v-if="$slots.footer" class="mt-6 border-t border-border/40 pt-5">
        <slot name="footer" />
      </div>
    </div>
  </section>
</template>
