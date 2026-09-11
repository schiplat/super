<script setup lang="ts">
import { computed } from 'vue'
import { cn } from '@/lib/utils'

const props = withDefaults(
  defineProps<{
    /** Visual tone of the badge */
    variant?: 'default' | 'success' | 'warning' | 'destructive' | 'info' | 'notice'
    size?: 'sm' | 'default'
    dot?: boolean
  }>(),
  {
    variant: 'default',
    size: 'default',
    dot: false,
  },
)

const variantClass = computed(() => {
  const map: Record<string, string> = {
    default: 'bg-muted text-muted-foreground',
    success: 'bg-success/10 text-success',
    warning: 'bg-warning/10 text-warning',
    destructive: 'bg-destructive/10 text-destructive',
    info: 'bg-info/10 text-info',
    notice: 'bg-notice/10 text-notice',
  }
  return map[props.variant] || map.default
})
</script>

<template>
  <span
    :class="
      cn(
        'inline-flex items-center gap-1 rounded-md font-medium',
        size === 'sm' ? 'px-1.5 py-0.5 text-[0.6875rem]' : 'px-2 py-0.5 text-xs',
        variantClass,
      )
    "
  >
    <span v-if="dot" class="inline-block h-1.5 w-1.5 rounded-full bg-current opacity-60" />
    <slot />
  </span>
</template>
