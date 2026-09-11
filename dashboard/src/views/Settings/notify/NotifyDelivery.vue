<script setup lang="ts">
import { ref, onMounted, onUnmounted, inject, type Ref } from 'vue';
import { Bell, RefreshCw, ArrowUp, ArrowDown } from 'lucide-vue-next';
import apiClient from '@/api/client';
import {
  type NotificationConfig, type NotifyStats,
  type DeliveryRecord, type DeliveryStats, type DeliveryOutcome,
  TRIGGER_OPTIONS,
} from '@/types/notification';
import UiButton from '@/components/ui/UiButton.vue';
import UiBadge from '@/components/ui/UiBadge.vue';
import { format } from 'date-fns';
import type { NotifyHeaderAction } from './NotifyLayout.vue';

const EVENT_OPTIONS = TRIGGER_OPTIONS.filter(t => t.value !== '*');

type SortField = 'time' | 'outcome' | 'channel' | 'event' | 'program' | 'detail';

const headerAction = inject<Ref<NotifyHeaderAction>>('notifyHeaderAction');

const channels = ref<{ id: string; name: string }[]>([]);
const stats = ref<NotifyStats>({ success: 0, failed: 0, suppressed: 0, snapshots: [], channels: {} });

const deliveryLoading = ref(false);
const deliveries = ref<DeliveryRecord[]>([]);
const deliveryStats = ref<DeliveryStats>({ total: 0, ok: 0, fail: 0, cooldown: 0, inhibited: 0, keep_days: 14 });
const deliveryOutcome = ref<'' | DeliveryOutcome>('');
const deliveryChannelId = ref('');
const deliveryEventType = ref('');
const deliveryQ = ref('');
const deliveryPage = ref(1);
const deliveryPageSize = ref(20);
const expandedDeliveryId = ref<number | null>(null);

const sortField = ref<SortField>('time');
const sortDir = ref<'asc' | 'desc'>('desc');

function toggleSort(field: SortField) {
  if (sortField.value === field) {
    sortDir.value = sortDir.value === 'asc' ? 'desc' : 'asc';
  } else {
    sortField.value = field;
    sortDir.value = field === 'time' ? 'desc' : 'asc';
  }
  deliveryPage.value = 1;
  void fetchDeliveries();
}

async function fetchDeliveries() {
  deliveryLoading.value = true;
  try {
    const params = new URLSearchParams();
    params.set('limit', String(deliveryPageSize.value));
    params.set('offset', String((deliveryPage.value - 1) * deliveryPageSize.value));
    params.set('sort_by', sortField.value);
    params.set('order', sortDir.value);
    if (deliveryOutcome.value) params.set('outcome', deliveryOutcome.value);
    if (deliveryChannelId.value) params.set('channel_id', deliveryChannelId.value);
    if (deliveryEventType.value) params.set('event_type', deliveryEventType.value);
    if (deliveryQ.value.trim()) params.set('q', deliveryQ.value.trim());
    const [listRes, statsRes] = await Promise.all([
      apiClient.get<DeliveryRecord[]>(`/api/v1/system/notify/deliveries?${params}`),
      apiClient.get<DeliveryStats>('/api/v1/system/notify/deliveries/stats').catch(() => null),
    ]);
    deliveries.value = listRes.data || [];
    if (statsRes?.data) deliveryStats.value = statsRes.data;
  } catch (e) {
    console.error(e);
    deliveries.value = [];
  } finally {
    deliveryLoading.value = false;
  }
}

function applyDeliveryFilters() {
  deliveryPage.value = 1;
  void fetchDeliveries();
}

function toggleOutcomeFilter(outcome: DeliveryOutcome) {
  deliveryOutcome.value = deliveryOutcome.value === outcome ? '' : outcome;
  deliveryPage.value = 1;
  void fetchDeliveries();
}

function clearDeliveryFilters() {
  deliveryOutcome.value = '';
  deliveryChannelId.value = '';
  deliveryEventType.value = '';
  deliveryQ.value = '';
  deliveryPage.value = 1;
  void fetchDeliveries();
}

function outcomeChipClass(outcome: DeliveryOutcome, active: boolean): string {
  const base =
    'inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg font-medium tabular-nums transition-all cursor-pointer border';
  switch (outcome) {
    case 'ok':
      return active
        ? `${base} bg-success text-success-foreground border-success shadow-sm`
        : `${base} bg-success/10 text-success border-transparent hover:border-success/30`;
    case 'fail':
      return active
        ? `${base} bg-destructive text-destructive-foreground border-destructive shadow-sm`
        : `${base} bg-destructive/10 text-destructive border-transparent hover:border-destructive/30`;
    case 'cooldown':
      return active
        ? `${base} bg-warning text-warning-foreground border-warning shadow-sm`
        : `${base} bg-warning/10 text-warning border-transparent hover:border-warning/30`;
    case 'inhibited':
      return active
        ? `${base} bg-foreground text-background border-foreground shadow-sm`
        : `${base} bg-muted text-muted-foreground border-transparent hover:border-border`;
    default:
      return `${base} bg-muted text-muted-foreground border-transparent`;
  }
}

function outcomeLabel(o: DeliveryOutcome): string {
  switch (o) {
    case 'ok': return 'OK';
    case 'fail': return 'Fail';
    case 'cooldown': return 'Cooldown';
    case 'inhibited': return 'Inhibited';
    default: return o;
  }
}

function outcomeVariant(o: DeliveryOutcome): 'success' | 'destructive' | 'warning' | 'default' {
  switch (o) {
    case 'ok': return 'success';
    case 'fail': return 'destructive';
    case 'cooldown': return 'warning';
    default: return 'default';
  }
}

function eventLabel(type: string): string {
  const known = TRIGGER_OPTIONS.find((t) => t.value === type);
  if (known) return known.label;
  if (type === 'batch_summary') return 'Batch summary';
  if (type === 'notify_test') return 'Test webhook';
  return type;
}

function formatDeliveryTime(tsMs: number): string {
  return tsMs ? format(new Date(tsMs), 'MM-dd HH:mm:ss') : '—';
}

function deliveryDetail(row: DeliveryRecord): string {
  if (row.error) return row.error;
  if (row.http_status != null) return `HTTP ${row.http_status}`;
  if (row.detail) return row.detail;
  return '—';
}

function onPageSizeChange(e: Event) {
  deliveryPageSize.value = Number((e.target as HTMLSelectElement).value);
  deliveryPage.value = 1;
  void fetchDeliveries();
}

onMounted(async () => {
  if (headerAction) headerAction.value = null;
  try {
    const [cfgRes, notifyStatsRes] = await Promise.all([
      apiClient.get<NotificationConfig>('/api/v1/system/notify').catch(() => null),
      apiClient.get<NotifyStats>('/api/v1/system/notify/stats').catch(() => null),
    ]);
    if (cfgRes?.data?.channels) {
      channels.value = cfgRes.data.channels.map(ch => ({ id: ch.id, name: ch.name || ch.id.slice(0, 8) }));
    }
    if (notifyStatsRes?.data) stats.value = notifyStatsRes.data;
  } catch (e) {
    console.error(e);
  }
  await fetchDeliveries();
});

onUnmounted(() => {
  if (headerAction) headerAction.value = null;
});
</script>

<template>
  <div class="space-y-4">
    <!-- Stats chips (also outcome filters) -->
    <div class="flex flex-wrap items-center gap-2 text-xs" role="group" aria-label="Filter by outcome">
      <button type="button" title="Filter: OK" :class="outcomeChipClass('ok', deliveryOutcome === 'ok')" :aria-pressed="deliveryOutcome === 'ok'" @click="toggleOutcomeFilter('ok')">
        OK {{ deliveryStats.ok }}
      </button>
      <button type="button" title="Filter: Fail" :class="outcomeChipClass('fail', deliveryOutcome === 'fail')" :aria-pressed="deliveryOutcome === 'fail'" @click="toggleOutcomeFilter('fail')">
        Fail {{ deliveryStats.fail }}
      </button>
      <button type="button" title="Filter: Cooldown" :class="outcomeChipClass('cooldown', deliveryOutcome === 'cooldown')" :aria-pressed="deliveryOutcome === 'cooldown'" @click="toggleOutcomeFilter('cooldown')">
        Cooldown {{ deliveryStats.cooldown }}
      </button>
      <button type="button" title="Filter: Inhibited" :class="outcomeChipClass('inhibited', deliveryOutcome === 'inhibited')" :aria-pressed="deliveryOutcome === 'inhibited'" @click="toggleOutcomeFilter('inhibited')">
        Inhibited {{ deliveryStats.inhibited }}
      </button>
      <span class="text-muted-foreground/60 ml-1">Retained {{ deliveryStats.keep_days || '∞' }}d · since restart {{ stats.success }}/{{ stats.failed }}</span>
      <UiButton size="sm" variant="ghost" class="gap-1.5 ml-auto" :disabled="deliveryLoading" @click="fetchDeliveries">
        <RefreshCw class="w-3.5 h-3.5" :class="{ 'animate-spin': deliveryLoading }" /> Refresh
      </UiButton>
    </div>

    <!-- Filters -->
    <div class="flex flex-col gap-2 lg:flex-row lg:items-center lg:flex-wrap">
      <select v-model="deliveryChannelId" class="field-control !h-8 !text-xs !w-auto max-w-[12rem]" @change="applyDeliveryFilters">
        <option value="">All channels</option>
        <option v-for="ch in channels" :key="ch.id" :value="ch.id">{{ ch.name }}</option>
      </select>
      <select v-model="deliveryEventType" class="field-control !h-8 !text-xs !w-auto" @change="applyDeliveryFilters">
        <option value="">All events</option>
        <option v-for="ev in EVENT_OPTIONS" :key="ev.value" :value="ev.value">{{ ev.label }}</option>
        <option value="batch_summary">Batch summary</option>
        <option value="notify_test">Test webhook</option>
      </select>
      <input
        v-model="deliveryQ"
        type="search"
        placeholder="Search…"
        class="field-control !h-8 !text-xs min-w-[10rem] flex-1"
        @keydown.enter="applyDeliveryFilters"
      />
      <div class="flex items-center gap-1.5">
        <UiButton size="sm" variant="secondary" @click="applyDeliveryFilters">Filter</UiButton>
        <UiButton size="sm" variant="ghost" @click="clearDeliveryFilters">Clear</UiButton>
      </div>
    </div>

    <!-- Table -->
    <div class="flex flex-col">
      <div class="data-table-scroll">
        <table class="data-table">
          <thead>
            <tr>
              <th class="pl-6 cursor-pointer select-none whitespace-nowrap" @click="toggleSort('time')">
                <span class="inline-flex items-center gap-1.5">
                  Time
                  <ArrowUp v-if="sortField === 'time' && sortDir === 'asc'" class="w-3 h-3" />
                  <ArrowDown v-else-if="sortField === 'time' && sortDir === 'desc'" class="w-3 h-3" />
                </span>
              </th>
              <th class="cursor-pointer select-none whitespace-nowrap" @click="toggleSort('outcome')">
                <span class="inline-flex items-center gap-1.5">
                  Outcome
                  <ArrowUp v-if="sortField === 'outcome' && sortDir === 'asc'" class="w-3 h-3" />
                  <ArrowDown v-else-if="sortField === 'outcome' && sortDir === 'desc'" class="w-3 h-3" />
                </span>
              </th>
              <th class="cursor-pointer select-none" @click="toggleSort('channel')">
                <span class="inline-flex items-center gap-1.5">
                  Channel
                  <ArrowUp v-if="sortField === 'channel' && sortDir === 'asc'" class="w-3 h-3" />
                  <ArrowDown v-else-if="sortField === 'channel' && sortDir === 'desc'" class="w-3 h-3" />
                </span>
              </th>
              <th class="cursor-pointer select-none" @click="toggleSort('event')">
                <span class="inline-flex items-center gap-1.5">
                  Event
                  <ArrowUp v-if="sortField === 'event' && sortDir === 'asc'" class="w-3 h-3" />
                  <ArrowDown v-else-if="sortField === 'event' && sortDir === 'desc'" class="w-3 h-3" />
                </span>
              </th>
              <th class="cursor-pointer select-none" @click="toggleSort('program')">
                <span class="inline-flex items-center gap-1.5">
                  Program
                  <ArrowUp v-if="sortField === 'program' && sortDir === 'asc'" class="w-3 h-3" />
                  <ArrowDown v-else-if="sortField === 'program' && sortDir === 'desc'" class="w-3 h-3" />
                </span>
              </th>
              <th class="pr-6 cursor-pointer select-none" @click="toggleSort('detail')">
                <span class="inline-flex items-center gap-1.5">
                  Detail
                  <ArrowUp v-if="sortField === 'detail' && sortDir === 'asc'" class="w-3 h-3" />
                  <ArrowDown v-else-if="sortField === 'detail' && sortDir === 'desc'" class="w-3 h-3" />
                </span>
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="deliveryLoading && !deliveries.length">
              <td colspan="6" class="py-16 text-center text-muted-foreground">
                <span class="inline-block w-6 h-6 border-2 border-muted-foreground/30 border-t-foreground/50 rounded-full animate-spin"></span>
              </td>
            </tr>
            <tr v-else-if="!deliveries.length">
              <td colspan="6" class="py-16 text-center">
                <div class="flex flex-col items-center gap-3 text-muted-foreground/40">
                  <Bell class="w-10 h-10" />
                  <p class="font-medium text-foreground/80">No delivery records yet</p>
                  <p class="text-xs max-w-sm">Send a Test webhook or wait for a matching event. Success, failure, cooldown, and inhibition are stored here.</p>
                </div>
              </td>
            </tr>
            <template v-else v-for="row in deliveries" :key="row.id">
              <tr
                class="clickable"
                :class="{ 'bg-muted/50': expandedDeliveryId === row.id }"
                @click="expandedDeliveryId = expandedDeliveryId === row.id ? null : row.id"
              >
                <td class="pl-6 py-3 whitespace-nowrap !text-[11px] leading-tight text-muted-foreground font-mono tabular-nums">
                  {{ formatDeliveryTime(row.ts_ms) }}
                </td>
                <td class="py-3 whitespace-nowrap">
                  <UiBadge :variant="outcomeVariant(row.outcome)" size="sm">{{ outcomeLabel(row.outcome) }}</UiBadge>
                </td>
                <td class="py-3">
                  <span v-if="row.channel_name" class="font-medium text-foreground/90" :title="row.channel_name">{{ row.channel_name }}</span>
                  <span v-else class="text-muted-foreground/30">—</span>
                </td>
                <td class="py-3">
                  <span class="text-sm text-foreground/80" :title="row.event_type">{{ eventLabel(row.event_type) }}</span>
                </td>
                <td class="py-3">
                  <span v-if="row.program_name" class="text-sm text-foreground/80" :title="row.program_name">{{ row.program_name }}</span>
                  <span v-else class="text-muted-foreground/30">—</span>
                </td>
                <td class="pr-6 py-3 max-w-[16rem]">
                  <span class="cell-meta line-clamp-1" :title="deliveryDetail(row)">{{ deliveryDetail(row) }}</span>
                </td>
              </tr>
              <tr v-if="expandedDeliveryId === row.id" class="!cursor-default hover:!bg-transparent">
                <td colspan="6" class="!px-6 !py-3 bg-muted/30 border-b border-border/40">
                  <dl class="grid grid-cols-1 sm:grid-cols-2 gap-x-6 gap-y-1.5 text-xs">
                    <div v-if="row.event_id" class="flex gap-2 min-w-0">
                      <dt class="shrink-0 text-muted-foreground/60 w-24">Event ID</dt>
                      <dd class="font-mono text-muted-foreground break-all">{{ row.event_id }}</dd>
                    </div>
                    <div v-if="row.latency_ms != null" class="flex gap-2 min-w-0">
                      <dt class="shrink-0 text-muted-foreground/60 w-24">Latency</dt>
                      <dd class="font-mono text-muted-foreground tabular-nums">{{ row.latency_ms }} ms</dd>
                    </div>
                    <div v-if="row.http_status != null" class="flex gap-2 min-w-0">
                      <dt class="shrink-0 text-muted-foreground/60 w-24">HTTP</dt>
                      <dd class="font-mono text-muted-foreground">{{ row.http_status }}</dd>
                    </div>
                    <div v-if="row.channel_id" class="flex gap-2 min-w-0">
                      <dt class="shrink-0 text-muted-foreground/60 w-24">Channel ID</dt>
                      <dd class="font-mono text-muted-foreground break-all">{{ row.channel_id }}</dd>
                    </div>
                    <div v-if="row.program_id" class="flex gap-2 min-w-0">
                      <dt class="shrink-0 text-muted-foreground/60 w-24">Program ID</dt>
                      <dd class="font-mono text-muted-foreground break-all">{{ row.program_id }}</dd>
                    </div>
                    <div v-if="row.detail" class="flex gap-2 min-w-0 sm:col-span-2">
                      <dt class="shrink-0 text-muted-foreground/60 w-24">Detail</dt>
                      <dd class="text-muted-foreground break-words">{{ row.detail }}</dd>
                    </div>
                    <div v-if="row.error" class="flex gap-2 min-w-0 sm:col-span-2">
                      <dt class="shrink-0 text-muted-foreground/60 w-24">Error</dt>
                      <dd class="text-destructive whitespace-pre-wrap break-words">{{ row.error }}</dd>
                    </div>
                  </dl>
                </td>
              </tr>
            </template>
          </tbody>
        </table>
      </div>

      <div v-if="deliveries.length" class="flex items-center justify-between gap-3 px-1 py-3 text-xs text-muted-foreground">
        <span>Page {{ deliveryPage }} · {{ deliveries.length }} row(s)</span>
        <div class="flex items-center gap-2">
          <select
            :value="deliveryPageSize"
            class="field-control !h-7 !text-xs !w-auto"
            @change="onPageSizeChange"
          >
            <option :value="20">20</option>
            <option :value="50">50</option>
          </select>
          <button
            type="button"
            class="h-7 px-2.5 rounded-md border border-border text-xs disabled:opacity-30 hover:bg-muted"
            :disabled="deliveryPage <= 1 || deliveryLoading"
            @click="deliveryPage -= 1; fetchDeliveries()"
          >Prev</button>
          <button
            type="button"
            class="h-7 px-2.5 rounded-md border border-border text-xs disabled:opacity-30 hover:bg-muted"
            :disabled="deliveries.length < deliveryPageSize || deliveryLoading"
            @click="deliveryPage += 1; fetchDeliveries()"
          >Next</button>
        </div>
      </div>
    </div>
  </div>
</template>
