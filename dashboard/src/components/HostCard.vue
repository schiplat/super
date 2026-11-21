<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { Cpu, MemoryStick } from 'lucide-vue-next'
import { API_PATHS } from '@/api/paths'
import apiClient from '@/api/client'
import { getSuperConfig } from '@/lib/superConfig'
import { useCapabilitiesStore } from '@/stores/capabilities'
import { useProgramStore } from '@/stores/program'
import UiSection from '@/components/ui/UiSection.vue'

interface SystemStats {
  cpu_percent: number
  memory_used_bytes: number
  memory_total_bytes: number
  timestamp: number
}

const store = useProgramStore()
const caps = useCapabilitiesStore()
const current = ref<SystemStats | null>(null)
const cpuHistory = ref<number[]>([])
const memHistory = ref<number[]>([])
const MAX_POINTS = 40
const CHART_MAX = 100

let timer: ReturnType<typeof setInterval> | null = null

async function fetchStats() {
  // On a gated instance a session-less probe would feed the per-IP 401
  // throttle (20 failures/min ≈ lockout at a 3s interval) — skip until the
  // user signs in. No-gate instances serve stats openly, token or not.
  if (caps.auth && !localStorage.getItem('super_token')) return
  try {

    const res = await apiClient.get(API_PATHS.SYSTEM.STATS)
    const data: SystemStats = res.data
    current.value = data
    cpuHistory.value = [...cpuHistory.value, data.cpu_percent].slice(-MAX_POINTS)
    const memPct = data.memory_total_bytes > 0 ? (data.memory_used_bytes / data.memory_total_bytes) * 100 : 0
    memHistory.value = [...memHistory.value, memPct].slice(-MAX_POINTS)
  } catch { /* best-effort */ }
}

const cpuLabel = computed(() => (current.value ? `${current.value.cpu_percent.toFixed(1)}%` : '—'))
const memPct = computed(() => {
  if (!current.value || current.value.memory_total_bytes === 0) return 0
  return (current.value.memory_used_bytes / current.value.memory_total_bytes) * 100
})
const memLabel = computed(() => (current.value ? `${memPct.value.toFixed(0)}%` : '—'))
const memUsedLabel = computed(() =>
  current.value ? `${(current.value.memory_used_bytes / 1024 ** 3).toFixed(1)} GB` : '—',
)
const memTotalLabel = computed(() =>
  current.value ? `${(current.value.memory_total_bytes / 1024 ** 3).toFixed(1)} GB` : '—',
)
const memHigh = computed(() => memPct.value >= 80)

/** Wide trend line: 0–100 across a 100×32 box, stretched to the card. */
function trendPoints(values: number[]): string {
  if (values.length < 2) return ''
  const step = 100 / (values.length - 1)
  return values
    .map((v, i) => {
      const x = i * step
      const y = 30 - (Math.min(Math.max(v, 0), CHART_MAX) / CHART_MAX) * 28
      return `${x.toFixed(1)},${y.toFixed(1)}`
    })
    .join(' ')
}

const cpuTrend = computed(() => trendPoints(cpuHistory.value))
const memTrend = computed(() => trendPoints(memHistory.value))

const version = getSuperConfig().version

onMounted(() => {
  void fetchStats()
  timer = setInterval(fetchStats, 3000)
})
onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <UiSection title="Host" class="h-full">
    <template #actions>
      <div class="flex items-center gap-2 text-[11px] text-muted-foreground">
        <span
          class="inline-flex items-center gap-1.5"
          :title="store.isConnected ? 'Connected' : 'Disconnected'"
        >
          <span
            class="h-1.5 w-1.5 rounded-full"
            :class="store.isConnected ? 'bg-success' : 'bg-destructive'"
          />
          {{ store.isConnected ? 'LIVE' : 'OFFLINE' }}
        </span>
        <span class="text-muted-foreground/50">·</span>
        <span class="tabular-nums">superd v{{ version }}</span>
      </div>
    </template>

    <div v-if="!current" class="grid grid-cols-2 gap-6">
      <div v-for="block in ['CPU', 'MEMORY']" :key="block" class="animate-pulse">
        <div class="h-3 w-14 rounded bg-muted/40"></div>
        <div class="mt-3 h-9 w-24 rounded bg-muted/40"></div>
        <div class="mt-3 h-10 w-full rounded bg-muted/40"></div>
      </div>
    </div>

    <div v-else class="grid grid-cols-2 gap-6">
      <!-- CPU -->
      <div class="min-w-0">
        <div class="flex items-center gap-2">
          <Cpu class="h-4 w-4 text-muted-foreground/50" />
          <span class="type-eyebrow">CPU</span>
        </div>
        <p class="mt-2 text-3xl font-semibold tabular-nums">{{ cpuLabel }}</p>
        <svg
          viewBox="0 0 100 32"
          preserveAspectRatio="none"
          class="mt-3 h-12 w-full opacity-90"
          aria-hidden="true"
        >
          <polyline
            v-if="cpuTrend"
            fill="none"
            class="stroke-violet-500"
            stroke-width="1.5"
            stroke-linejoin="round"
            stroke-linecap="round"
            vector-effect="non-scaling-stroke"
            :points="cpuTrend"
          />
        </svg>
      </div>

      <!-- MEMORY -->
      <div class="min-w-0">
        <div class="flex items-center gap-2">
          <MemoryStick class="h-4 w-4 text-muted-foreground/50" />
          <span class="type-eyebrow">MEMORY</span>
        </div>
        <p class="mt-2 text-3xl font-semibold tabular-nums">{{ memLabel }}</p>
        <div class="mt-3 h-2 w-full overflow-hidden rounded-full bg-muted">
          <div
            class="h-full rounded-full transition-[width] duration-500"
            :class="memHigh ? 'bg-warning' : 'bg-primary'"
            :style="{ width: `${Math.min(memPct, 100)}%` }"
          />
        </div>
        <p class="mt-2 text-xs text-muted-foreground">
          {{ memUsedLabel }} <span class="text-muted-foreground/50">of</span> {{ memTotalLabel }}
        </p>
        <svg
          viewBox="0 0 100 32"
          preserveAspectRatio="none"
          class="mt-3 h-12 w-full opacity-90"
          aria-hidden="true"
        >
          <polyline
            v-if="memTrend"
            fill="none"
            class="stroke-sky-500"
            stroke-width="1.5"
            stroke-linejoin="round"
            stroke-linecap="round"
            vector-effect="non-scaling-stroke"
            :points="memTrend"
          />
        </svg>
      </div>
    </div>
  </UiSection>
</template>
