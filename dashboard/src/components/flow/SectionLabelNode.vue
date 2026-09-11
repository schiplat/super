<script setup lang="ts">
defineProps<{
  data: { label: string; hint?: string };
}>();
</script>

<template>
  <div class="section-label-node pointer-events-none select-none">
    <span class="section-label-node__title">{{ data.label }}</span>
    <span v-if="data.hint" class="section-label-node__hint">{{ data.hint }}</span>
  </div>
</template>

<style scoped>
.section-label-node {
  display: flex;
  align-items: baseline;
  gap: 8px;
  white-space: nowrap;
}

.section-label-node__title {
  font-size: 13px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: hsl(var(--muted-foreground));
  opacity: 0.65;
}

.section-label-node__hint {
  font-size: 12px;
  font-weight: 500;
  color: hsl(var(--muted-foreground));
  opacity: 0.50;
}
</style>
