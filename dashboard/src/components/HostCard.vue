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

/** Area variant of the trend line: same points, closed to the chart floor. */
const cpuArea = computed(() => {
  const points = cpuTrend.value
  if (!points) return ''
  return `M${points.replace(/ /g, ' L')} L100,32 L0,32 Z`
})

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
  <UiSection title="Host">
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

    <div v-if="!current" class="grid gap-6 sm:grid-cols-2 sm:gap-0">
      <div class="animate-pulse sm:pr-6">
        <div class="flex items-center justify-between">
          <div class="h-3 w-14 rounded bg-muted/40"></div>
          <div class="h-6 w-16 rounded bg-muted/40"></div>
        </div>
        <div class="mt-4 h-20 w-full rounded bg-muted/40"></div>
      </div>
      <div class="animate-pulse border-t border-border/40 pt-6 sm:border-t-0 sm:border-l sm:border-border/40 sm:pl-6 sm:pt-0">
        <div class="flex items-center justify-between">
          <div class="h-3 w-16 rounded bg-muted/40"></div>
          <div class="h-6 w-16 rounded bg-muted/40"></div>
        </div>
        <div class="mt-4 h-2.5 w-full rounded-full bg-muted/40"></div>
        <div class="mt-3 h-3 w-40 rounded bg-muted/40"></div>
      </div>
    </div>

    <div v-else class="grid gap-6 sm:grid-cols-2 sm:gap-0">
      <!-- CPU -->
      <div class="min-w-0 sm:pr-6">
        <div class="flex items-center justify-between gap-2">
          <div class="flex items-center gap-2">
            <Cpu class="h-4 w-4 text-muted-foreground/50" />
            <span class="type-eyebrow">CPU</span>
          </div>
          <span class="text-2xl font-semibold tabular-nums">{{ cpuLabel }}</span>
        </div>
        <svg
          viewBox="0 0 100 32"
          preserveAspectRatio="none"
          class="mt-4 h-20 w-full"
          aria-hidden="true"
        >
          <defs>
            <linearGradient id="cpu-area" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="#8b5cf6" stop-opacity="0.28" />
              <stop offset="100%" stop-color="#8b5cf6" stop-opacity="0" />
            </linearGradient>
          </defs>
          <path v-if="cpuArea" :d="cpuArea" fill="url(#cpu-area)" />
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

      <!-- MEMORY: current level is the information — bar + usage, no trend -->
      <div class="min-w-0 border-t border-border/40 pt-6 sm:border-t-0 sm:border-l sm:border-border/40 sm:pl-6 sm:pt-0">
        <div class="flex items-center justify-between gap-2">
          <div class="flex items-center gap-2">
            <MemoryStick class="h-4 w-4 text-muted-foreground/50" />
            <span class="type-eyebrow">MEMORY</span>
          </div>
          <span class="text-2xl font-semibold tabular-nums">{{ memLabel }}</span>
        </div>
        <div class="mt-4 h-2.5 w-full overflow-hidden rounded-full bg-muted">
          <div
            class="h-full rounded-full transition-[width] duration-500"
            :class="memHigh ? 'bg-warning' : 'bg-primary'"
            :style="{ width: `${Math.min(memPct, 100)}%` }"
          />
        </div>
        <p class="mt-3 text-xs text-muted-foreground">
          <span class="font-medium text-foreground">{{ memUsedLabel }}</span>
          <span class="text-muted-foreground/50"> of </span>
          {{ memTotalLabel }}
        </p>
      </div>
    </div>
  </UiSection>
</template>
