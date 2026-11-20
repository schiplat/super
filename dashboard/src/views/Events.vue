<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { format } from 'date-fns'
import { RefreshCw, Search } from 'lucide-vue-next'
import apiClient from '@/api/client'
import { API_PATHS } from '@/api/paths'
import { useProgramStore } from '@/stores/program'
import UiButton from '@/components/ui/UiButton.vue'
import UiDataTable from '@/components/ui/UiDataTable.vue'
import UiStatusChip from '@/components/ui/UiStatusChip.vue'
import type { EventStats, ProgramEventRecord } from '@/types'
import type { DataColumn } from '@/types/ui'

const PAGE_SIZE = 50
const store = useProgramStore()

const programId = ref('')
const eventType = ref('')
const range = ref<'1h' | '24h' | '7d' | '30d' | 'all'>('24h')
const query = ref('')
const rows = ref<ProgramEventRecord[]>([])
const types = ref<string[]>([])
const offset = ref(0)
const loading = ref(true)
const error = ref('')

const page = computed(() => Math.floor(offset.value / PAGE_SIZE) + 1)
const columns: DataColumn[] = [
  { key: 'time', label: 'Time' },
  { key: 'program', label: 'Program' },
  { key: 'event', label: 'Event' },
  { key: 'result', label: 'Exit / signal' },
  { key: 'duration', label: 'Duration' },
  { key: 'msg', label: 'Message' },
]

const RANGE_SECONDS: Record<string, number | null> = {
  '1h': 60 * 60,
  '24h': 24 * 60 * 60,
  '7d': 7 * 24 * 60 * 60,
  '30d': 30 * 24 * 60 * 60,
  all: null,
}

function formatTime(ts: number): string {
  return ts ? format(new Date(ts * 1000), 'MM-dd HH:mm:ss') : '—'
}

function result(row: ProgramEventRecord): string {
  if (row.exit_code != null) return `exit ${row.exit_code}`
  if (row.signal != null) return `sig ${row.signal}`
  return '—'
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

async function load() {
  loading.value = true
  error.value = ''
  try {
    const params: Record<string, unknown> = {
      limit: PAGE_SIZE,
      offset: offset.value,
      order: 'desc',
      sort_by: 'time',
    }
    if (programId.value) params.program_id = programId.value
    if (eventType.value) params.event_type = eventType.value
    if (query.value.trim()) params.q = query.value.trim()
    const seconds = RANGE_SECONDS[range.value]
    if (seconds) params.from = Math.floor(Date.now() / 1000) - seconds
    const response = await apiClient.get<ProgramEventRecord[]>(API_PATHS.EVENT_HISTORY.QUERY, { params })
    rows.value = response.data ?? []
  } catch (e: any) {
    error.value = e.response?.data?.message || 'Could not load events.'
    rows.value = []
  } finally {
    loading.value = false
  }
}

let searchTimer: ReturnType<typeof setTimeout> | null = null
watch(query, () => {
  if (searchTimer) clearTimeout(searchTimer)
  searchTimer = setTimeout(() => {
    offset.value = 0
    void load()
  }, 300)
})
watch([programId, eventType, range], () => {
  offset.value = 0
  void load()
})

function previousPage() {
  if (offset.value >= PAGE_SIZE) {
    offset.value -= PAGE_SIZE
    void load()
  }
}

function nextPage() {
  if (rows.value.length === PAGE_SIZE) {
    offset.value += PAGE_SIZE
    void load()
  }
}

onMounted(async () => {
  void store.fetchPrograms()
  try {
    const stats = await apiClient.get<EventStats>(API_PATHS.EVENT_HISTORY.STATS)
    types.value = (stats.data.by_type ?? []).map((entry) => entry.event)
  } catch {
    // The type filter is a convenience; the page works without it.
  }
  await load()
})
</script>

<template>
  <div class="space-y-6">
    <UiDataTable
      title="Events"
      description="Lifecycle, health, and scheduler history across every program."
      :columns="columns"
      :rows-count="rows.length"
      :loading="loading"
      empty="No events match these filters"
      empty-hint="Widen the time range or clear the filters."
    >
      <template #toolbar>
        <div class="flex w-full flex-wrap items-center gap-3">
          <select v-model="programId" class="field-control field-control-sm w-52" aria-label="Program">
            <option value="">All programs</option>
            <option v-for="program in store.programs" :key="program.id" :value="program.id">
              {{ program.name }}
            </option>
          </select>

          <select v-model="eventType" class="field-control field-control-sm w-44" aria-label="Event type">
            <option value="">All events</option>
            <option v-for="type in types" :key="type" :value="type">{{ type }}</option>
          </select>

          <select v-model="range" class="field-control field-control-sm w-36" aria-label="Time range">
            <option value="1h">Last hour</option>
            <option value="24h">Last 24 hours</option>
            <option value="7d">Last 7 days</option>
            <option value="30d">Last 30 days</option>
            <option value="all">All time</option>
          </select>

          <div class="relative w-full sm:max-w-xs">
            <Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground/40" />
            <input
              v-model="query"
              type="text"
              placeholder="Search messages…"
              class="field-control field-control-sm pl-9"
            />
          </div>

          <UiButton variant="outline" size="sm" :disabled="loading" @click="load">
            <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': loading }" />
            <span class="hidden sm:inline">Refresh</span>
          </UiButton>

          <p v-if="error" class="text-xs text-destructive">{{ error }}</p>
        </div>
      </template>

      <tr
        v-for="(row, index) in rows"
        :key="`${row.ts}-${row.event}-${index}`"
        class="border-b border-border/40 last:border-0 hover:bg-muted/40"
      >
        <td class="whitespace-nowrap font-mono text-xs tabular-nums text-muted-foreground">
          {{ formatTime(row.ts) }}
        </td>
        <td class="text-xs">{{ row.program_name || '—' }}</td>
        <td>
          <UiStatusChip :tone="eventTone(row.event, row.exit_code)">{{ row.event }}</UiStatusChip>
        </td>
        <td class="font-mono text-xs text-muted-foreground">{{ result(row) }}</td>
        <td class="font-mono text-xs text-muted-foreground">
          {{ row.duration_secs != null ? `${row.duration_secs}s` : '—' }}
        </td>
        <td class="max-w-[32rem] truncate text-xs text-muted-foreground" :title="row.msg">
          {{ row.msg }}
        </td>
      </tr>

      <template #footer>
        <div class="flex items-center justify-between gap-3 text-[11px] text-muted-foreground">
          <span class="tabular-nums">Page {{ page }}</span>
          <div class="flex items-center gap-2">
            <UiButton variant="outline" size="sm" :disabled="offset === 0 || loading" @click="previousPage">
              Previous
            </UiButton>
            <UiButton
              variant="outline"
              size="sm"
              :disabled="rows.length < PAGE_SIZE || loading"
              @click="nextPage"
            >
              Next
            </UiButton>
          </div>
        </div>
      </template>
    </UiDataTable>
  </div>
</template>
