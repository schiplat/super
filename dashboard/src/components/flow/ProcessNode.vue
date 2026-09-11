<script setup lang="ts">
import { computed, inject, type Ref } from 'vue';
import { Handle, Position } from '@vue-flow/core';
import { Database, Server, Globe, Cpu, Box, Terminal, AlertCircle } from 'lucide-vue-next';
import type { Program } from '@/types';
import { PROCESS_NODE_HEIGHT, PROCESS_NODE_WIDTH } from './processNodeLayout';

const props = defineProps<{
  data: { fullData: Program; status: string; variant?: 'chain' | 'standalone' };
}>();

const topologyFullscreen = inject<Ref<boolean>>('topologyFullscreen');
const isFullscreen = computed(() => topologyFullscreen?.value ?? false);

const p = computed(() => props.data.fullData);
const isChain = computed(() => props.data.variant === 'chain');

const statusColorClass = computed(() => {
  if (p.value.status === 'Running' && p.value.health_error) return 'bg-destructive';
  switch (p.value.status) {
    case 'Running': return 'bg-warning';
    case 'Healthy': return 'bg-success';
    case 'Waiting': return 'bg-info';
    case 'Fatal': return 'bg-destructive';
    case 'Backoff': return 'bg-warning';
    case 'Stopped': return 'bg-border';
    default: return 'bg-foreground/20';
  }
});

const iconComponent = computed(() => {
  const name = p.value.name.toLowerCase();
  if (name.includes('db') || name.includes('redis') || name.includes('sql')) return Database;
  if (name.includes('web') || name.includes('http') || name.includes('front')) return Globe;
  if (name.includes('api') || name.includes('back')) return Server;
  if (name.includes('worker') || name.includes('queue')) return Cpu;
  if (name.includes('log')) return Terminal;
  return Box;
});

const groupLabel = computed(() => p.value.group || (isChain.value ? 'flow' : 'standalone'));

function formatMemBytes(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)}G`;
  if (bytes >= 1024 ** 2) return `${Math.round(bytes / 1024 / 1024)}M`;
  return `${Math.round(bytes / 1024)}K`;
}

const usageLabel = computed(() => {
  const cpu = p.value.cpu_usage;
  const mem = p.value.mem_usage;
  if (cpu == null && mem == null) return null;
  const cpuText = cpu != null ? `${cpu.toFixed(0)}%` : '—';
  const memText = mem != null ? formatMemBytes(mem) : '—';
  return `${cpuText} · ${memText}`;
});

const limitsLabel = computed(() => {
  const rl = p.value.resource_limits;
  if (!rl) return null;
  const parts: string[] = [];
  if (rl.cpu_quota != null && rl.cpu_quota > 0) {
    const q = rl.cpu_quota;
    parts.push(`${q % 1 === 0 ? q.toFixed(0) : q.toFixed(1)}%`);
  }
  if (rl.memory_limit != null && rl.memory_limit > 0) {
    parts.push(formatMemBytes(rl.memory_limit));
  }
  return parts.length ? parts.join(' · ') : null;
});
</script>

<template>
  <div
    class="process-node-card relative box-border rounded-2xl shadow-sm transition-all duration-200"
    :class="[
      isFullscreen
        ? 'bg-card border border-solid border-border shadow-md hover:shadow-lg'
        : 'bg-muted/40 border border-dashed border-border/80 hover:shadow-md hover:bg-muted/60',
    ]"
    :style="{ width: `${PROCESS_NODE_WIDTH}px`, height: `${PROCESS_NODE_HEIGHT}px` }"
  >
    <Handle v-if="isChain" type="target" :position="Position.Left" class="!w-2 !h-2 !border-none" :class="isFullscreen ? '!bg-foreground/50' : '!bg-foreground/25'" />
    <Handle v-if="isChain" type="source" :position="Position.Right" class="!w-2 !h-2 !border-none" :class="isFullscreen ? '!bg-foreground/50' : '!bg-foreground/25'" />

    <div class="flex h-full items-start gap-2 px-2.5 py-2">
      <span class="w-2 h-2 rounded-full shrink-0 mt-1" :class="statusColorClass"></span>
      <component :is="iconComponent" class="w-3 h-3 shrink-0 mt-0.5" :class="isFullscreen ? 'text-muted-foreground' : 'text-muted-foreground/50'" />
      <div class="min-w-0 flex-1 flex flex-col justify-start gap-0.5 overflow-hidden">
        <span class="font-semibold text-xs leading-tight truncate text-foreground" :title="p.name">{{ p.name }}</span>
        <span class="text-xs uppercase leading-tight truncate tracking-wider" :class="isFullscreen ? 'text-muted-foreground/60' : 'text-muted-foreground/40'" :title="groupLabel">{{ groupLabel }}</span>
        <span class="text-xs font-mono leading-tight whitespace-nowrap" :class="usageLabel ? (isFullscreen ? 'text-foreground/65' : 'text-foreground/45') : (isFullscreen ? 'text-muted-foreground/35' : 'text-muted-foreground/20')" :title="usageLabel || 'No usage data'">{{ usageLabel || '— · —' }}</span>
        <span class="text-xs font-mono leading-tight whitespace-nowrap" :class="limitsLabel ? (isFullscreen ? 'text-violet-500' : 'text-violet-500/70') : (isFullscreen ? 'text-muted-foreground/35' : 'text-muted-foreground/20')" :title="limitsLabel ? `Isolation cap: ${limitsLabel}` : 'No resource limits'">{{ limitsLabel ? `cap ${limitsLabel}` : 'cap —' }}</span>
      </div>
    </div>

    <div v-if="(p as any).last_error" class="absolute top-1.5 right-1.5 text-destructive animate-pulse">
      <AlertCircle class="w-2.5 h-2.5" />
    </div>
  </div>
</template>
