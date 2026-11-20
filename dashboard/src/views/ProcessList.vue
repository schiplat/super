<script setup lang="ts">
import { onMounted, onUnmounted, ref, computed, watch, nextTick } from 'vue';
import { useRouter, useRoute } from 'vue-router';
import { useProgramStore } from '@/stores/program';
import { useSettingsStore } from '@/stores/settings';
import { useAuthStore } from '@/stores/auth';
import { useClipboard, useWindowSize } from '@vueuse/core';
import StatusBadge from '@/components/StatusBadge.vue';
import ActionButtons from '@/components/ActionButtons.vue';
import Slot from '@/slots/Slot.vue';
import BasePagination from '@/components/BasePagination.vue';
import TopologyGraph from '@/components/TopologyGraph.vue';
import EmptyProgramsState from '@/components/EmptyProgramsState.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiEmptyState from '@/components/ui/UiEmptyState.vue';
import UiNotice from '@/components/ui/UiNotice.vue';
import {
  RefreshCw, Search,
  AlertOctagon,
  LayoutList, Network, Plus,
  ArrowUp, ArrowDown, Info, Copy, Check
} from 'lucide-vue-next';
import { format } from 'date-fns';
import type { Program } from '@/types';

const { copy } = useClipboard();
const copiedId = ref<string | null>(null);
let copiedIdTimer: ReturnType<typeof setTimeout> | null = null;
async function copyProgramId(id: string, e: Event) {
  e.stopPropagation();
  await copy(id);
  copiedId.value = id;
  if (copiedIdTimer) clearTimeout(copiedIdTimer);
  copiedIdTimer = setTimeout(() => { copiedId.value = null; }, 1500);
}

const store = useProgramStore();
const settingsStore = useSettingsStore();
const authStore = useAuthStore();
const router = useRouter();
const route = useRoute();

const searchQuery = ref('');
const currentPage = ref(1);
const activeFilter = ref<string>('ALL');
const selectedGroup = ref<string>('');

type SortField = 'name' | 'group' | 'status' | 'pid' | 'uptime' | 'updated' | null;
const sortField = ref<SortField>('updated');
const sortDir = ref<'asc' | 'desc'>('desc');

function toggleSort(field: SortField) {
  if (sortField.value === field) sortDir.value = sortDir.value === 'asc' ? 'desc' : 'asc';
  else { sortField.value = field; sortDir.value = 'asc'; }
  currentPage.value = 1;
}

const viewMode = ref<'list' | 'graph'>('list');
const graphRef = ref<InstanceType<typeof TopologyGraph> | null>(null);
const { width } = useWindowSize();
let graphPollTimer: ReturnType<typeof setInterval> | null = null;

watch(width, (newWidth) => { if (newWidth < 640 && viewMode.value === 'graph') viewMode.value = 'list'; }, { immediate: true });

watch(viewMode, (mode) => {
  if (graphPollTimer) { clearInterval(graphPollTimer); graphPollTimer = null; }
  if (mode === 'graph') {
    void store.fetchPrograms();
    graphPollTimer = setInterval(() => store.fetchPrograms(), 5000);
    nextTick(() => setTimeout(() => graphRef.value?.resize(), 50));
  }
});

const uniqueGroups = computed(() => {
  const groups = new Set<string>();
  store.programs.forEach(p => { if (p.group) groups.add(p.group); });
  return Array.from(groups).sort();
});

/** Alive but health_check failing — backend keeps status `Running` + `health_error`. */
function isHealthFailing(p: Program): boolean {
  return p.status === 'Running' && !!p.health_error;
}

const stats = computed(() => ({
  total: store.programs.length,
  running: store.programs.filter(p =>
    ['Healthy', 'Starting'].includes(p.status) || (p.status === 'Running' && !p.health_error),
  ).length,
  stopped: store.programs.filter(p => ['Stopped', 'Stopping'].includes(p.status)).length,
  problem: store.programs.filter(p =>
    ['Fatal', 'Backoff'].includes(p.status) || isHealthFailing(p),
  ).length,
}));

const filterChips = [
  { key: 'ALL', label: 'All', valueKey: 'total', dot: 'bg-info' },
  { key: 'Running', label: 'Running', valueKey: 'running', dot: 'bg-success' },
  { key: 'Stopped', label: 'Stopped', valueKey: 'stopped', dot: 'bg-muted-foreground/40' },
  { key: 'Fatal', label: 'Issues', valueKey: 'problem', dot: 'bg-destructive' },
] as const;

const filteredPrograms = computed(() => {
  let result = store.programs;
  if (activeFilter.value !== 'ALL') {
    if (activeFilter.value === 'Running') {
      result = result.filter(p =>
        ['Healthy', 'Starting'].includes(p.status) || (p.status === 'Running' && !p.health_error),
      );
    } else if (activeFilter.value === 'Stopped') {
      result = result.filter(p => ['Stopped', 'Stopping'].includes(p.status));
    } else if (activeFilter.value === 'Fatal') {
      result = result.filter(p => ['Fatal', 'Backoff'].includes(p.status) || isHealthFailing(p));
    }
  }
  if (selectedGroup.value) result = result.filter(p => p.group === selectedGroup.value);
  const query = searchQuery.value.toLowerCase().trim();
  if (query) result = result.filter(p => p.name.toLowerCase().includes(query) || p.id.toLowerCase().includes(query) || (p.group && p.group.toLowerCase().includes(query)));
  return result;
});

const sortedPrograms = computed(() => {
  if (!sortField.value) return filteredPrograms.value;
  const dir = sortDir.value === 'asc' ? 1 : -1;
  return [...filteredPrograms.value].sort((a, b) => {
    let va: string | number = ''; let vb: string | number = '';
    switch (sortField.value) {
      case 'name': va = a.name; vb = b.name; break;
      case 'group': va = a.group ?? ''; vb = b.group ?? ''; break;
      case 'status': va = a.status; vb = b.status; break;
      case 'pid': va = a.pid ?? 0; vb = b.pid ?? 0; break;
      case 'uptime': va = a.uptime_sec ?? 0; vb = b.uptime_sec ?? 0; break;
      case 'updated': va = a.updated_at; vb = b.updated_at; break;
    }
    return va < vb ? -1 * dir : va > vb ? 1 * dir : 0;
  });
});

const paginatedPrograms = computed(() => {
  const start = (currentPage.value - 1) * settingsStore.defaultPageSize;
  return sortedPrograms.value.slice(start, start + settingsStore.defaultPageSize);
});

watch([searchQuery, () => settingsStore.defaultPageSize, activeFilter, selectedGroup], () => { currentPage.value = 1; });
onMounted(() => { store.fetchPrograms(); });
onUnmounted(() => { if (graphPollTimer) clearInterval(graphPollTimer); });

watch(() => route.query.detail, (id) => { if (typeof id === 'string' && id) void router.replace(`/programs/${id}`); }, { immediate: true });
// Stat cards on the Overview page link here with `?status=Running|Stopped|Fatal`.
watch(() => route.query.status, (value) => {
  if (typeof value === 'string' && ['ALL', 'Running', 'Stopped', 'Fatal'].includes(value)) {
    activeFilter.value = value;
    currentPage.value = 1;
  }
}, { immediate: true });

function openDetails(id: string) { void router.push(`/programs/${id}`); }
function setFilter(status: string) { activeFilter.value = activeFilter.value === status ? 'ALL' : status; }
function toggleGroupFilter(group: string) { selectedGroup.value = selectedGroup.value === group ? '' : group; }

function formatUptime(sec?: number) {
  if (sec === undefined || sec === null) return '-';
  if (sec < 60) return `${sec}s`;
  const min = Math.floor(sec / 60); if (min < 60) return `${min}m ${sec % 60}s`;
  const hr = Math.floor(min / 60); if (hr < 24) return `${hr}h ${min % 60}m`;
  return `${Math.floor(hr / 24)}d ${hr % 24}h`;
}

function formatTime(timestamp: number) { return timestamp ? format(new Date(timestamp * 1000), 'yyyy-MM-dd HH:mm') : '-'; }
function formatMemBytes(bytes: number): string { if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)}G`; if (bytes >= 1024 ** 2) return `${Math.round(bytes / 1024 / 1024)}M`; return `${Math.round(bytes / 1024)}K`; }

function formatResourceLimits(proc: Program): string | null {
  const rl = proc.resource_limits; if (!rl) return null;
  const parts: string[] = [];
  if (rl.cpu_quota != null && rl.cpu_quota > 0) { const q = rl.cpu_quota; parts.push(`${q % 1 === 0 ? q.toFixed(0) : q.toFixed(1)}%`); }
  if (rl.memory_limit != null && rl.memory_limit > 0) parts.push(formatMemBytes(rl.memory_limit));
  return parts.length ? parts.join(' · ') : null;
}

const hasAnyResourceLimits = computed(() => store.programs.some(p => formatResourceLimits(p) != null));
const hasActiveFilters = computed(() => activeFilter.value !== 'ALL' || !!selectedGroup.value || !!searchQuery.value.trim());
</script>

<template>
  <div class="space-y-6">
    <UiNotice v-if="store.error" tone="danger">
      <div class="flex items-center gap-3">
        <span class="flex-1">{{ store.error }}</span>
        <UiButton variant="ghost" size="sm" @click="store.fetchPrograms()">Retry</UiButton>
      </div>
    </UiNotice>

    <EmptyProgramsState
      v-if="!store.isLoading && store.programs.length === 0"
      :can-create="authStore.canOperate"
      premium
    />

    <template v-else>
      <section class="overflow-hidden rounded-2xl bg-card ring-1 ring-border/60 shadow-[0_1px_2px_hsl(220_20%_20%/0.04),0_2px_8px_hsl(220_20%_20%/0.03)]">
        <!-- Toolbar: one 20px gutter, 32px controls -->
        <div class="flex flex-wrap items-center justify-between gap-3 border-b border-border/40 px-5 py-3">
          <div class="flex min-w-0 flex-wrap items-center gap-1">
            <button
              v-for="item in filterChips"
              :key="item.key"
              type="button"
              class="inline-flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
              :class="activeFilter === item.key
                ? 'bg-muted text-foreground'
                : 'text-muted-foreground hover:bg-muted/40 hover:text-foreground'"
              @click="setFilter(item.key)"
            >
              <span class="h-1.5 w-1.5 rounded-full" :class="item.dot" aria-hidden="true" />
              <span>{{ item.label }}</span>
              <span class="tabular-nums">{{ stats[item.valueKey] }}</span>
            </button>
            <button
              v-if="selectedGroup"
              type="button"
              class="inline-flex items-center gap-1 rounded-lg bg-muted/40 px-1.5 py-0.5 text-[11px] text-muted-foreground transition-colors hover:text-foreground"
              @click="selectedGroup = ''"
            >
              Group: {{ selectedGroup }} &times;
            </button>
          </div>

          <div class="flex flex-wrap items-center gap-3">
            <div class="hidden items-center gap-1 rounded-xl bg-muted/40 p-1 sm:flex">
              <button
                type="button"
                class="inline-flex h-8 w-8 items-center justify-center rounded-lg transition-colors"
                :class="viewMode === 'list' ? 'bg-card text-foreground' : 'text-muted-foreground hover:text-foreground'"
                title="List view"
                @click="viewMode = 'list'"
              >
                <LayoutList class="h-4 w-4" />
              </button>
              <button
                type="button"
                class="inline-flex h-8 w-8 items-center justify-center rounded-lg transition-colors"
                :class="viewMode === 'graph' ? 'bg-card text-foreground' : 'text-muted-foreground hover:text-foreground'"
                title="Topology view"
                @click="viewMode = 'graph'"
              >
                <Network class="h-4 w-4" />
              </button>
            </div>

            <router-link v-if="authStore.canOperate" to="/programs/new">
              <UiButton size="sm" class="gap-2 whitespace-nowrap">
                <Plus class="h-4 w-4" />
                <span class="hidden md:inline">New Program</span><span class="md:hidden">New</span>
              </UiButton>
            </router-link>

            <select v-model="selectedGroup" class="field-control field-control-sm w-40">
              <option value="">All groups</option>
              <option v-for="group in uniqueGroups" :key="group" :value="group">{{ group }}</option>
            </select>

            <div class="relative w-full sm:w-64">
              <Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground/40" />
              <input
                v-model="searchQuery"
                type="text"
                placeholder="Search programs…"
                class="field-control field-control-sm max-w-xs pl-9"
              />
            </div>

            <UiButton variant="outline" size="sm" :disabled="store.isLoading" @click="store.fetchPrograms()">
              <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': store.isLoading }" />
              <span class="hidden sm:inline">Refresh</span>
            </UiButton>
          </div>
        </div>

        <!-- List view -->
        <div v-if="viewMode === 'list'">
          <div class="overflow-x-auto">
            <table class="data-table">
              <thead>
                <tr>
                  <th class="cursor-pointer select-none" @click="toggleSort('name')">
                    <span class="inline-flex items-center gap-1.5">
                      Name / ID
                      <ArrowUp v-if="sortField === 'name' && sortDir === 'asc'" class="h-3 w-3" />
                      <ArrowDown v-else-if="sortField === 'name' && sortDir === 'desc'" class="h-3 w-3" />
                    </span>
                  </th>
                  <th class="cursor-pointer select-none" @click="toggleSort('group')">
                    <span class="inline-flex items-center gap-1.5">Group<ArrowUp v-if="sortField === 'group' && sortDir === 'asc'" class="h-3 w-3" /><ArrowDown v-else-if="sortField === 'group' && sortDir === 'desc'" class="h-3 w-3" /></span>
                  </th>
                  <th class="cursor-pointer select-none" @click="toggleSort('status')">
                    <span class="inline-flex items-center gap-1.5">Status<ArrowUp v-if="sortField === 'status' && sortDir === 'asc'" class="h-3 w-3" /><ArrowDown v-else-if="sortField === 'status' && sortDir === 'desc'" class="h-3 w-3" /></span>
                  </th>
                  <th class="cursor-pointer select-none" @click="toggleSort('pid')">
                    <span class="inline-flex items-center gap-1.5">PID<ArrowUp v-if="sortField === 'pid' && sortDir === 'asc'" class="h-3 w-3" /><ArrowDown v-else-if="sortField === 'pid' && sortDir === 'desc'" class="h-3 w-3" /></span>
                  </th>
                  <th class="cursor-pointer select-none" @click="toggleSort('uptime')">
                    <span class="inline-flex items-center gap-1.5">Uptime<ArrowUp v-if="sortField === 'uptime' && sortDir === 'asc'" class="h-3 w-3" /><ArrowDown v-else-if="sortField === 'uptime' && sortDir === 'desc'" class="h-3 w-3" /></span>
                  </th>
                  <th v-if="hasAnyResourceLimits">Limits</th>
                  <th class="cursor-pointer select-none" @click="toggleSort('updated')">
                    <span class="inline-flex items-center gap-1.5">Updated<ArrowUp v-if="sortField === 'updated' && sortDir === 'asc'" class="h-3 w-3" /><ArrowDown v-else-if="sortField === 'updated' && sortDir === 'desc'" class="h-3 w-3" /></span>
                  </th>
                  <th class="w-[240px] text-center">Actions</th>
                </tr>
              </thead>
              <tbody>
                <template v-if="store.isLoading && store.programs.length === 0">
                  <tr v-for="row in 3" :key="row" class="border-b border-border/40 last:border-0">
                    <td :colspan="hasAnyResourceLimits ? 8 : 7">
                      <div class="h-4 w-full animate-pulse rounded bg-muted/40"></div>
                    </td>
                  </tr>
                </template>
                <template v-else-if="filteredPrograms.length === 0">
                  <tr>
                    <td :colspan="hasAnyResourceLimits ? 8 : 7">
                      <UiEmptyState
                        title="No programs match your filters"
                        :hint="hasActiveFilters ? 'Try clearing the filters or the search.' : undefined"
                      />
                    </td>
                  </tr>
                </template>
                <template v-else>
                  <tr v-for="proc in paginatedPrograms" :key="proc.id" class="clickable">
                    <td class="group/name">
                      <div class="flex flex-col">
                        <span class="inline-flex cursor-pointer items-center gap-1.5 font-semibold text-foreground/90" @click="openDetails(proc.id)">
                          {{ proc.name }}
                          <Info class="h-3 w-3 shrink-0 text-muted-foreground/25 transition-colors group-hover/name:text-primary" title="Open details" />
                        </span>
                        <span class="mt-0.5 inline-flex items-center gap-0.5">
                          <span class="font-mono text-xs text-muted-foreground select-all" :title="proc.id">{{ proc.id.slice(0, 8) }}</span>
                          <button
                            type="button"
                            class="inline-flex h-5 w-5 items-center justify-center rounded-lg text-muted-foreground/40 transition-colors hover:bg-muted/40 hover:text-foreground"
                            :title="copiedId === proc.id ? 'Copied' : 'Copy full id'"
                            @click="copyProgramId(proc.id, $event)"
                          >
                            <Check v-if="copiedId === proc.id" class="h-3 w-3 text-success" />
                            <Copy v-else class="h-3 w-3" />
                          </button>
                        </span>
                      </div>
                    </td>
                    <td>
                      <span
                        v-if="proc.group"
                        class="inline-flex cursor-pointer items-center rounded-lg bg-muted/40 px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground transition-colors hover:text-foreground"
                        :class="{ 'bg-muted text-foreground': selectedGroup === proc.group }"
                        title="Filter by this group"
                        @click="toggleGroupFilter(proc.group)"
                      >{{ proc.group }}</span>
                      <span v-else class="text-muted-foreground/20">—</span>
                    </td>
                    <td>
                      <div class="flex items-center gap-2">
                        <StatusBadge :status="proc.status" :health-error="proc.health_error" />
                        <div v-if="proc.last_error || proc.health_error" class="group/err relative">
                          <div class="animate-pulse cursor-help" :class="proc.last_error ? 'text-destructive' : 'text-warning'">
                            <AlertOctagon class="h-4 w-4" />
                          </div>
                          <div
                            class="pointer-events-none absolute bottom-full left-1/2 z-50 mb-2 w-max max-w-[260px] -translate-x-1/2 whitespace-normal break-words rounded-xl p-2.5 text-center text-xs leading-tight opacity-0 shadow-lg transition-opacity group-hover/err:opacity-100"
                            :class="proc.last_error ? 'bg-destructive text-destructive-foreground' : 'bg-warning text-warning-foreground'"
                          >
                            {{ proc.last_error || proc.health_error }}
                            <div
                              class="absolute left-1/2 top-full -translate-x-1/2 border-4 border-transparent"
                              :class="proc.last_error ? 'border-t-destructive' : 'border-t-warning'"
                            />
                          </div>
                        </div>
                      </div>
                    </td>
                    <td class="font-mono text-xs text-muted-foreground">{{ proc.pid || '—' }}</td>
                    <td class="font-mono text-xs text-muted-foreground">{{ formatUptime(proc.uptime_sec) }}</td>
                    <td v-if="hasAnyResourceLimits">
                      <span v-if="formatResourceLimits(proc)" class="font-mono text-xs text-muted-foreground" :title="`CPU / memory cap: ${formatResourceLimits(proc)}`">{{ formatResourceLimits(proc) }}</span>
                      <span v-else class="text-muted-foreground/20">—</span>
                    </td>
                    <td class="font-mono text-xs tabular-nums text-muted-foreground">{{ formatTime(proc.updated_at) }}</td>
                    <td>
                      <div class="flex items-center justify-center gap-0.5">
                        <!-- STABLE: process.actions — plugins read context.process.{id,name,status,pid}. Do not change :context shape without updating tests/slots.spec.ts + ui-bridge. -->
                        <Slot name="process.actions" :context="{ process: proc }" />
                        <ActionButtons :id="proc.id" :status="proc.status" :name="proc.name" />
                      </div>
                    </td>
                  </tr>
                </template>
              </tbody>
            </table>
          </div>

          <BasePagination
            v-model:page="currentPage"
            v-model:pageSize="settingsStore.defaultPageSize"
            :total="filteredPrograms.length"
            :disabled="store.isLoading"
          />
        </div>

        <!-- Topology view -->
        <div v-else class="h-[420px] w-full bg-muted/40 sm:h-[520px] lg:h-[600px]">
          <TopologyGraph ref="graphRef" :programs="filteredPrograms" @node-click="openDetails" />
        </div>
      </section>
    </template>
  </div>
</template>
