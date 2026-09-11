<script setup lang="ts">
import { onMounted, onUnmounted, ref, computed, watch, nextTick } from 'vue';
import { useRouter, useRoute } from 'vue-router';
import { useProgramStore } from '@/stores/program';
import { useSettingsStore } from '@/stores/settings';
import { useAuthStore } from '@/stores/auth';
import { useClipboard, useWindowSize } from '@vueuse/core';
import StatusBadge from '@/components/StatusBadge.vue';
import ActionButtons from '@/components/ActionButtons.vue';
import BasePagination from '@/components/BasePagination.vue';
import OverviewHeader from '@/components/OverviewHeader.vue';
import TopologyGraph from '@/components/TopologyGraph.vue';
import EmptyProgramsState from '@/components/EmptyProgramsState.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiBadge from '@/components/ui/UiBadge.vue';
import ProcessDetailDrawer from '@/components/ProcessDetailDrawer.vue';
import {
  RefreshCw, AlertCircle, Box, Search,
  Folder, AlertOctagon,
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
const showDrawer = ref(false);
const selectedProcessId = ref<string | null>(null);

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

watch(() => route.query.detail, (id) => { if (id && typeof id === 'string') { openDetails(id); router.replace({ query: {} }); } }, { immediate: true });

function openDetails(id: string) { selectedProcessId.value = id; showDrawer.value = true; }
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
  <div class="flex flex-col gap-3 sm:gap-4">

    <!-- Error Banner -->
    <div v-if="store.error" class="rounded-xl border border-destructive/30 bg-destructive/5 flex items-center gap-3 px-4 py-3 shrink-0 text-sm">
      <AlertCircle class="w-5 h-5 text-destructive" />
      <span class="font-medium text-foreground flex-1">{{ store.error }}</span>
      <UiButton variant="ghost" size="sm" @click="store.fetchPrograms()">Retry</UiButton>
    </div>

    <EmptyProgramsState v-if="!store.isLoading && store.programs.length === 0" :can-create="authStore.canOperate" premium />

    <div v-else class="surface-card shadow-sm overflow-visible">
      <OverviewHeader :stats="stats" :active-filter="activeFilter" class="border-b border-border/70 mx-3 sm:mx-4 mt-3 mb-0" @filter="setFilter" />

      <!-- Toolbar -->
      <div class="px-4 py-2.5 border-b border-border flex flex-col xl:flex-row justify-between items-center gap-2 sm:gap-3 shrink-0">
        <div class="flex items-center gap-3 w-full xl:w-auto">
          <h2 class="font-semibold text-foreground whitespace-nowrap text-sm">{{ activeFilter === 'ALL' ? 'All Processes' : `${activeFilter} Processes` }}</h2>
          <UiBadge size="sm">{{ filteredPrograms.length }}</UiBadge>
          <UiBadge v-if="selectedGroup" variant="default" @click="selectedGroup = ''" class="cursor-pointer">Group: {{ selectedGroup }} &times;</UiBadge>
        </div>

        <div class="flex flex-col sm:flex-row items-center gap-3 w-full xl:w-auto">
          <div class="bg-muted p-1 rounded-lg flex items-center gap-1 border border-border/40 hidden sm:flex">
            <button class="flex items-center justify-center w-9 h-7 rounded-md transition-all duration-200" :class="viewMode === 'list' ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'" @click="viewMode = 'list'" title="List View"><LayoutList class="w-4 h-4" /></button>
            <button class="flex items-center justify-center w-9 h-7 rounded-md transition-all duration-200" :class="viewMode === 'graph' ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'" @click="viewMode = 'graph'" title="Topology View"><Network class="w-4 h-4" /></button>
          </div>
          <div class="w-px h-6 bg-border hidden sm:block"></div>

          <router-link v-if="authStore.canOperate" to="/programs/new">
            <UiButton size="sm" class="gap-2 shadow-sm whitespace-nowrap">
              <Plus class="w-4 h-4" />
              <span class="hidden md:inline">New Program</span><span class="md:hidden">New</span>
            </UiButton>
          </router-link>

          <div class="relative w-full sm:w-48">
            <select v-model="selectedGroup" class="field-control !h-8 !text-xs !rounded-lg">
              <option value="">All Groups</option>
              <option v-for="group in uniqueGroups" :key="group" :value="group">{{ group }}</option>
            </select>
          </div>
          <div class="relative w-full sm:w-64">
            <Search class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground/40" />
            <input v-model="searchQuery" type="text" placeholder="Search..." class="field-control !h-8 !pl-9 !text-xs !rounded-lg" />
          </div>
          <UiButton variant="outline" size="sm" @click="store.fetchPrograms()" :disabled="store.isLoading">
            <RefreshCw class="w-3.5 h-3.5" :class="{ 'animate-spin': store.isLoading }" />
            <span class="hidden sm:inline">Refresh</span>
          </UiButton>
        </div>
      </div>

      <!-- List View -->
      <div v-if="viewMode === 'list'" class="flex flex-col">
        <div class="data-table-scroll">
          <table class="data-table">
            <thead>
              <tr>
                <th class="pl-6 cursor-pointer select-none" @click="toggleSort('name')">
                  <span class="inline-flex items-center gap-1.5">
                    Name / ID
                    <ArrowUp v-if="sortField === 'name' && sortDir === 'asc'" class="w-3 h-3" />
                    <ArrowDown v-else-if="sortField === 'name' && sortDir === 'desc'" class="w-3 h-3" />
                  </span>
                </th>
                <th class="cursor-pointer select-none" @click="toggleSort('group')"><span class="inline-flex items-center gap-1.5">Group<ArrowUp v-if="sortField === 'group' && sortDir === 'asc'" class="w-3 h-3" /><ArrowDown v-else-if="sortField === 'group' && sortDir === 'desc'" class="w-3 h-3" /></span></th>
                <th class="cursor-pointer select-none" @click="toggleSort('status')"><span class="inline-flex items-center gap-1.5">Status<ArrowUp v-if="sortField === 'status' && sortDir === 'asc'" class="w-3 h-3" /><ArrowDown v-else-if="sortField === 'status' && sortDir === 'desc'" class="w-3 h-3" /></span></th>
                <th class="cursor-pointer select-none" @click="toggleSort('pid')"><span class="inline-flex items-center gap-1.5">PID<ArrowUp v-if="sortField === 'pid' && sortDir === 'asc'" class="w-3 h-3" /><ArrowDown v-else-if="sortField === 'pid' && sortDir === 'desc'" class="w-3 h-3" /></span></th>
                <th class="cursor-pointer select-none" @click="toggleSort('uptime')"><span class="inline-flex items-center gap-1.5">Uptime<ArrowUp v-if="sortField === 'uptime' && sortDir === 'asc'" class="w-3 h-3" /><ArrowDown v-else-if="sortField === 'uptime' && sortDir === 'desc'" class="w-3 h-3" /></span></th>
                <th v-if="hasAnyResourceLimits">Limits</th>
                <th class="cursor-pointer select-none" @click="toggleSort('updated')"><span class="inline-flex items-center gap-1.5">Updated<ArrowUp v-if="sortField === 'updated' && sortDir === 'asc'" class="w-3 h-3" /><ArrowDown v-else-if="sortField === 'updated' && sortDir === 'desc'" class="w-3 h-3" /></span></th>
                <th class="pr-6 text-center w-[240px]">Actions</th>
              </tr>
            </thead>
            <tbody>
              <tr v-if="store.isLoading && store.programs.length === 0">
                <td :colspan="hasAnyResourceLimits ? 8 : 7" class="py-16 text-center text-muted-foreground"><span class="inline-block w-6 h-6 border-2 border-muted-foreground/30 border-t-foreground/50 rounded-full animate-spin"></span></td>
              </tr>
              <tr v-else-if="filteredPrograms.length === 0">
                <td :colspan="hasAnyResourceLimits ? 8 : 7" class="py-16 text-center">
                  <div class="flex flex-col items-center gap-3 text-muted-foreground/40"><Box class="w-10 h-10" /><p class="font-medium">No processes match your filters</p><p class="text-xs" v-if="hasActiveFilters">Try clearing filters or search.</p></div>
                </td>
              </tr>
              <tr v-else v-for="proc in paginatedPrograms" :key="proc.id" class="clickable">
                <td class="pl-6 py-3 group/name">
                  <div class="flex flex-col">
                    <span class="inline-flex items-center gap-1.5 font-semibold text-foreground/90 cursor-pointer" @click="openDetails(proc.id)">
                      {{ proc.name }}
                      <Info class="w-3 h-3 text-muted-foreground/25 group-hover/name:text-primary transition-colors shrink-0" title="Click to view details" />
                    </span>
                    <span class="inline-flex items-center gap-0.5 mt-0.5">
                      <span class="font-mono text-xs text-muted-foreground select-all" :title="proc.id">{{ proc.id.slice(0, 8) }}</span>
                      <button
                        type="button"
                        class="inline-flex h-5 w-5 items-center justify-center rounded text-muted-foreground/40 hover:text-foreground hover:bg-muted transition-colors"
                        :title="copiedId === proc.id ? 'Copied' : 'Copy full id'"
                        @click="copyProgramId(proc.id, $event)"
                      >
                        <Check v-if="copiedId === proc.id" class="w-3 h-3 text-success" />
                        <Copy v-else class="w-3 h-3" />
                      </button>
                    </span>
                  </div>
                </td>
                <td class="py-3">
                  <span v-if="proc.group" class="inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium bg-muted text-muted-foreground hover:bg-accent hover:text-accent-foreground cursor-pointer transition-colors" :class="{ 'bg-primary text-primary-foreground hover:bg-primary hover:text-primary-foreground': selectedGroup === proc.group }" @click="toggleGroupFilter(proc.group)" title="Filter by this group">{{ proc.group }}</span>
                  <span v-else class="text-muted-foreground/20">-</span>
                </td>
                <td class="py-3">
                  <div class="flex items-center gap-2">
                    <StatusBadge :status="proc.status" :health-error="proc.health_error" />
                    <div v-if="proc.last_error || proc.health_error" class="group/err relative">
                      <div
                        class="cursor-help animate-pulse"
                        :class="proc.last_error ? 'text-destructive' : 'text-warning'"
                      >
                        <AlertOctagon class="w-4 h-4" />
                      </div>
                      <div class="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 w-max max-w-[260px] p-2.5 text-xs rounded-xl shadow-xl opacity-0 group-hover/err:opacity-100 transition-opacity pointer-events-none z-50 whitespace-normal break-words leading-tight text-center"
                        :class="proc.last_error ? 'bg-destructive text-destructive-foreground' : 'bg-warning text-warning-foreground'"
                      >
                        {{ proc.last_error || proc.health_error }}
                        <div
                          class="absolute top-full left-1/2 -translate-x-1/2 border-4 border-transparent"
                          :class="proc.last_error ? 'border-t-destructive' : 'border-t-warning'"
                        />
                      </div>
                    </div>
                  </div>
                </td>
                <td class="py-3 font-mono text-xs text-muted-foreground">{{ proc.pid || '-' }}</td>
                <td class="py-3 font-mono text-xs text-muted-foreground">{{ formatUptime(proc.uptime_sec) }}</td>
                <td v-if="hasAnyResourceLimits" class="py-3"><span v-if="formatResourceLimits(proc)" class="font-mono text-xs text-violet-600/80" :title="`CPU / memory cap: ${formatResourceLimits(proc)}`">{{ formatResourceLimits(proc) }}</span><span v-else class="text-muted-foreground/20">—</span></td>
                <td class="py-3 !text-[11px] leading-tight text-muted-foreground font-mono tabular-nums">{{ formatTime(proc.updated_at) }}</td>
                <td class="pr-6 py-3"><div class="flex justify-center"><ActionButtons :id="proc.id" :status="proc.status" :name="proc.name" /></div></td>
              </tr>
            </tbody>
          </table>
        </div>
        <BasePagination v-model:page="currentPage" v-model:pageSize="settingsStore.defaultPageSize" :total="filteredPrograms.length" :disabled="store.isLoading" />
      </div>

      <!-- Graph View -->
      <div v-else class="h-[420px] sm:h-[520px] lg:h-[600px] w-full bg-muted/50"><TopologyGraph ref="graphRef" :programs="filteredPrograms" @node-click="openDetails" /></div>
    </div>

    <ProcessDetailDrawer v-model="showDrawer" :process-id="selectedProcessId" @change-process="(id: string) => selectedProcessId = id" />
  </div>
</template>
