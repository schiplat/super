<script setup lang="ts">
import { computed } from 'vue';
import type { ProcessStatus } from '@/types';
import UiBadge from '@/components/ui/UiBadge.vue';

const props = defineProps<{
  status: ProcessStatus;
  /** When set with status Running, process is alive but health_check is failing. */
  healthError?: string | null;
}>();

const isUnhealthy = computed(
  () => props.status === 'Running' && !!props.healthError,
);

const label = computed(() => (isUnhealthy.value ? 'Unhealthy' : props.status));

const variant = computed((): 'success' | 'warning' | 'default' | 'destructive' | 'info' | 'notice' => {
  if (isUnhealthy.value) return 'destructive';
  const map: Record<ProcessStatus, 'success' | 'warning' | 'default' | 'destructive' | 'info' | 'notice'> = {
    Healthy: 'success',
    Running: 'warning',
    Stopped: 'default',
    Fatal: 'destructive',
    Backoff: 'warning',
    Starting: 'info',
    Stopping: 'warning',
    Waiting: 'notice',
  };
  return map[props.status] || 'default';
});
</script>

<template>
  <span :title="healthError || undefined" class="inline-flex">
    <UiBadge :variant="variant" :dot="status === 'Healthy'">
      {{ label }}
    </UiBadge>
  </span>
</template>
