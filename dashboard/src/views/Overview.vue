<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { Activity, ArrowRight, CircleAlert, CirclePlay, CircleStop, Server } from 'lucide-vue-next'
import { format } from 'date-fns'
import apiClient from '@/api/client'
import { API_PATHS } from '@/api/paths'
import { useProgramStore } from '@/stores/program'
import SystemMetricsPanel from '@/components/SystemMetricsPanel.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiEmptyState from '@/components/ui/UiEmptyState.vue'
import UiSection from '@/components/ui/UiSection.vue'
import UiStatusChip from '@/components/ui/UiStatusChip.vue'
import type { Program, ProgramEventRecord } from '@/types'

const store = useProgramStore()
const router = useRouter()

const stats = computed(() => ({
  total: store.programs.length,
  running: store.programs.filter(
    (p) => ['Healthy', 'Starting'].includes(p.status) || (p.status === 'Running' && !p.health_error),
  ).length,
  stopped: store.programs.filter((p) => ['Stopped', 'Stopping'].includes(p.status)).length,
  problem: store.programs.filter(
    (p) => ['Fatal', 'Backoff'].includes(p.status) || !!p.health_error,
  ).length,
}))

const cards = computed(() => [
  { label: 'Programs', value: stats.value.total, filter: 'ALL', icon: Activity },
  { label: 'Running', value: stats.value.running, filter: 'Running', icon: CirclePlay },
  { label: 'Stopped', value: stats.value.stopped, filter: 'Stopped', icon: CircleStop },
  { label: 'Issues', value: stats.value.problem, filter: 'Fatal', icon: CircleAlert },
])

const attention = computed(() =>
  store.programs
    .filter((p) => ['Fatal', 'Backoff'].includes(p.status) || !!p.health_error || !!p.last_error)
    .slice(0, 6),
)

const events = ref<ProgramEventRecord[]>([])
const eventsLoading = ref(true)
const eventsError = ref('')

function openFilter(filter: string) {
  void router.push({ path: '/programs', query: { status: filter } })
}

function openProgram(program: Program) {
  void router.push(`/programs/${program.id}`)
}

function eventTone(event: string, exitCode?: number | null): 'neutral' | 'success' | 'warning' | 'danger' {
  const name = event.toLowerCase()
  if (exitCode != null && exitCode !== 0) return 'danger'
  if (name.includes('crash') || name.includes('fatal') || name.includes('fail') || name.includes('error')) {
    return 'danger'
  }
  if (name.includes('restart') || name.includes('backoff') || name.includes('kill') || name.includes('warn')) {
    return 'warning'
  }
  if (name.includes('start') || name.includes('healthy') || name.includes('stop') || name.includes('exit')) {
    return 'success'
  }
  return 'neutral'
}

function formatTime(ts: number): string {
  return ts ? format(new Date(ts * 1000), 'MM-dd HH:mm:ss') : '—'
}

onMounted(async () => {
  void store.fetchPrograms()
  try {
    const response = await apiClient.get<ProgramEventRecord[]>(API_PATHS.EVENT_HISTORY.QUERY, {
      params: { limit: 8, order: 'desc' },
    })
    events.value = response.data ?? []
  } catch (e: any) {
    eventsError.value = e.response?.data?.message || 'Could not load recent events.'
  } finally {
    eventsLoading.value = false
  }
})
</script>

<template>
  <div class="space-y-6">
    <!-- Status counters -->
    <div class="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
      <button
        v-for="card in cards"
        :key="card.label"
        type="button"
        class="rounded-2xl bg-card p-5 text-left ring-1 ring-border/60 shadow-[0_1px_2px_hsl(220_20%_20%/0.04),0_2px_8px_hsl(220_20%_20%/0.03)] transition-colors hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
        @click="openFilter(card.filter)"
      >
        <div class="flex items-center justify-between">
          <span class="type-eyebrow">{{ card.label }}</span>
          <component :is="card.icon" class="h-4 w-4 text-muted-foreground/40" />
        </div>
        <p class="mt-2 text-2xl font-semibold tabular-nums">{{ card.value }}</p>
      </button>
    </div>

    <div class="grid gap-6 xl:grid-cols-2">
      <!-- Needs attention -->
      <UiSection title="Needs attention" description="Failing, restarting, or unhealthy programs.">
        <div v-if="store.isLoading && store.programs.length === 0" class="space-y-2">
          <div v-for="row in 3" :key="row" class="h-10 animate-pulse rounded-xl bg-muted/40"></div>
        </div>
        <div v-else-if="attention.length" class="space-y-1">
          <button
            v-for="program in attention"
            :key="program.id"
            type="button"
            class="flex w-full items-center justify-between gap-3 rounded-xl px-2 py-2 text-left transition-colors hover:bg-muted/40"
            @click="openProgram(program)"
          >
            <div class="min-w-0">
              <p class="truncate text-sm font-medium">{{ program.name }}</p>
              <p class="truncate text-xs text-muted-foreground">
                {{ program.last_error || program.health_error }}
              </p>
            </div>
            <UiStatusChip :tone="['Fatal', 'Backoff'].includes(program.status) ? 'danger' : 'warning'">
              {{ program.status }}
            </UiStatusChip>
          </button>
        </div>
        <UiEmptyState v-else title="Everything is healthy" hint="No failing or unhealthy programs." />
      </UiSection>

      <!-- Recent events -->
      <UiSection title="Recent events" description="The newest lifecycle and health events.">
        <template #actions>
          <UiButton variant="ghost" size="sm" @click="router.push('/events')">
            All events
            <ArrowRight class="h-3.5 w-3.5" />
          </UiButton>
        </template>
        <div v-if="eventsLoading" class="space-y-2">
          <div v-for="row in 4" :key="row" class="h-8 animate-pulse rounded-lg bg-muted/40"></div>
        </div>
        <p v-else-if="eventsError" class="text-sm text-destructive">{{ eventsError }}</p>
        <div v-else-if="events.length" class="space-y-1">
          <div
            v-for="(event, index) in events"
            :key="`${event.ts}-${event.event}-${index}`"
            class="flex items-center gap-3 rounded-xl px-2 py-2"
          >
            <span class="w-24 shrink-0 font-mono text-[11px] tabular-nums text-muted-foreground">
              {{ formatTime(event.ts) }}
            </span>
            <UiStatusChip :tone="eventTone(event.event, event.exit_code)">
              {{ event.event }}
            </UiStatusChip>
            <span class="min-w-0 flex-1 truncate text-xs text-muted-foreground" :title="event.msg">
              {{ event.program_name || 'daemon' }} — {{ event.msg }}
            </span>
          </div>
        </div>
        <UiEmptyState v-else title="No events yet" hint="Events appear once programs start running." />
      </UiSection>
    </div>

    <!-- Host resources -->
    <UiSection title="Host" description="Resources on the machine running superd.">
      <div class="flex items-center gap-3 py-1">
        <Server class="h-4 w-4 text-muted-foreground/40" />
        <SystemMetricsPanel />
      </div>
    </UiSection>
  </div>
</template>
