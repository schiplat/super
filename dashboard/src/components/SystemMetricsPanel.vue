<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue';
import { Server } from 'lucide-vue-next';
import { API_PATHS } from '@/api/paths';
import apiClient from '@/api/client';

interface SystemStats {
  cpu_percent: number;
  memory_used_bytes: number;
  memory_total_bytes: number;
  timestamp: number;
}

const current = ref<SystemStats | null>(null);
const cpuHistory = ref<number[]>([]);
const memHistory = ref<number[]>([]);
const MAX_POINTS = 40;
const CHART_MAX = 100;

let timer: ReturnType<typeof setInterval> | null = null;

async function fetchStats() {
  if (!localStorage.getItem('super_token')) return;
  try {
    const res = await apiClient.get(API_PATHS.SYSTEM.STATS);
    const data: SystemStats = res.data;
    current.value = data;
    cpuHistory.value = [...cpuHistory.value, data.cpu_percent].slice(-MAX_POINTS);
    const memPct = data.memory_total_bytes > 0 ? (data.memory_used_bytes / data.memory_total_bytes) * 100 : 0;
    memHistory.value = [...memHistory.value, memPct].slice(-MAX_POINTS);
  } catch { /* best-effort */ }
}

function sparkPoints(values: number[]): string {
  if (values.length < 2) return '';
  const step = 48 / (values.length - 1);
  return values.map((v, i) => {
    const x = i * step;
    const y = 10 - (Math.min(Math.max(v, 0), CHART_MAX) / CHART_MAX) * 8;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(' ');
}

const cpuLabel = computed(() => current.value ? `${current.value.cpu_percent.toFixed(1)}%` : '—');
const memPctLabel = computed(() => {
  if (!current.value || current.value.memory_total_bytes === 0) return '—';
  const pct = (current.value.memory_used_bytes / current.value.memory_total_bytes) * 100;
  return `${pct.toFixed(0)}%`;
});

const cpuPoints = computed(() => sparkPoints(cpuHistory.value));
const memPoints = computed(() => sparkPoints(memHistory.value));

onMounted(() => { fetchStats(); timer = setInterval(fetchStats, 3000); });
onUnmounted(() => { if (timer) clearInterval(timer); });
</script>

<template>
  <div class="flex items-center gap-2 sm:gap-2.5 min-w-0 shrink-0" aria-label="superd host machine resources">
    <div class="flex items-center gap-1 shrink-0 pr-1 sm:pr-1.5 border-r border-border/50" title="Resources on the machine running superd">
      <Server class="w-3 h-3 text-muted-foreground/40 shrink-0" aria-hidden="true" />
      <span class="text-xs sm:text-xs font-medium text-muted-foreground/50 uppercase tracking-wide leading-none">Host</span>
    </div>
    <div class="flex items-center gap-1.5 min-w-0">
      <span class="text-xs font-medium text-muted-foreground/40 uppercase tracking-wide shrink-0">CPU</span>
      <span class="text-xs font-mono font-semibold tabular-nums text-foreground shrink-0">{{ cpuLabel }}</span>
      <svg viewBox="0 0 48 12" class="w-12 sm:w-14 h-3 shrink-0 opacity-90" preserveAspectRatio="none" aria-hidden="true">
        <polyline v-if="cpuPoints" fill="none" class="stroke-violet-500" stroke-width="1.2" stroke-linejoin="round" stroke-linecap="round" vector-effect="non-scaling-stroke" :points="cpuPoints" />
      </svg>
    </div>
    <div class="flex items-center gap-1.5 min-w-0">
      <span class="text-xs font-medium text-muted-foreground/40 uppercase tracking-wide shrink-0">Mem</span>
      <span class="text-xs font-mono font-semibold tabular-nums text-foreground shrink-0">{{ memPctLabel }}</span>
      <svg viewBox="0 0 48 12" class="w-12 sm:w-14 h-3 shrink-0 opacity-90" preserveAspectRatio="none" aria-hidden="true">
        <polyline v-if="memPoints" fill="none" class="stroke-sky-500" stroke-width="1.2" stroke-linejoin="round" stroke-linecap="round" vector-effect="non-scaling-stroke" :points="memPoints" />
      </svg>
    </div>
  </div>
</template>
