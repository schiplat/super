<script setup lang="ts">
const props = defineProps<{
  title: string;
  value: number;
  icon: any;
  colorClass: string;
  isActive?: boolean;
  compact?: boolean;
}>();

defineEmits(['click']);
</script>

<template>
  <div
    class="bg-card border rounded-xl cursor-pointer transition-all duration-200 group relative overflow-hidden h-full"
    :class="[
      compact ? 'p-3' : 'p-5',
      isActive
        ? 'border-foreground/20 ring-1 ring-foreground/10 shadow-md'
        : 'border-border hover:border-border hover:shadow-sm'
    ]"
    @click="$emit('click')"
  >
    <div class="flex justify-between items-start">
      <div>
        <p class="font-medium text-muted-foreground" :class="compact ? 'text-xs' : 'text-sm'">{{ title }}</p>
        <h3 class="font-bold tracking-tight text-foreground" :class="compact ? 'text-2xl mt-1' : 'text-3xl mt-2'">{{ value }}</h3>
      </div>
      <div class="rounded-lg transition-colors bg-muted" :class="[colorClass, compact ? 'p-2' : 'p-3']">
        <component :is="icon" :class="compact ? 'w-5 h-5' : 'w-6 h-6'" />
      </div>
    </div>
    <div v-if="!compact" class="absolute -right-4 -bottom-4 w-24 h-24 rounded-full opacity-5 pointer-events-none transition-transform group-hover:scale-110" :class="colorClass.replace('text-', 'bg-')"></div>
  </div>
</template>
