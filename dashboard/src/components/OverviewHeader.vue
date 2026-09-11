<script setup lang="ts">
import SystemMetricsPanel from '@/components/SystemMetricsPanel.vue';

defineProps<{
  stats: { total: number; running: number; stopped: number; problem: number };
  activeFilter: string;
}>();

const emit = defineEmits<{ filter: [status: string] }>();

const filters = [
  { key: 'ALL', label: 'All', valueKey: 'total' as const, dot: 'bg-info', active: 'bg-info/10 text-info ring-info/25' },
  { key: 'Running', label: 'Running', valueKey: 'running' as const, dot: 'bg-success', active: 'bg-success/10 text-success ring-success/25' },
  { key: 'Stopped', label: 'Stopped', valueKey: 'stopped' as const, dot: 'bg-muted-foreground/40', active: 'bg-muted text-foreground ring-foreground/10' },
  { key: 'Fatal', label: 'Issues', valueKey: 'problem' as const, dot: 'bg-destructive', active: 'bg-destructive/10 text-destructive ring-destructive/20', warn: true },
];

function toggleFilter(key: string) {
  emit('filter', key);
}
</script>

<template>
  <div class="flex flex-wrap items-center gap-x-3 gap-y-1.5 px-2 py-1.5 rounded-md bg-muted/30 shrink-0" aria-label="Overview">
    <SystemMetricsPanel />

    <div class="hidden sm:block w-px h-5 bg-border/60 shrink-0" aria-hidden="true" />

    <div class="flex flex-wrap items-center gap-1 p-1 rounded-lg bg-muted/50 sm:ml-auto" role="tablist" aria-label="Filter by status">
      <button
        v-for="item in filters"
        :key="item.key"
        type="button" role="tab"
        :aria-selected="activeFilter === item.key"
        class="inline-flex items-center gap-1.5 px-2.5 sm:px-3 py-1 rounded-md text-xs font-medium transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
        :class="[
          activeFilter === item.key
            ? `ring-1 shadow-sm ${item.active}`
            : 'text-muted-foreground/60 hover:text-foreground/80 hover:bg-card/60',
          item.warn && stats[item.valueKey] > 0 && activeFilter !== item.key ? 'text-destructive hover:text-destructive' : '',
        ]"
        @click="toggleFilter(item.key)"
      >
        <span class="w-1.5 h-1.5 rounded-full shrink-0" :class="item.dot" aria-hidden="true" />
        <span class="hidden sm:inline">{{ item.label }}</span>
        <span class="sm:hidden">{{ item.label === 'Running' ? 'Run' : item.label === 'Stopped' ? 'Stop' : item.label === 'Issues' ? 'Issue' : item.label }}</span>
        <span class="tabular-nums font-semibold">{{ stats[item.valueKey] }}</span>
      </button>
    </div>
  </div>
</template>
