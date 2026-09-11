<script setup lang="ts">
defineProps<{
  label: string
  hint?: string
  required?: boolean
}>()
</script>

<template>
  <label class="block space-y-1.5">
    <span class="flex items-baseline gap-1 text-sm font-semibold tracking-tight text-foreground">
      {{ label }}
      <span v-if="required" class="text-destructive" aria-hidden="true">*</span>
    </span>
    <slot />
    <span v-if="hint" class="type-meta block">{{ hint }}</span>
  </label>
</template>
