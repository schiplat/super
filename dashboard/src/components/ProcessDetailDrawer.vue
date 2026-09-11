<script setup lang="ts">
import { ref, watch, onUnmounted, nextTick, computed } from 'vue';
import { useRouter } from 'vue-router';
import {
  X, Terminal as TerminalIcon, FileText, Cpu,
  Copy, Check, Folder, User, PlayCircle, Layers, Activity,
  Link2, TerminalSquare, AlertTriangle, Clock, Edit, Gauge, RefreshCw, History,
  ChevronLeft, ChevronRight, ArrowDown, ArrowUp
} from 'lucide-vue-next';
import { useProgramStore } from '@/stores/program';
import { useAuthStore } from '@/stores/auth';
import apiClient from '@/api/client';
import { API_PATHS } from '@/api/paths';
import LogTerminal from '@/components/LogTerminal.vue';
import StatusBadge from '@/components/StatusBadge.vue';
import ActionButtons from '@/components/ActionButtons.vue';
import Slot from '@/slots/Slot.vue';
import UiBadge from '@/components/ui/UiBadge.vue';
import UiButton from '@/components/ui/UiButton.vue';
import type { ProgramDetail, ProgramLogsResponse, ProgramEventRecord, ProcessStatus, EventStats } from '@/types';
import { useClipboard } from '@vueuse/core';
import { format } from 'date-fns';
import { alertDialog } from '@/lib/app-dialog';

const props = defineProps<{
  modelValue: boolean;
  processId: string | null;
}>();

const emit = defineEmits(['update:modelValue', 'change-process']);
const store = useProgramStore();
const authStore = useAuthStore();
const { copy, copied } = useClipboard();
const nameCopied = ref(false);
const idCopied = ref(false);
const copyTimers: Record<string, ReturnType<typeof setTimeout> | null> = { name: null, id: null };
function flashCopied( which: 'name' | 'id') {
  const flag = which === 'name' ? nameCopied : idCopied;
  flag.value = true;
  if (copyTimers[which]) clearTimeout(copyTimers[which]!);
  copyTimers[which] = setTimeout(() => { flag.value = false; }, 1500);
}
async function copyProgramName() {
  const name = summaryData.value?.name;
  if (!name) return;
  await copy(name);
  flashCopied('name');
}
async function copyProgramId() {
  if (!props.processId) return;
  await copy(props.processId);
  flashCopied('id');
}
const router = useRouter();

const activeTab = ref<'logs' | 'config' | 'events'>('logs');
const logMode = ref<'live' | 'history'>('live');
const historyLines = ref(200);
const historySource = ref<'all' | 'stdout' | 'stderr'>('all');
const historyHint = ref<string | null>(null);

// Events tab state
const eventsLoading = ref(false);
const eventsError = ref<string | null>(null);
const events = ref<ProgramEventRecord[]>([]);
const eventsTotal = ref<number | null>(null);
const eventsPage = ref(1);
const eventsPageSize = ref(20);
type EventsSortField = 'time' | 'event' | 'exit_code' | 'signal' | 'retry_count' | 'duration_secs' | 'msg';
const eventsSortField = ref<EventsSortField>('time');
const eventsSortDir = ref<'desc' | 'asc'>('desc');
const eventsFilterType = ref('');
const eventsFilterExit = ref('');
const eventsFilterQ = ref('');
const eventsPageSizeOptions = [10, 20, 50, 100];
const eventsTypeOptions = [
  '',
  'process_fatal',
  'process_backoff',
  'process_recovered',
  'process_started',
  'process_exit',
  'health_restart',
  'cron_started',
  'cron_exit',
  'cron_spawn_failed',
  'queue_full',
  'memory_pressure',
  'memory_oom_kill',
];

const eventsHasPrev = computed(() => eventsPage.value > 1);
const eventsHasNext = computed(() => {
  if (eventsTotal.value != null) {
    return eventsPage.value * eventsPageSize.value < eventsTotal.value;
  }
  return events.value.length >= eventsPageSize.value;
});
const eventsRangeLabel = computed(() => {
  if (events.value.length === 0) return '0 events';
  const start = (eventsPage.value - 1) * eventsPageSize.value + 1;
  const end = start + events.value.length - 1;
  if (eventsTotal.value != null) return `${start}–${end} of ${eventsTotal.value}`;
  return `${start}–${end}`;
});

const streamOptions = [{ value: 'stderr' as const, label: 'stderr' }, { value: 'stdout' as const, label: 'stdout' }, { value: 'all' as const, label: 'all' }];
const tailOptions = [100, 200, 500, 1000];
const historyLoading = ref(false);
const historyError = ref<string | null>(null);
const historyContent = ref('');
const terminalRef = ref<InstanceType<typeof LogTerminal> | null>(null);
const isLoadingDetail = ref(false);
const detailData = ref<ProgramDetail | null>(null);

function formatTime(timestamp: number) { return timestamp ? format(new Date(timestamp * 1000), 'yyyy-MM-dd HH:mm') : '—'; }

const summaryData = computed(() => store.programs.find(p => p.id === props.processId));

/**
 * Slot tab API for extensions on `process.detail.tabs`. A plugin calls
 * context.registerTab({ id, label, render }) and renders its tab button +
 * panel itself; the drawer only mounts the anchor at the end of the strip.
 */
function registerTab(tab: { id: string; label: string; render: () => unknown }) {
  const cur = activeTab.value as string;
  void cur;
  console.debug('[Slot:process.detail.tabs] registerTab', tab.id, tab.label);
  return () => {};
}

const currentError = computed(() =>
  (detailData.value as any)?.last_error || (detailData.value as any)?.health_error || (summaryData.value as any)?.last_error || (summaryData.value as any)?.health_error || null
);

const isHealthCheckError = computed(() => {
  const fromDetail = (detailData.value as any)?.health_error; const fromSummary = (summaryData.value as any)?.health_error;
  return !!(fromDetail || fromSummary) && !((detailData.value as any)?.last_error || (summaryData.value as any)?.last_error);
});

const fullCommand = computed(() => {
  if (!detailData.value) return ''; const cfg = detailData.value.config; return [cfg.command, ...cfg.args].join(' ');
});

const commandBinary = computed(() => detailData.value ? detailData.value.config.command : '');
const commandArgs = computed(() => detailData.value ? detailData.value.config.args || [] : []);

const hasHooks = computed(() => {
  const h = detailData.value?.config.hooks; if (!h) return false; return Object.values(h).some(cmd => !!cmd);
});

const resourceLimits = computed(() => detailData.value?.config.resource_limits);
const hasResourceLimits = computed(() => {
  const rl = resourceLimits.value; if (!rl) return false; return (rl.cpu_quota != null && rl.cpu_quota > 0) || (rl.memory_limit != null && rl.memory_limit > 0);
});

const cpuQuotaLabel = computed(() => {
  const cpu = resourceLimits.value?.cpu_quota; if (cpu == null || cpu <= 0) return null; return `${cpu % 1 === 0 ? cpu.toFixed(0) : cpu.toFixed(2)} cores`;
});

const memoryLimitLabel = computed(() => {
  const mb = resourceLimits.value?.memory_limit; if (mb == null || mb <= 0) return null;
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`; return `${mb} MB`;
});

// Cron policy fields (present only when set)
const cronPolicy = computed(() => {
  const c = detailData.value?.config as any;
  return {
    maxConcurrent: c?.max_concurrent ?? null,
    maxQueued: c?.max_queued ?? null,
    onOverlap: c?.on_overlap ?? null,
    catchup: c?.catchup ?? null,
    jitterSec: c?.jitter_sec ?? null,
  };
});
const hasCronPolicy = computed(() => {
  const p = cronPolicy.value;
  return p.maxConcurrent != null || p.maxQueued != null || p.onOverlap != null || p.catchup != null || p.jitterSec != null;
});

// Health check tuning shown with effective (0 = default) values
const healthTuning = computed(() => {
  const hc = detailData.value?.config.health_check;
  if (!hc) return null;
  return {
    interval: hc.interval_secs || 10,
    timeout: hc.timeout_secs || (hc.type === 'tcp' ? 3 : 5),
    startPeriod: hc.start_period_secs || 0,
    maxFailures: hc.max_failures ?? 3,
  };
});

const showOtaArtifact = computed(() => {
  const cfg = detailData.value?.config;
  if (!cfg) return false;
  return !!(cfg.artifact || cfg.restore_path);
});

const otaChecksumShort = computed(() => {
  const sum = detailData.value?.config.artifact?.checksum?.trim() || '';
  if (!sum) return '—';
  return sum.length <= 16 ? sum : `${sum.slice(0, 8)}…${sum.slice(-8)}`;
});

const otaSourceTruncated = computed(() => {
  const src = detailData.value?.config.artifact?.source || '';
  if (!src) return '—';
  if (src.length <= 64) return src;
  return `${src.slice(0, 28)}…${src.slice(-28)}`;
});

const isSignalRestartWithoutProbe = computed(() => {
  const cfg = detailData.value?.config;
  if (!cfg?.artifact) return false;
  const rp = (cfg.artifact.restart_policy || '').trim().toLowerCase();
  if (!(rp === 'signal' || rp.startsWith('signal:'))) return false;
  const hc = cfg.health_check;
  return !hc || (hc as { type?: string }).type === 'disabled';
});

const programStatus = computed((): ProcessStatus | undefined => {
  // Prefer store summary when it already reflects a health failure (detail can lag one tick).
  const summary = summaryData.value;
  if (summary?.status === 'Running' && summary.health_error) return 'Running';
  return detailData.value?.state ?? summary?.status;
});
const displayHealthError = computed(
  () => detailData.value?.health_error || summaryData.value?.health_error || null,
);
const healthLatestResult = computed(() => {
  if (!detailData.value?.config.health_check) return null;
  const status = programStatus.value;
  const err = displayHealthError.value;
  if (status === 'Healthy') {
    return { kind: 'passing' as const, label: 'Passing', detail: null as string | null };
  }
  if (err) {
    return { kind: 'failing' as const, label: 'Failing', detail: err };
  }
  if (status === 'Starting' || status === 'Running') {
    return { kind: 'probing' as const, label: 'Probing', detail: 'Waiting for a successful check (or still in start period).' };
  }
  return { kind: 'idle' as const, label: 'Not probing', detail: 'Process is not running.' };
});
const isActivelyRunning = computed(() => ['Running', 'Healthy', 'Starting'].includes(programStatus.value || ''));

const refreshingDetail = ref(false);

watch(() => summaryData.value?.status, (newStatus, oldStatus) => {
  if (newStatus && newStatus !== oldStatus && props.processId && props.modelValue) void refreshDetailSilent(props.processId);
});

async function refreshDetailSilent(id: string) {
  try {
    const res = await apiClient.get<ProgramDetail>(API_PATHS.PROGRAMS.DETAIL(id));
    detailData.value = res.data; const target = store.programs.find(p => p.id === id);
    if (target) { target.status = res.data.state; target.last_error = res.data.last_error; target.health_error = res.data.health_error; target.pid = res.data.pid; }
  } catch (e) { console.error('Failed to refresh detail silently:', e); }
}

async function refreshDetailNow() {
  if (!props.processId) return;
  refreshingDetail.value = true;
  try {
    // Always refresh header/status; also refresh the active tab's data.
    const jobs: Promise<unknown>[] = [fetchDetail(props.processId)];
    if (activeTab.value === 'events') {
      jobs.push(fetchEvents(props.processId));
    } else if (activeTab.value === 'logs' && logMode.value === 'history') {
      jobs.push(fetchHistory(props.processId));
    }
    await Promise.all(jobs);
  } finally {
    refreshingDetail.value = false;
  }
}

function syncLogDefaults() {
  const err = currentError.value; const status = programStatus.value;
  if (err || status === 'Fatal' || status === 'Backoff') { logMode.value = 'history'; historySource.value = 'stderr'; }
  else if (!isActivelyRunning.value) { logMode.value = 'history'; historySource.value = 'all'; }
  else { logMode.value = 'live'; historySource.value = 'all'; }
}

watch(historySource, async () => { if (logMode.value === 'history' && props.processId && props.modelValue) await fetchHistory(props.processId); });
watch(activeTab, (newTab) => { if (newTab === 'logs' && logMode.value === 'live') nextTick(() => terminalRef.value?.fit()); });

async function fetchHistory(id: string) {
  historyLoading.value = true; historyError.value = null; historyHint.value = null;
  try {
    const sourceParam = historySource.value === 'all' ? undefined : historySource.value;
    let res = await apiClient.get<ProgramLogsResponse>(API_PATHS.PROGRAMS.LOGS(id, historyLines.value, sourceParam));
    let logs = res.data.logs;
    if (historySource.value === 'stderr' && logs.length === 0) {
      const fallback = await apiClient.get<ProgramLogsResponse>(API_PATHS.PROGRAMS.LOGS(id, historyLines.value));
      logs = fallback.data.logs; if (logs.length > 0) historyHint.value = 'No stderr file captured; showing stdout + stderr.';
    }
    historyContent.value = logs.length ? logs.map((f) => (logs.length > 1 ? `--- ${f.source} ---\n${f.content}` : f.content)).join('\n\n') : '(no log files on disk yet)';
  } catch (e) { historyError.value = 'Failed to load historical logs'; historyContent.value = ''; console.error(e); }
  finally { historyLoading.value = false; }
}

async function fetchEvents(id: string, resetPage = false) {
  if (resetPage) eventsPage.value = 1;
  eventsLoading.value = true;
  eventsError.value = null;
  try {
    const exitRaw = eventsFilterExit.value.trim();
    const exitCode = exitRaw === '' ? undefined : Number(exitRaw);
    const query = {
      limit: eventsPageSize.value,
      offset: (eventsPage.value - 1) * eventsPageSize.value,
      sort_by: eventsSortField.value,
      order: eventsSortDir.value,
      event_type: eventsFilterType.value || undefined,
      exit_code: exitCode != null && !Number.isNaN(exitCode) ? exitCode : undefined,
      q: eventsFilterQ.value.trim() || undefined,
    };
    const [listRes, statsRes] = await Promise.all([
      apiClient.get<ProgramEventRecord[]>(API_PATHS.PROGRAMS.EVENTS(id, query)),
      apiClient.get<EventStats>(API_PATHS.EVENTS.STATS(id)).catch(() => null),
    ]);
    events.value = listRes.data || [];
    // Stats is unfiltered retention total — only show as total when no filters.
    const filtered = !!(query.event_type || query.exit_code != null || query.q);
    eventsTotal.value = filtered ? null : (statsRes?.data?.total ?? null);
  } catch (e) {
    eventsError.value = 'Failed to load event history';
    events.value = [];
    eventsTotal.value = null;
    console.error(e);
  } finally {
    eventsLoading.value = false;
  }
}

function applyEventFilters() {
  if (!props.processId) return;
  void fetchEvents(props.processId, true);
}

function clearEventFilters() {
  eventsFilterType.value = '';
  eventsFilterExit.value = '';
  eventsFilterQ.value = '';
  if (props.processId) void fetchEvents(props.processId, true);
}

function toggleEventsSort(field: EventsSortField) {
  if (eventsSortField.value === field) {
    eventsSortDir.value = eventsSortDir.value === 'desc' ? 'asc' : 'desc';
  } else {
    eventsSortField.value = field;
    // Time defaults to newest-first; other columns default ascending.
    eventsSortDir.value = field === 'time' ? 'desc' : 'asc';
  }
  if (props.processId) void fetchEvents(props.processId, true);
}

function changeEventsPageSize(size: number) {
  eventsPageSize.value = size;
  if (props.processId) void fetchEvents(props.processId, true);
}

function eventsPrevPage() {
  if (!eventsHasPrev.value || !props.processId) return;
  eventsPage.value -= 1;
  void fetchEvents(props.processId);
}

function eventsNextPage() {
  if (!eventsHasNext.value || !props.processId) return;
  eventsPage.value += 1;
  void fetchEvents(props.processId);
}

// Signal label helper (matches the notify plugin / OSS semantics)
function signalLabel(sig: number | null | undefined): string {
  if (sig == null) return '—';
  const names: Record<number, string> = { 1: 'HUP', 2: 'INT', 3: 'QUIT', 6: 'ABRT', 9: 'KILL', 11: 'SEGV', 13: 'PIPE', 14: 'ALRM', 15: 'TERM' };
  return names[sig] ? `${names[sig]} (${sig})` : String(sig);
}

function eventColor(event: string): string {
  switch (event) {
    case 'process_fatal':
    case 'memory_oom_kill':
    case 'cron_spawn_failed':
      return 'text-destructive';
    case 'process_backoff':
    case 'health_restart':
    case 'memory_pressure':
    case 'queue_full':
      return 'text-warning';
    case 'process_recovered':
    case 'process_started':
      return 'text-success';
    default:
      return 'text-muted-foreground';
  }
}

function eventLabel(event: string): string {
  switch (event) {
    case 'process_fatal': return 'Fatal';
    case 'process_backoff': return 'Backoff';
    case 'process_recovered': return 'Recovered';
    case 'process_started': return 'Started';
    case 'process_exit': return 'Exited';
    case 'health_restart': return 'Health Restart';
    case 'cron_started': return 'Cron Started';
    case 'cron_exit': return 'Cron Exit';
    case 'cron_spawn_failed': return 'Cron Spawn Failed';
    case 'queue_full': return 'Queue Full';
    case 'memory_pressure': return 'Memory Pressure';
    case 'memory_oom_kill': return 'OOM Kill';
    default: return event;
  }
}

function formatEventTime(ev: ProgramEventRecord): string {
  const ms = ev.ts_ms ?? (ev.ts ? ev.ts * 1000 : 0);
  return ms ? format(new Date(ms), 'yyyy-MM-dd HH:mm:ss') : '—';
}

function viewErrorLogs() { activeTab.value = 'logs'; logMode.value = 'history'; historySource.value = 'stderr'; historyHint.value = isHealthCheckError.value ? 'Look for amber [superd] lines in stderr.' : null; if (props.processId) void fetchHistory(props.processId); }

/** Bind WS log lines into the xterm instance. Must run on every drawer open — closing
 *  unsubscribes, and reopening the same processId does not re-fire the processId watcher. */
function bindLiveLogs(id: string) {
  store.subscribeLog(id, (line, source) => terminalRef.value?.writeLine(line, source));
}

function unbindLiveLogs(id: string | null | undefined) {
  if (id) store.unsubscribeLog(id);
}

watch(() => props.processId, async (newId, oldId) => {
  if (oldId) unbindLiveLogs(oldId);
  if (!newId) return;
  detailData.value = null;
  if (props.modelValue) {
    terminalRef.value?.clear();
    bindLiveLogs(newId);
    await fetchDetail(newId); syncLogDefaults();
    if (logMode.value === 'history') await fetchHistory(newId); else if (activeTab.value === 'logs') nextTick(() => terminalRef.value?.fit());
    void fetchEvents(newId);
  }
});

watch(() => props.modelValue, (isOpen) => {
  if (!isOpen) {
    unbindLiveLogs(props.processId);
    return;
  }
  if (!props.processId) return;
  // Re-subscribe every open (same id after close would otherwise stay silent).
  bindLiveLogs(props.processId);
  void fetchDetail(props.processId).then(() => {
    syncLogDefaults();
    if (logMode.value === 'history') void fetchHistory(props.processId!);
    else setTimeout(() => { if (activeTab.value === 'logs') terminalRef.value?.fit(); }, 300);
  });
  void fetchEvents(props.processId);
}, { immediate: true });

// Remounted LogTerminal (v-if) needs a fit after switching back to live; subscription is already active.
watch(logMode, async (mode) => {
  if (mode === 'live') {
    if (props.modelValue && props.processId) bindLiveLogs(props.processId);
    nextTick(() => terminalRef.value?.fit());
  } else if (props.processId && props.modelValue) {
    await fetchHistory(props.processId);
  }
});

async function fetchDetail(id: string) {
  isLoadingDetail.value = true;
  try {
    const res = await apiClient.get<ProgramDetail>(API_PATHS.PROGRAMS.DETAIL(id)); detailData.value = res.data;
    const target = store.programs.find(p => p.id === id);
    if (target) { target.status = res.data.state; target.last_error = res.data.last_error; target.health_error = res.data.health_error; target.pid = res.data.pid; }
  } catch (e) { console.error('Failed to fetch details', e); }
  finally { isLoadingDetail.value = false; }
}

async function jumpToDependency(depName: string) {
  const target = store.programs.find(p => p.name === depName);
  if (target) emit('change-process', target.id);
  else {
    await alertDialog(`Dependency "${depName}" not found.`, {
      title: 'Not found',
    });
  }
}
onUnmounted(() => { unbindLiveLogs(props.processId); });
function close() { emit('update:modelValue', false); }
function goToEdit() { if (props.processId) router.push(`/programs/${props.processId}/edit`); }
</script>

<template>
  <div v-if="modelValue" class="fixed inset-0 z-[100] bg-foreground/20 backdrop-blur-[2px] transition-opacity" @click="close"></div>
  <div class="fixed inset-y-0 right-0 z-[101] w-full md:w-[850px] bg-card shadow-2xl transform transition-transform duration-300 ease-in-out flex flex-col border-l border-border" :class="modelValue ? 'translate-x-0' : 'translate-x-full'">
    <div v-if="processId" class="flex flex-col h-full">

      <!-- Header -->
      <div class="px-5 md:px-7 pt-5 pb-0 bg-card shrink-0" :class="currentError ? '' : 'border-b border-border'">
        <div class="flex items-start gap-3">
          <div class="w-11 h-11 bg-muted rounded-2xl flex items-center justify-center text-muted-foreground shrink-0 mt-0.5 border border-border/60"><Cpu class="w-5 h-5" /></div>
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2 min-w-0">
              <h2 class="text-lg font-bold tracking-tight truncate text-foreground">{{ summaryData?.name || 'Loading...' }}</h2>
              <button
                v-if="summaryData?.name"
                type="button"
                class="inline-flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground/50 hover:text-foreground hover:bg-muted transition-colors shrink-0"
                :title="nameCopied ? 'Copied' : 'Copy name'"
                @click="copyProgramName"
              >
                <Check v-if="nameCopied" class="w-3.5 h-3.5 text-success" />
                <Copy v-else class="w-3.5 h-3.5" />
              </button>
              <StatusBadge
                :status="programStatus || 'Stopped'"
                :health-error="displayHealthError"
              />
            </div>
            <div class="flex items-center gap-1.5 mt-1.5 mb-3 min-w-0">
              <span class="text-xs text-muted-foreground font-mono tracking-tight truncate" :title="processId">{{ processId.slice(0, 8) }}</span>
              <button
                v-if="processId"
                type="button"
                class="inline-flex h-5 w-5 items-center justify-center rounded text-muted-foreground/40 hover:text-foreground hover:bg-muted transition-colors shrink-0"
                :title="idCopied ? 'Copied' : 'Copy full id'"
                @click="copyProgramId"
              >
                <Check v-if="idCopied" class="w-3 h-3 text-success" />
                <Copy v-else class="w-3 h-3" />
              </button>
              <span v-if="summaryData?.pid" class="text-xs text-muted-foreground/30 select-none">·</span>
              <span v-if="summaryData?.pid" class="text-xs text-muted-foreground font-mono">pid {{ summaryData.pid }}</span>
            </div>
          </div>
          <div class="flex items-center gap-1 shrink-0 -mt-0.5">
            <button
              class="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
              :disabled="refreshingDetail"
              :title="activeTab === 'events' ? 'Refresh status and events' : activeTab === 'logs' && logMode === 'history' ? 'Refresh status and log history' : 'Refresh status'"
              @click="refreshDetailNow"
            >
              <RefreshCw class="w-3.5 h-3.5" :class="{ 'animate-spin': refreshingDetail }" />
              <span class="hidden sm:inline">Refresh</span>
            </button>
            <button v-if="authStore.canManage" class="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-muted transition-colors" @click="goToEdit">
              <Edit class="w-3.5 h-3.5" /><span class="hidden sm:inline">Edit</span>
            </button>
            <button class="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground/50 hover:text-foreground hover:bg-muted transition-colors ml-1" @click="close"><X class="w-4 h-4" /></button>
          </div>
        </div>
        <div class="py-3 border-t border-border/60"><ActionButtons :id="processId" :status="programStatus || 'Stopped'" :name="summaryData?.name" /></div>
      </div>

      <!-- Error banner -->
      <div v-if="currentError" class="px-6 py-4 flex items-start gap-3 shrink-0" :class="isHealthCheckError ? 'bg-warning/5' : 'bg-destructive/5'">
        <div class="mt-0.5 shrink-0" :class="isHealthCheckError ? 'text-warning' : 'text-destructive'"><AlertTriangle class="w-5 h-5" /></div>
        <div class="flex-1 min-w-0">
          <h3 class="text-sm font-bold" :class="isHealthCheckError ? 'text-warning' : 'text-destructive'">{{ isHealthCheckError ? 'Health Check Failed' : 'Latest Error' }}</h3>
          <p class="text-xs mt-1 font-mono break-words leading-relaxed select-text" :class="isHealthCheckError ? 'text-warning/80' : 'text-destructive/80'">{{ currentError }}</p>
          <p class="text-xs mt-2 text-muted-foreground">
            <template v-if="isHealthCheckError">Check <strong>Configuration → health_check</strong>. Use the button below to open stderr file history.</template>
            <template v-else>superd keeps only the most recent error in memory. Use file logs below for full history.</template>
          </p>
          <button type="button" class="inline-flex items-center gap-1 mt-2 px-2.5 py-1 rounded-lg text-xs font-medium border transition-colors" :class="isHealthCheckError ? 'border-warning/30 text-warning hover:bg-warning/10' : 'border-destructive/30 text-destructive hover:bg-destructive/10'" @click="viewErrorLogs">
            {{ isHealthCheckError ? 'View stderr diagnostics' : 'View file log history' }}
          </button>
        </div>
      </div>

      <!-- Tabs -->
      <div class="px-5 md:px-7 border-b border-border bg-muted/40 shrink-0">
        <div class="flex gap-0">
          <button class="flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-[3px] transition-all -mb-px" :class="activeTab === 'logs' ? 'border-foreground text-foreground' : 'border-transparent text-muted-foreground hover:text-foreground hover:border-border'" @click="activeTab = 'logs'"><TerminalIcon class="w-4 h-4" />Logs</button>
          <button class="flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-[3px] transition-all -mb-px" :class="activeTab === 'events' ? 'border-foreground text-foreground' : 'border-transparent text-muted-foreground hover:text-foreground hover:border-border'" @click="activeTab = 'events'"><History class="w-4 h-4" />Events</button>
          <button class="flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-[3px] transition-all -mb-px" :class="activeTab === 'config' ? 'border-foreground text-foreground' : 'border-transparent text-muted-foreground hover:text-foreground hover:border-border'" @click="activeTab = 'config'"><FileText class="w-4 h-4" />Configuration</button>
          <!-- Plugin-registered tabs; each extension renders its own tab strip entry + panel via context.slotTabs -->
          <Slot name="process.detail.tabs" :context="{ process: summaryData, registerTab }" />
        </div>
      </div>

      <!-- Content -->
      <div class="flex-1 overflow-hidden relative bg-muted/30">
        <!-- Logs -->
        <div v-show="activeTab === 'logs'" class="absolute inset-0 flex flex-col">
          <div class="px-4 py-2.5 border-b border-border bg-card shrink-0 space-y-2">
            <div class="flex items-center justify-between gap-3">
              <div class="bg-muted/80 p-0.5 rounded-lg inline-flex border border-border/40">
                <button type="button" class="px-3 py-1.5 rounded-md text-xs font-medium transition-all" :class="logMode === 'live' ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'" @click="logMode = 'live'">Live stream</button>
                <button type="button" class="px-3 py-1.5 rounded-md text-xs font-medium transition-all" :class="logMode === 'history' ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'" @click="logMode = 'history'">Log history</button>
              </div>
              <span v-if="logMode === 'live'" class="text-xs text-muted-foreground/50 hidden sm:inline">WebSocket tail</span>
            </div>
            <div v-if="logMode === 'history'" class="flex flex-col gap-2 sm:flex-row sm:items-center sm:gap-3">
              <div class="bg-muted/80 p-0.5 rounded-lg inline-flex border border-border/40 shrink-0">
                <button v-for="opt in streamOptions" :key="opt.value" type="button" class="px-2.5 py-1 rounded-md text-xs font-medium transition-all" :class="historySource === opt.value ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'" @click="historySource = opt.value">{{ opt.label }}</button>
              </div>
              <div class="flex items-center gap-2 shrink-0">
                <div class="bg-muted/80 p-0.5 rounded-lg inline-flex border border-border/40">
                  <button v-for="n in tailOptions" :key="n" type="button" class="min-w-[2.25rem] px-2 py-1 rounded-md text-xs font-medium font-mono transition-all" :class="historyLines === n ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'" @click="historyLines = n; processId && fetchHistory(processId)">{{ n }}</button>
                </div>
                <span class="text-xs text-muted-foreground/50">lines</span>
              </div>
              <button type="button" class="inline-flex items-center px-2.5 py-1 rounded-lg text-xs font-medium border border-border text-muted-foreground hover:text-foreground hover:bg-muted transition-colors shrink-0 self-start sm:self-auto" :disabled="historyLoading || !processId" @click="processId && fetchHistory(processId)">Refresh</button>
            </div>
          </div>
          <div v-if="logMode === 'live'" class="flex-1 p-0 bg-[#1e1e1e] min-h-0"><LogTerminal ref="terminalRef" :logs="[]" /></div>
          <div v-else class="flex-1 min-h-0 overflow-auto bg-[#1e1e1e] p-4">
            <div v-if="historyLoading" class="text-gray-400 text-sm">Loading...</div>
            <div v-else-if="historyError" class="text-destructive/70 text-sm">{{ historyError }}</div>
            <template v-else>
              <p v-if="historyHint" class="text-amber-300/90 text-xs mb-3">{{ historyHint }}</p>
              <pre class="text-xs text-gray-200 font-mono whitespace-pre-wrap break-words">{{ historyContent }}</pre>
            </template>
          </div>
        </div>

        <!-- Events -->
        <div v-show="activeTab === 'events'" class="absolute inset-0 flex flex-col min-h-0">
          <div class="px-4 py-3 border-b border-border bg-card shrink-0 space-y-2.5">
            <div class="flex flex-col gap-2 lg:flex-row lg:items-center lg:flex-wrap">
              <select
                v-model="eventsFilterType"
                class="h-8 rounded-lg border border-border bg-background px-2.5 text-xs text-foreground"
                @change="applyEventFilters"
              >
                <option value="">All types</option>
                <option v-for="t in eventsTypeOptions.filter(Boolean)" :key="t" :value="t">{{ eventLabel(t) }}</option>
              </select>
              <input
                v-model="eventsFilterExit"
                type="number"
                placeholder="Exit code"
                class="h-8 w-28 rounded-lg border border-border bg-background px-2.5 text-xs font-mono text-foreground"
                @keydown.enter="applyEventFilters"
              />
              <input
                v-model="eventsFilterQ"
                type="search"
                placeholder="Search message…"
                class="h-8 min-w-[10rem] flex-1 rounded-lg border border-border bg-background px-2.5 text-xs text-foreground"
                @keydown.enter="applyEventFilters"
              />
              <div class="flex items-center gap-1.5 shrink-0">
                <UiButton size="sm" variant="secondary" @click="applyEventFilters">Filter</UiButton>
                <UiButton size="sm" variant="ghost" @click="clearEventFilters">Clear</UiButton>
              </div>
            </div>
            <div class="flex items-center justify-between gap-3 text-xs text-muted-foreground">
              <span>{{ eventsRangeLabel }}</span>
              <div class="flex items-center gap-2">
                <span class="hidden sm:inline">Rows</span>
                <select
                  :value="eventsPageSize"
                  class="h-7 rounded-md border border-border bg-background px-1.5 text-xs"
                  @change="changeEventsPageSize(Number(($event.target as HTMLSelectElement).value))"
                >
                  <option v-for="n in eventsPageSizeOptions" :key="n" :value="n">{{ n }}</option>
                </select>
                <button
                  type="button"
                  class="inline-flex h-7 w-7 items-center justify-center rounded-md border border-border disabled:opacity-30 hover:bg-muted"
                  :disabled="!eventsHasPrev || eventsLoading"
                  @click="eventsPrevPage"
                >
                  <ChevronLeft class="w-4 h-4" />
                </button>
                <span class="font-mono tabular-nums min-w-[3rem] text-center">p.{{ eventsPage }}</span>
                <button
                  type="button"
                  class="inline-flex h-7 w-7 items-center justify-center rounded-md border border-border disabled:opacity-30 hover:bg-muted"
                  :disabled="!eventsHasNext || eventsLoading"
                  @click="eventsNextPage"
                >
                  <ChevronRight class="w-4 h-4" />
                </button>
              </div>
            </div>
          </div>

          <div class="flex-1 min-h-0 overflow-auto">
            <div v-if="eventsLoading" class="flex justify-center py-10">
              <span class="inline-block w-6 h-6 border-2 border-muted-foreground/30 border-t-foreground/50 rounded-full animate-spin"></span>
            </div>
            <div v-else-if="eventsError" class="flex flex-col items-center justify-center h-full text-muted-foreground/50 gap-3 py-10">
              <History class="w-10 h-10" />
              <p class="text-sm font-medium">{{ eventsError }}</p>
            </div>
            <div v-else-if="events.length === 0" class="flex flex-col items-center justify-center h-full text-muted-foreground/50 gap-3 py-10">
              <History class="w-10 h-10" />
              <p class="text-sm font-medium">No lifecycle events recorded</p>
              <p class="text-xs max-w-sm text-center">Crashes, OOM kills (signal 9), backoff retries, cron runs, and recoveries appear here once they happen.</p>
            </div>
            <table v-else class="w-full text-left text-xs border-collapse">
              <thead class="sticky top-0 z-10 bg-muted/95 backdrop-blur-sm border-b border-border">
                <tr class="text-[10px] uppercase tracking-wider text-muted-foreground">
                  <th
                    v-for="col in ([
                      { field: 'time', label: 'Time' },
                      { field: 'event', label: 'Event' },
                      { field: 'exit_code', label: 'Exit' },
                      { field: 'signal', label: 'Signal' },
                      { field: 'retry_count', label: 'Retry' },
                      { field: 'duration_secs', label: 'Duration' },
                      { field: 'msg', label: 'Message' },
                    ] as const)"
                    :key="col.field"
                    class="px-3 py-2.5 font-semibold whitespace-nowrap cursor-pointer select-none hover:text-foreground"
                    @click="toggleEventsSort(col.field)"
                  >
                    <span class="inline-flex items-center gap-1.5">
                      {{ col.label }}
                      <ArrowUp v-if="eventsSortField === col.field && eventsSortDir === 'asc'" class="w-3 h-3" />
                      <ArrowDown v-else-if="eventsSortField === col.field && eventsSortDir === 'desc'" class="w-3 h-3" />
                    </span>
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="(ev, idx) in events"
                  :key="`${ev.ts_ms ?? ev.ts}-${ev.event}-${idx}`"
                  class="border-b border-border/60 hover:bg-muted/40 align-top"
                >
                  <td class="px-3 py-2.5 font-mono text-muted-foreground whitespace-nowrap">{{ formatEventTime(ev) }}</td>
                  <td class="px-3 py-2.5 whitespace-nowrap">
                    <span class="font-semibold" :class="eventColor(ev.event)">{{ eventLabel(ev.event) }}</span>
                  </td>
                  <td class="px-3 py-2.5 font-mono whitespace-nowrap" :class="ev.exit_code != null && ev.exit_code !== 0 ? 'text-destructive' : 'text-foreground/70'">
                    {{ ev.exit_code ?? '—' }}
                  </td>
                  <td class="px-3 py-2.5 font-mono whitespace-nowrap">
                    <span :class="ev.signal === 9 ? 'text-destructive font-bold' : 'text-foreground/70'">{{ signalLabel(ev.signal) }}</span>
                    <span v-if="ev.signal === 9" class="ml-1 text-[10px] text-destructive/80">OOM?</span>
                  </td>
                  <td class="px-3 py-2.5 font-mono text-muted-foreground whitespace-nowrap">{{ ev.retry_count ?? '—' }}</td>
                  <td class="px-3 py-2.5 font-mono text-muted-foreground whitespace-nowrap">
                    {{ ev.duration_secs != null ? `${ev.duration_secs}s` : '—' }}
                  </td>
                  <td class="px-3 py-2.5 text-foreground/75 break-words max-w-md">{{ ev.msg || '—' }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>

        <!-- Config -->
        <div v-show="activeTab === 'config'" class="absolute inset-0 overflow-y-auto p-4 md:p-6 lg:p-8">
          <div v-if="isLoadingDetail" class="flex justify-center py-10"><span class="inline-block w-6 h-6 border-2 border-muted-foreground/30 border-t-foreground/50 rounded-full animate-spin"></span></div>
          <div v-else-if="!detailData" class="flex flex-col items-center justify-center h-full text-muted-foreground/50 gap-3">
            <FileText class="w-10 h-10" />
            <p class="text-sm font-medium">No configuration data available</p>
            <p class="text-xs">Try refreshing the page or reopening this detail panel.</p>
          </div>
            <div v-else class="flex flex-col gap-5 md:gap-6 max-w-4xl pb-10">
            <!-- Execution Context -->
            <section>
              <h3 class="text-xs font-bold text-muted-foreground/60 uppercase tracking-[0.06em] mb-3 flex items-center gap-2"><PlayCircle class="w-3.5 h-3.5" />Execution Context</h3>
              <div class="rounded-2xl bg-muted/60 overflow-hidden">
                <!-- Command -->
                <div class="px-5 py-4 bg-muted/55 group">
                  <div class="text-xs font-medium text-muted-foreground mb-2.5">Command</div>
                  <div class="flex flex-wrap items-end gap-x-3 gap-y-2">
                    <span class="inline-flex flex-col items-start gap-0.5">
                      <span class="inline-flex items-center px-1.5 py-px rounded text-[10px] uppercase tracking-wider font-bold leading-none bg-foreground/10 text-foreground/50">cmd</span>
                      <span class="font-mono text-sm font-semibold text-foreground/90">{{ commandBinary }}</span>
                    </span>
                    <template v-for="(arg, idx) in commandArgs" :key="idx">
                      <span class="inline-flex flex-col items-start gap-0.5">
                        <span class="inline-flex items-center px-1.5 py-px rounded text-[10px] uppercase tracking-wider font-bold leading-none bg-warning/12 text-warning">arg</span>
                        <span class="font-mono text-sm text-foreground/70">{{ arg }}</span>
                      </span>
                    </template>
                    <span v-if="commandArgs.length === 0" class="text-xs text-muted-foreground/30 italic">(no arguments)</span>
                    <button class="inline-flex h-6 w-6 items-center justify-center rounded-lg opacity-0 group-hover:opacity-100 transition-opacity shrink-0" @click="copy(fullCommand)" title="Copy full command">
                      <Check v-if="copied" class="w-3 h-3 text-success" /><Copy v-else class="w-3 h-3 text-muted-foreground" />
                    </button>
                  </div>
                </div>
                <!-- Meta -->
                <div class="grid grid-cols-1 md:grid-cols-2">
                  <div class="px-5 py-3.5 bg-muted/45 rounded-bl-2xl">
                    <div class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground mb-1"><Folder class="w-3 h-3" />Working Directory</div>
                    <div class="font-mono text-sm text-foreground">{{ detailData.config.cwd || '(default)' }}</div>
                  </div>
                  <div class="px-5 py-3.5 bg-muted/40 rounded-br-2xl">
                    <div class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground mb-1"><User class="w-3 h-3" />User / Group</div>
                    <div class="font-mono text-sm text-foreground">{{ detailData.config.user || 'root' }}<span v-if="detailData.config.group" class="text-muted-foreground/50">/ {{ detailData.config.group }}</span></div>
                  </div>
                </div>
                <!-- Timestamps -->
                <div class="grid grid-cols-1 md:grid-cols-2">
                  <div class="px-5 py-3 bg-muted/35 rounded-bl-2xl">
                    <div class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground mb-1"><Clock class="w-3 h-3" />Created At</div>
                    <div class="font-mono text-xs text-foreground/80">{{ formatTime(detailData.config.created_at) }}</div>
                  </div>
                  <div class="px-5 py-3 bg-muted/30 rounded-br-2xl">
                    <div class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground mb-1"><Clock class="w-3 h-3" />Updated At</div>
                    <div class="font-mono text-xs text-foreground/80">{{ formatTime(detailData.config.updated_at) }}</div>
                  </div>
                </div>
              </div>
            </section>

            <!-- Env -->
            <section v-if="detailData.config.env && Object.keys(detailData.config.env).length > 0">
              <h3 class="text-xs font-bold text-muted-foreground/60 uppercase tracking-[0.06em] mb-3 flex items-center gap-2"><Layers class="w-3.5 h-3.5" />Environment Variables</h3>
              <div class="rounded-2xl bg-muted/55 overflow-hidden">
                <div class="data-table-scroll">
                  <table class="data-table">
                    <thead><tr><th class="w-1/3 pl-4">Key</th><th class="pl-4">Value</th></tr></thead>
                    <tbody class="font-mono text-xs">
                      <tr v-for="(val, key) in detailData.config.env" :key="key" class="even:bg-muted/45 hover:bg-muted/60 transition-colors">
                        <td class="pl-4 font-semibold text-foreground/80 py-2.5">{{ key }}</td>
                        <td class="pl-4 text-muted-foreground break-all py-2.5">{{ val }}</td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </div>
            </section>

            <!-- Resource Isolation -->
            <section v-if="hasResourceLimits">
              <h3 class="text-xs font-bold text-muted-foreground/60 uppercase tracking-[0.06em] mb-3 flex items-center gap-2"><Gauge class="w-3.5 h-3.5" />Resource Isolation</h3>
              <div class="rounded-2xl bg-muted/55 overflow-hidden">
                <div class="grid grid-cols-1 sm:grid-cols-2">
                  <div v-if="cpuQuotaLabel" class="px-5 py-3.5 bg-muted/45 rounded-bl-2xl sm:rounded-bl-2xl sm:rounded-br-none"><div class="text-xs font-medium text-muted-foreground mb-1">CPU Quota</div><div class="font-mono text-sm font-semibold text-foreground">{{ cpuQuotaLabel }}</div></div>
                  <div v-if="memoryLimitLabel" class="px-5 py-3.5 bg-muted/40 rounded-br-2xl sm:rounded-br-2xl sm:rounded-bl-none"><div class="text-xs font-medium text-muted-foreground mb-1">Memory Limit</div><div class="font-mono text-sm font-semibold text-foreground">{{ memoryLimitLabel }}</div></div>
                </div>
              </div>
            </section>

            <!-- Cron Schedule (own section so configured cron is never buried) -->
            <section v-if="detailData.config.cron || hasCronPolicy">
              <h3 class="text-xs font-bold text-muted-foreground/60 uppercase tracking-[0.06em] mb-3 flex items-center gap-2"><Clock class="w-3.5 h-3.5" />Cron Schedule</h3>
              <div class="rounded-2xl bg-muted/55 overflow-hidden">
                <div class="px-5 py-4 space-y-3">
                  <div class="bg-background/60 rounded-lg px-3 py-2.5 font-mono text-sm text-foreground inline-block">{{ detailData.config.cron || '—' }}</div>
                  <div v-if="hasCronPolicy" class="grid grid-cols-2 sm:grid-cols-3 gap-x-4 gap-y-2">
                    <div v-if="cronPolicy.maxConcurrent != null" class="flex justify-between gap-3"><span class="text-xs text-muted-foreground/70">Max Concurrent</span><span class="font-mono text-xs text-foreground/80">{{ cronPolicy.maxConcurrent }}</span></div>
                    <div v-if="cronPolicy.maxQueued != null" class="flex justify-between gap-3"><span class="text-xs text-muted-foreground/70">Max Queued</span><span class="font-mono text-xs text-foreground/80">{{ cronPolicy.maxQueued }}</span></div>
                    <div v-if="cronPolicy.onOverlap != null" class="flex justify-between gap-3"><span class="text-xs text-muted-foreground/70">On Overlap</span><span class="font-mono text-xs text-foreground/80">{{ cronPolicy.onOverlap }}</span></div>
                    <div v-if="cronPolicy.catchup != null" class="flex justify-between gap-3"><span class="text-xs text-muted-foreground/70">Catchup</span><span class="font-mono text-xs text-foreground/80">{{ cronPolicy.catchup }}</span></div>
                    <div v-if="cronPolicy.jitterSec != null" class="flex justify-between gap-3"><span class="text-xs text-muted-foreground/70">Jitter</span><span class="font-mono text-xs text-foreground/80">{{ cronPolicy.jitterSec }}s</span></div>
                  </div>
                </div>
              </div>
            </section>

            <!-- Lifecycle & Advanced -->
            <section class="grid grid-cols-1 md:grid-cols-2 gap-5 items-start">
              <!-- Lifecycle -->
              <div>
                <h3 class="text-xs font-bold text-muted-foreground/60 uppercase tracking-[0.06em] mb-3 flex items-center gap-2"><Activity class="w-3.5 h-3.5" />Lifecycle</h3>
                <div class="rounded-2xl bg-muted/55 overflow-hidden">
                  <div class="px-5 py-4 space-y-3.5">
                    <div class="flex justify-between items-center"><span class="text-sm text-foreground/80">Autostart</span><UiBadge :variant="detailData.config.autostart ? 'success' : 'default'">{{ detailData.config.autostart ? 'Enabled' : 'Disabled' }}</UiBadge></div>
                    <div class="flex justify-between items-center"><span class="text-sm text-foreground/80">Retry Limit</span><span class="font-mono text-sm font-semibold text-foreground">{{ detailData.config.retry_limit }} times</span></div>
                    <div class="flex justify-between items-center"><span class="text-sm text-foreground/80">Autorestart</span><span class="font-mono text-sm text-foreground">{{ detailData.config.autorestart || 'unexpected' }}</span></div>
                    <div v-if="detailData.config.exitcodes?.length" class="flex justify-between items-center"><span class="text-sm text-foreground/80">Exitcodes</span><span class="font-mono text-sm text-foreground">{{ detailData.config.exitcodes.join(', ') }}</span></div>
                    <div class="flex justify-between items-center"><span class="text-sm text-foreground/80">Startsecs</span><span class="font-mono text-sm text-foreground">{{ detailData.config.startsecs ?? 10 }}s</span></div>
                  </div>
                  <div v-if="detailData.config.depends_on.length > 0" class="px-5 py-4 bg-muted/40">
                    <div class="text-xs font-bold text-muted-foreground/50 uppercase tracking-[0.06em] mb-2.5 flex items-center gap-1.5"><Link2 class="w-3 h-3" />Depends On</div>
                    <div class="flex flex-wrap gap-2">
                      <span v-for="dep in detailData.config.depends_on" :key="dep" class="inline-flex items-center px-2.5 py-1 rounded-lg text-xs font-medium bg-background/60 text-foreground/70 cursor-pointer hover:bg-primary/10 hover:text-primary transition-colors" @click="jumpToDependency(dep)">{{ dep }}</span>
                    </div>
                  </div>
                </div>
              </div>

              <!-- Advanced -->
              <div v-if="hasHooks || detailData.config.health_check">
                <h3 class="text-xs font-bold text-muted-foreground/60 uppercase tracking-[0.06em] mb-3 flex items-center gap-2"><TerminalSquare class="w-3.5 h-3.5" />Advanced</h3>
                <div class="flex flex-col gap-4">
                  <div v-if="detailData.config.health_check" class="rounded-2xl bg-muted/55 px-5 py-4">
                    <div class="flex items-center gap-2 mb-3"><span class="text-sm font-semibold text-foreground">Health Check</span></div>
                    <div
                      v-if="healthLatestResult"
                      class="mb-3 rounded-xl px-3.5 py-2.5 text-xs"
                      :class="{
                        'bg-success/10 text-success': healthLatestResult.kind === 'passing',
                        'bg-destructive/10 text-destructive': healthLatestResult.kind === 'failing',
                        'bg-warning/10 text-warning': healthLatestResult.kind === 'probing',
                        'bg-muted text-muted-foreground': healthLatestResult.kind === 'idle',
                      }"
                    >
                      <div class="flex items-center justify-between gap-2">
                        <span class="font-semibold uppercase tracking-wide text-[10px] opacity-80">Latest result</span>
                        <span class="font-bold">{{ healthLatestResult.label }}</span>
                      </div>
                      <p v-if="healthLatestResult.detail" class="mt-1.5 font-mono text-[11px] leading-relaxed break-words opacity-90">{{ healthLatestResult.detail }}</p>
                    </div>
                    <div class="rounded-xl bg-background/60 px-4 py-3 font-mono text-xs text-foreground/80 break-all">
                      <div v-if="detailData.config.health_check.type === 'tcp'"><UiBadge variant="success" size="sm">TCP</UiBadge> <span class="ml-1.5">{{ detailData.config.health_check.host }}:{{ detailData.config.health_check.port }}</span></div>
                      <div v-else-if="detailData.config.health_check.type === 'http'">
                        <div class="flex items-center gap-2 mb-0.5"><UiBadge variant="info" size="sm">HTTP</UiBadge><span class="font-bold uppercase text-xs">{{ detailData.config.health_check.method || 'GET' }}</span></div>
                        <div class="text-foreground/60 mt-1">{{ detailData.config.health_check.url }}</div>
                      </div>
                      <div v-else-if="detailData.config.health_check.type === 'exec'"><UiBadge variant="warning" size="sm">EXEC</UiBadge><span class="ml-1.5 text-foreground/70">$ {{ detailData.config.health_check.command }}</span></div>
                      <div v-if="healthTuning" class="mt-2.5 pt-2.5 border-t border-border/50 text-muted-foreground/80 space-y-1">
                        <div class="flex flex-wrap gap-x-4 gap-y-1">
                          <span>interval <span class="text-foreground/80">{{ healthTuning.interval }}s</span></span>
                          <span>timeout <span class="text-foreground/80">{{ healthTuning.timeout }}s</span></span>
                          <span>start period <span class="text-foreground/80">{{ healthTuning.startPeriod }}s</span></span>
                          <span>max failures <span class="text-foreground/80">{{ healthTuning.maxFailures }}</span></span>
                        </div>
                      </div>
                    </div>
                  </div>
                  <div v-if="hasHooks" class="rounded-2xl bg-muted/55 px-5 py-4">
                    <div class="flex items-center gap-2 mb-3"><span class="text-sm font-semibold text-foreground">Hooks</span></div>
                    <div class="space-y-3">
                      <div v-for="(cmd, key) in detailData.config.hooks" :key="key">
                        <div v-if="cmd" class="flex flex-col gap-1">
                          <UiBadge size="sm">{{ key.toString().replace('_', '-') }}</UiBadge>
                          <div class="rounded-lg bg-background/60 px-3 py-2 font-mono text-xs text-foreground/70 break-all">{{ cmd }}</div>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </section>

            <!-- OTA Artifact -->
            <section v-if="showOtaArtifact">
              <h3 class="text-xs font-bold text-muted-foreground/60 uppercase tracking-[0.06em] mb-3 flex items-center gap-2"><RefreshCw class="w-3.5 h-3.5" />OTA Artifact</h3>
              <div class="rounded-2xl bg-muted/55 overflow-hidden px-5 py-4 space-y-3">
                <div v-if="detailData.config.restore_path" class="flex flex-wrap items-center gap-2">
                  <UiBadge variant="warning">Upgrade verifying / pending rollback</UiBadge>
                </div>
                <div v-if="isSignalRestartWithoutProbe" class="flex flex-wrap items-center gap-2">
                  <UiBadge variant="warning">signal* missing health probe</UiBadge>
                  <span class="text-xs text-amber-700 dark:text-amber-400">Next OTA/update will fail until a real health_check is configured or restart_policy changes.</span>
                </div>
                <div v-if="detailData.config.artifact" class="grid grid-cols-1 sm:grid-cols-2 gap-x-4 gap-y-2 text-sm">
                  <div class="flex justify-between gap-3 sm:col-span-2"><span class="text-xs text-muted-foreground/70 shrink-0">Source</span><span class="font-mono text-xs text-foreground/80 text-right break-all" :title="detailData.config.artifact.source">{{ otaSourceTruncated }}</span></div>
                  <div class="flex justify-between gap-3"><span class="text-xs text-muted-foreground/70">Checksum</span><span class="font-mono text-xs text-foreground/80" :title="detailData.config.artifact.checksum">{{ otaChecksumShort }}</span></div>
                  <div class="flex justify-between gap-3"><span class="text-xs text-muted-foreground/70">Extract</span><span class="font-mono text-xs text-foreground/80">{{ detailData.config.artifact.extract ? 'yes' : 'no' }}</span></div>
                  <div class="flex justify-between gap-3 sm:col-span-2"><span class="text-xs text-muted-foreground/70 shrink-0">Destination</span><span class="font-mono text-xs text-foreground/80 text-right break-all">{{ detailData.config.artifact.destination }}</span></div>
                  <div class="flex justify-between gap-3 sm:col-span-2"><span class="text-xs text-muted-foreground/70">Restart policy</span><span class="font-mono text-xs text-foreground/80">{{ detailData.config.artifact.restart_policy || 'immediate' }}</span></div>
                  <div class="flex justify-between gap-3"><span class="text-xs text-muted-foreground/70">Download timeout</span><span class="font-mono text-xs text-foreground/80">{{ detailData.config.artifact.download_timeout ?? 60 }}s</span></div>
                  <div class="flex justify-between gap-3"><span class="text-xs text-muted-foreground/70">Verify timeout</span><span class="font-mono text-xs text-foreground/80">{{ detailData.config.artifact.verify_timeout ?? 60 }}s</span></div>
                </div>
                <div v-if="detailData.config.restore_path" class="pt-2 border-t border-border/50">
                  <div class="flex justify-between gap-3"><span class="text-xs text-muted-foreground/70 shrink-0">Backup (bak)</span><span class="font-mono text-xs text-foreground/80 text-right break-all">{{ detailData.config.restore_path }}</span></div>
                </div>
              </div>
            </section>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
