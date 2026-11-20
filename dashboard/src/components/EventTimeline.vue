<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  ChevronRight,
  Circle,
  CircleAlert,
  CircleCheck,
  RefreshCw,
  Search,
  TriangleAlert,
} from 'lucide-vue-next'
import apiClient from '@/api/client'
import { API_PATHS } from '@/api/paths'
import { useProgramStore } from '@/stores/program'
import { eventDayLabel, eventResult, eventTime, eventTimestamp, eventTone, type EventTone } from '@/lib/events'
import UiButton from '@/components/ui/UiButton.vue'
import UiEmptyState from '@/components/ui/UiEmptyState.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiStatusChip from '@/components/ui/UiStatusChip.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'
import type { EventStats, ProgramEventRecord } from '@/types'

type EventRange = '1h' | '24h' | '7d' | '30d' | 'all'

const props = withDefaults(
  defineProps<{
    /** Fixed program (detail tab); hides the program filter. */
    programId?: string
    /** Compact, card-less mode for the program detail tab. */
    compact?: boolean
    initialRange?: EventRange
    pageSize?: number
  }>(),
  { initialRange: '24h', pageSize: 50 },
)

const store = useProgramStore()
const route = useRoute()
const router = useRouter()

const RANGE_SECONDS: Record<EventRange, number | null> = {
  '1h': 60 * 60,
  '24h': 24 * 60 * 60,
  '7d': 7 * 24 * 60 * 60,
  '30d': 30 * 24 * 60 * 60,
  all: null,
}

const bulkCard =
  'rounded-2xl bg-card p-5 ring-1 ring-border/60 shadow-[0_1px_2px_hsl(220_20%_20%/0.04),0_2px_8px_hsl(220_20%_20%/0.03)]'

const initialQuery = route.query
const initialPage =
  typeof initialQuery.page === 'string' ? Math.max(1, Number(initialQuery.page) || 1) : 1
const initialRangeRaw = typeof initialQuery.range === 'string' ? initialQuery.range : ''
const initialRange = (Object.keys(RANGE_SECONDS) as EventRange[]).includes(
  initialRangeRaw as EventRange,
)
  ? (initialRangeRaw as EventRange)
  : props.initialRange

const selectedProgram = ref(
  props.programId ??
    (typeof initialQuery.program_id === 'string' ? initialQuery.program_id : ''),
)
const eventType = ref(typeof initialQuery.type === 'string' ? initialQuery.type : '')
const range = ref<EventRange>(initialRange)
const query = ref(typeof initialQuery.q === 'string' ? initialQuery.q : '')
const offset = ref((initialPage - 1) * props.pageSize)

const rows = ref<ProgramEventRecord[]>([])
const summary = ref<EventStats | null>(null)
const types = ref<string[]>([])
const loading = ref(true)
const error = ref('')
const live = ref(false)
const newKeys = ref<Set<string>>(new Set())
const lastLoaded = ref<number>(0)
const expanded = ref<string | null>(null)
const copiedKey = ref<string | null>(null)

const page = computed(() => Math.floor(offset.value / props.pageSize) + 1)
const pageStart = computed(() => (rows.value.length ? offset.value + 1 : 0))
const pageEnd = computed(() => offset.value + rows.value.length)

const toneIcon = {
  danger: CircleAlert,
  warning: TriangleAlert,
  success: CircleCheck,
  neutral: Circle,
} as const

const toneText: Record<EventTone, string> = {
  danger: 'text-destructive',
  warning: 'text-warning',
  success: 'text-success',
  neutral: 'text-muted-foreground/40',
}

const toneDot: Record<EventTone, string> = {
  danger: 'bg-destructive',
  warning: 'bg-warning',
  success: 'bg-success',
  neutral: 'bg-muted-foreground/40',
}

function rowKey(row: ProgramEventRecord): string {
  return `${row.ts_ms || row.ts * 1000}-${row.event}-${row.program_id ?? 'daemon'}-${row.msg}`
}

const grouped = computed(() => {
  const groups: { label: string; rows: { row: ProgramEventRecord; key: string }[] }[] = []
  rows.value.forEach((row, index) => {
    const label = eventDayLabel(row.ts)
    let group = groups[groups.length - 1]
    if (!group || group.label !== label) {
      group = { label, rows: [] }
      groups.push(group)
    }
    group.rows.push({ row, key: `${rowKey(row)}-${index}` })
  })
  return groups
})

const summaryCards = computed(() => {
  const stats = summary.value
  if (!stats) return []
  const cards: { label: string; value: number; tone: EventTone }[] = [
    { label: 'Events', value: stats.total, tone: 'neutral' },
  ]
  for (const entry of stats.by_type.slice(0, 3)) {
    cards.push({ label: entry.event, value: entry.count, tone: eventTone(entry.event) })
  }
  return cards
})

function rangeFrom(): number | undefined {
  const seconds = RANGE_SECONDS[range.value]
  return seconds ? Math.floor(Date.now() / 1000) - seconds : undefined
}

function listParams(): Record<string, unknown> {
  const params: Record<string, unknown> = {
    limit: props.pageSize,
    offset: offset.value,
    order: 'desc',
    sort_by: 'time',
  }
  if (selectedProgram.value) params.program_id = selectedProgram.value
  if (eventType.value) params.event_type = eventType.value
  if (query.value.trim()) params.q = query.value.trim()
  if (!props.compact) {
    const from = rangeFrom()
    if (from != null) params.from = from
  }
  return params
}

function statsParams(): Record<string, unknown> {
  const params: Record<string, unknown> = {}
  if (selectedProgram.value) params.program_id = selectedProgram.value
  if (!props.compact) {
    const from = rangeFrom()
    if (from != null) params.from = from
  }
  return params
}

function mergeTypes(stats: EventStats | null) {
  const merged = new Set(types.value)
  for (const entry of stats?.by_type ?? []) merged.add(entry.event)
  types.value = [...merged].sort()
}

let flashNext = false

async function load() {
  // Compact mode without a program (detail opened before a row is selected):
  // nothing to show, and a global fetch would be misleading.
  if (props.compact && !selectedProgram.value) {
    rows.value = []
    loading.value = false
    return
  }
  loading.value = true
  error.value = ''
  try {
    const [list, stats] = await Promise.all([
      apiClient.get<ProgramEventRecord[]>(API_PATHS.EVENT_HISTORY.QUERY, {
        params: listParams(),
      }),
      apiClient.get<EventStats>(API_PATHS.EVENT_HISTORY.STATS, {
        params: statsParams(),
      }),
    ])
    const previous = new Set(rows.value.map(rowKey))
    rows.value = list.data ?? []
    summary.value = stats.data
    mergeTypes(stats.data)
    lastLoaded.value = Date.now()
    if (flashNext) {
      flashNext = false
      newKeys.value = new Set(
        rows.value.map(rowKey).filter((key) => !previous.has(key)),
      )
      setTimeout(() => (newKeys.value = new Set()), 2000)
    }
  } catch (e: any) {
    error.value = e.response?.data?.message || 'Could not load events.'
    rows.value = []
  } finally {
    loading.value = false
  }
}

function syncUrl() {
  if (props.compact) return
  const next: Record<string, string> = {}
  if (selectedProgram.value) next.program_id = selectedProgram.value
  if (eventType.value) next.type = eventType.value
  if (range.value !== props.initialRange) next.range = range.value
  if (query.value.trim()) next.q = query.value.trim()
  if (page.value > 1) next.page = String(page.value)
  void router.replace({ path: '/events', query: next })
}

let searchTimer: ReturnType<typeof setTimeout> | null = null

function onFiltersChanged() {
  offset.value = 0
  syncUrl()
  void load()
}

watch([selectedProgram, eventType, range], onFiltersChanged)
watch(query, () => {
  if (searchTimer) clearTimeout(searchTimer)
  searchTimer = setTimeout(onFiltersChanged, 300)
})

let liveTimer: ReturnType<typeof setInterval> | null = null

function stopLive() {
  if (liveTimer) clearInterval(liveTimer)
  liveTimer = null
}

// The detail drawer stays mounted while switching programs, so the scoped
// timeline must follow the prop instead of only reading it at setup.
watch(
  () => props.programId,
  (id) => {
    if (!props.compact) return
    selectedProgram.value = id ?? ''
    offset.value = 0
    void load()
  },
)

watch(live, (enabled) => {
  stopLive()
  if (!enabled) return
  offset.value = 0
  void load()
  liveTimer = setInterval(() => {
    if (loading.value) return
    offset.value = 0
    flashNext = true
    void load()
  }, 5000)
})

function goOlder() {
  if (rows.value.length < props.pageSize) return
  offset.value += props.pageSize
  syncUrl()
  void load()
}

function goNewer() {
  if (offset.value === 0) return
  offset.value = Math.max(0, offset.value - props.pageSize)
  syncUrl()
  void load()
}

function toggleRow(key: string) {
  expanded.value = expanded.value === key ? null : key
}

function openProgram(id?: string | null) {
  if (!id) return
  void router.push(`/programs/${id}`)
}

function openInEvents() {
  const target: Record<string, string> = { range: '7d' }
  if (props.programId) target.program_id = props.programId
  void router.push({ path: '/events', query: target })
}

async function copyEvent(row: ProgramEventRecord, key: string) {
  try {
    await navigator.clipboard.writeText(JSON.stringify(row, null, 2))
    copiedKey.value = key
    setTimeout(() => (copiedKey.value = null), 1500)
  } catch {
    // Clipboard is best-effort; the row detail stays visible either way.
  }
}

function meta(row: ProgramEventRecord): string {
  const parts: string[] = [eventResult(row)]
  if (row.duration_secs != null) parts.push(`${row.duration_secs}s`)
  if (row.retry_count != null) parts.push(`retry ${row.retry_count}`)
  return parts.join(' · ')
}

onMounted(() => {
  if (!props.compact) void store.fetchPrograms()
  void load()
})

onUnmounted(() => {
  stopLive()
  if (searchTimer) clearTimeout(searchTimer)
})
</script>

<template>
  <div :class="compact ? 'flex min-h-0 flex-1 flex-col' : 'space-y-6'">
    <!-- Summary: current window, same numbers the list is filtered by -->
    <div v-if="!compact" class="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
      <div v-for="card in summaryCards" :key="card.label" :class="bulkCard">
        <div class="flex items-center justify-between gap-2">
          <span class="type-eyebrow truncate">{{ card.label }}</span>
          <span class="h-1.5 w-1.5 shrink-0 rounded-full" :class="toneDot[card.tone]"></span>
        </div>
        <p class="mt-2 text-2xl font-semibold tabular-nums">{{ card.value }}</p>
      </div>
      <div v-if="!summaryCards.length" :class="bulkCard" class="animate-pulse">
        <div class="h-3 w-16 rounded bg-muted/40"></div>
        <div class="mt-3 h-7 w-10 rounded bg-muted/40"></div>
      </div>
    </div>

    <section
      :class="
        compact
          ? 'flex min-h-0 flex-1 flex-col'
          : 'overflow-hidden rounded-2xl bg-card ring-1 ring-border/60 shadow-[0_1px_2px_hsl(220_20%_20%/0.04),0_2px_8px_hsl(220_20%_20%/0.03)]'
      "
    >
      <header
        v-if="!compact"
        class="flex flex-wrap items-start justify-between gap-3 px-6 pt-5"
      >
        <div class="min-w-0">
          <h2 class="text-sm font-semibold tracking-tight">Event timeline</h2>
          <p class="mt-1 text-sm text-muted-foreground">
            Lifecycle, health, and scheduler history across every program.
          </p>
        </div>
      </header>

      <!-- Filters: one 20px gutter, 32px controls -->
      <div
        class="flex flex-wrap items-center gap-3 border-b border-border/40 px-5 py-3"
        :class="compact ? '' : 'mt-4'"
      >
        <select
          v-if="!compact && !programId"
          v-model="selectedProgram"
          class="field-control field-control-sm w-52"
          aria-label="Program"
        >
          <option value="">All programs</option>
          <option v-for="program in store.programs" :key="program.id" :value="program.id">
            {{ program.name }}
          </option>
        </select>

        <select
          v-model="eventType"
          class="field-control field-control-sm w-44"
          aria-label="Event type"
        >
          <option value="">All events</option>
          <option v-for="type in types" :key="type" :value="type">{{ type }}</option>
        </select>

        <select
          v-if="!compact"
          v-model="range"
          class="field-control field-control-sm w-36"
          aria-label="Time range"
        >
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

        <div class="ml-auto flex items-center gap-3">
          <label v-if="!compact" class="flex items-center gap-2 text-xs text-muted-foreground">
            <UiSwitch v-model="live" aria-label="Live updates" />
            Live
          </label>
          <UiButton variant="outline" size="sm" :disabled="loading" @click="load">
            <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': loading }" />
            <span class="hidden sm:inline">Refresh</span>
          </UiButton>
        </div>
      </div>

      <div v-if="error" class="px-5 pt-3">
        <UiNotice tone="danger">
          <div class="flex items-center gap-3">
            <span class="flex-1">{{ error }}</span>
            <UiButton variant="ghost" size="sm" @click="load">Retry</UiButton>
          </div>
        </UiNotice>
      </div>

      <div :class="compact ? 'min-h-0 flex-1 overflow-y-auto' : ''">
        <div v-if="loading && !rows.length" class="space-y-2 p-5">
          <div v-for="row in 5" :key="row" class="h-9 animate-pulse rounded-lg bg-muted/40"></div>
        </div>

        <UiEmptyState
          v-else-if="!rows.length && !error"
          title="No events in this range"
          hint="Widen the time range or clear the filters."
        >
          <UiButton v-if="!compact" variant="secondary" size="sm" @click="range = '7d'">
            Widen to 7 days
          </UiButton>
        </UiEmptyState>

        <div v-else class="divide-y divide-border/40">
          <template v-for="group in grouped" :key="group.label">
            <div
              class="sticky top-0 z-10 bg-card px-5 py-2 text-[11px] font-medium uppercase tracking-[0.04em] text-muted-foreground/80"
            >
              {{ group.label }}
            </div>

            <div v-for="entry in group.rows" :key="entry.key">
              <button
                type="button"
                class="flex w-full items-center gap-3 px-5 py-3 text-left transition-colors hover:bg-muted/40 focus-visible:bg-muted/40 focus-visible:outline-none"
                :class="{ 'bg-muted/40': newKeys.has(rowKey(entry.row)) }"
                @click="toggleRow(entry.key)"
              >
                <span class="w-[4.5rem] shrink-0 font-mono text-xs tabular-nums text-muted-foreground">
                  {{ eventTime(entry.row.ts) }}
                </span>
                <component
                  :is="toneIcon[eventTone(entry.row.event, entry.row.exit_code)]"
                  class="h-4 w-4 shrink-0"
                  :class="toneText[eventTone(entry.row.event, entry.row.exit_code)]"
                  aria-hidden="true"
                />
                <span class="w-[8.5rem] shrink-0 truncate">
                  <UiStatusChip :tone="eventTone(entry.row.event, entry.row.exit_code)">
                    {{ entry.row.event }}
                  </UiStatusChip>
                </span>
                <span
                  v-if="!compact"
                  class="hidden w-[9rem] shrink-0 truncate text-xs text-muted-foreground md:block"
                >
                  {{ entry.row.program_name || 'daemon' }}
                </span>
                <span class="min-w-0 flex-1 truncate text-sm">{{ entry.row.msg }}</span>
                <span
                  class="hidden shrink-0 font-mono text-[11px] tabular-nums text-muted-foreground sm:block"
                >
                  {{ meta(entry.row) }}
                </span>
                <ChevronRight
                  class="h-4 w-4 shrink-0 text-muted-foreground/40 transition-transform"
                  :class="{ 'rotate-90': expanded === entry.key }"
                />
              </button>

              <div v-if="expanded === entry.key" class="px-5 pb-4">
                <div class="rounded-xl bg-muted/40 p-3">
                  <p class="text-sm leading-snug">{{ entry.row.msg }}</p>
                  <dl class="mt-3 grid grid-cols-2 gap-x-4 gap-y-1 text-xs sm:grid-cols-4">
                    <div>
                      <dt class="text-muted-foreground">Program</dt>
                      <dd class="mt-0.5 truncate">{{ entry.row.program_name || 'daemon' }}</dd>
                    </div>
                    <div>
                      <dt class="text-muted-foreground">Result</dt>
                      <dd class="mt-0.5 font-mono">{{ eventResult(entry.row) }}</dd>
                    </div>
                    <div>
                      <dt class="text-muted-foreground">Duration</dt>
                      <dd class="mt-0.5 font-mono">
                        {{ entry.row.duration_secs != null ? `${entry.row.duration_secs}s` : '—' }}
                      </dd>
                    </div>
                    <div>
                      <dt class="text-muted-foreground">Timestamp</dt>
                      <dd class="mt-0.5 font-mono">{{ eventTimestamp(entry.row.ts) }}</dd>
                    </div>
                  </dl>
                  <div class="mt-3 flex flex-wrap items-center gap-2">
                    <UiButton
                      v-if="entry.row.program_id"
                      size="sm"
                      variant="secondary"
                      @click="openProgram(entry.row.program_id)"
                    >
                      Open program
                    </UiButton>
                    <UiButton size="sm" variant="ghost" @click="copyEvent(entry.row, entry.key)">
                      {{ copiedKey === entry.key ? 'Copied' : 'Copy event' }}
                    </UiButton>
                  </div>
                </div>
              </div>
            </div>
          </template>
        </div>
      </div>

      <footer
        class="flex items-center justify-between gap-3 border-t border-border/40 px-5 py-2.5 text-[11px] text-muted-foreground"
      >
        <template v-if="compact">
          <span class="tabular-nums">
            {{ lastLoaded ? `Updated ${eventTime(Math.floor(lastLoaded / 1000))}` : '—' }}
          </span>
          <button
            type="button"
            class="rounded text-muted-foreground transition-colors hover:text-foreground"
            @click="openInEvents"
          >
            Open in Events ↗
          </button>
        </template>
        <template v-else>
          <span class="tabular-nums">
            Showing {{ pageStart }}–{{ pageEnd }} · Page {{ page }}
          </span>
          <div class="flex items-center gap-2">
            <UiButton variant="outline" size="sm" :disabled="offset === 0 || loading" @click="goNewer">
              Newer
            </UiButton>
            <UiButton
              variant="outline"
              size="sm"
              :disabled="rows.length < pageSize || loading"
              @click="goOlder"
            >
              Older
            </UiButton>
          </div>
        </template>
      </footer>
    </section>
  </div>
</template>
