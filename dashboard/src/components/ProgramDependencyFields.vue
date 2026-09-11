<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { ExternalLink, Link2 } from 'lucide-vue-next';
import TomSelect from 'tom-select';
import 'tom-select/dist/css/tom-select.css';
import { useProgramStore } from '@/stores/program';

const props = defineProps<{
  modelValue: { value: string }[];
  /** Current program name (edit) — excluded from suggestions */
  excludeName?: string;
  docHref?: string;
}>();

const emit = defineEmits<{
  'update:modelValue': [value: { value: string }[]];
}>();

const store = useProgramStore();
const selectEl = ref<HTMLSelectElement | null>(null);
let ts: TomSelect | null = null;
let syncing = false;

const selected = computed(() =>
  props.modelValue.map((d) => d.value.trim()).filter(Boolean),
);

const allNames = computed(() => {
  const names = store.programs.map((p) => p.name).filter(Boolean);
  const ex = props.excludeName?.trim();
  return [...new Set(names)].filter((n) => n !== ex).sort((a, b) => a.localeCompare(b));
});

function emitValues(values: string[]) {
  const cleaned = [...new Set(values.map((v) => v.trim()).filter(Boolean))];
  emit(
    'update:modelValue',
    cleaned.map((value) => ({ value })),
  );
}

function optionPool(): string[] {
  // Keep already-selected names in the pool so stale deps remain visible until removed.
  return [...new Set([...allNames.value, ...selected.value])];
}

function rebuildOptions() {
  if (!ts) return;
  syncing = true;
  const current = ts.getValue() as string[];
  const asArray = Array.isArray(current) ? current : current ? [current] : [];
  ts.clearOptions();
  for (const name of optionPool()) {
    ts.addOption({ value: name, text: name });
  }
  ts.refreshOptions(false);
  ts.setValue(asArray, true);
  syncing = false;
}

function syncFromModel() {
  if (!ts) return;
  const want = selected.value;
  const haveRaw = ts.getValue();
  const have = Array.isArray(haveRaw) ? haveRaw : haveRaw ? [haveRaw] : [];
  if (want.length === have.length && want.every((v, i) => v === have[i])) return;
  syncing = true;
  for (const name of want) {
    if (!ts.options[name]) ts.addOption({ value: name, text: name });
  }
  ts.setValue(want, true);
  syncing = false;
}

onMounted(() => {
  if (!store.programs.length) void store.fetchPrograms();
  if (!selectEl.value) return;

  ts = new TomSelect(selectEl.value, {
    plugins: ['remove_button'],
    maxItems: null,
    create: false,
    persist: false,
    hideSelected: true,
    closeAfterSelect: false,
    placeholder: allNames.value.length
      ? 'Select existing programs…'
      : 'No other programs yet',
    render: {
      no_results: () => `<div class="no-results">No matching programs</div>`,
    },
    onChange(value: string | string[]) {
      if (syncing) return;
      const values = Array.isArray(value) ? value : value ? [value] : [];
      emitValues(values);
    },
  });

  rebuildOptions();
  syncFromModel();
});

watch(allNames, () => {
  rebuildOptions();
  if (ts) {
    ts.settings.placeholder = allNames.value.length
      ? 'Select existing programs…'
      : 'No other programs yet';
  }
});
watch(selected, () => syncFromModel());

onBeforeUnmount(() => {
  ts?.destroy();
  ts = null;
});
</script>

<template>
  <div class="surface-card border border-border shadow-sm">
    <div class="p-5 md:p-6">
      <h3 class="text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground mb-1.5 flex items-center gap-2">
        <Link2 class="w-3.5 h-3.5" /> Dependencies
        <a
          v-if="docHref"
          :href="docHref"
          target="_blank"
          class="inline-flex text-foreground/25 hover:text-primary transition-colors"
          title="Docs: Orchestration"
        ><ExternalLink class="w-3 h-3" /></a>
      </h3>
      <p class="text-xs text-muted-foreground mb-4">
        Programs that must be <span class="font-medium text-foreground/70">Healthy</span> before this one starts.
        Only existing programs can be selected — create dependencies first.
      </p>

      <div class="deps-tom-select">
        <select ref="selectEl" multiple autocomplete="off" />
      </div>
    </div>
  </div>
</template>

<style>
/* Match dashboard field-control / chip look (Tom Select default skin). */
.deps-tom-select .ts-wrapper {
  width: 100%;
}

.deps-tom-select .ts-wrapper.multi .ts-control {
  min-height: 2.5rem;
  padding: 0.35rem 0.65rem;
  gap: 0.35rem;
  border-radius: 0.75rem;
  border-color: hsl(var(--border));
  background-color: hsl(var(--card));
  box-shadow: inset 0 1px 1px rgba(28, 25, 23, 0.04);
  font-size: 0.875rem;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  color: hsl(var(--foreground));
}

.deps-tom-select .ts-wrapper.focus .ts-control {
  border-color: hsl(var(--foreground) / 0.35);
  box-shadow: 0 0 0 2px hsl(var(--ring) / 0.3);
}

.deps-tom-select .ts-wrapper.multi .ts-control > div {
  margin: 0;
  padding: 0.15rem 0.45rem;
  border-radius: 0.5rem;
  background: hsl(var(--muted));
  color: hsl(var(--foreground) / 0.8);
  border: 0;
}

.deps-tom-select .ts-wrapper.multi .ts-control > div.active {
  background: hsl(var(--primary) / 0.12);
  color: hsl(var(--primary));
}

.deps-tom-select .ts-wrapper .ts-control > input {
  font-size: 0.875rem !important;
  color: hsl(var(--foreground)) !important;
}

.deps-tom-select .ts-wrapper .ts-control > input::placeholder {
  color: hsl(var(--muted-foreground));
}

.deps-tom-select .ts-dropdown {
  border-radius: 0.75rem;
  border-color: hsl(var(--border));
  background: hsl(var(--card));
  color: hsl(var(--foreground));
  box-shadow: 0 10px 30px rgba(15, 23, 42, 0.08);
  margin-top: 0.25rem;
  z-index: 60;
}

.deps-tom-select .ts-dropdown .option,
.deps-tom-select .ts-dropdown .create,
.deps-tom-select .ts-dropdown .no-results {
  padding: 0.5rem 0.75rem;
  font-size: 0.8125rem;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
}

.deps-tom-select .ts-dropdown .active {
  background: hsl(var(--accent));
  color: hsl(var(--accent-foreground));
}

.deps-tom-select .ts-dropdown .create {
  color: hsl(var(--primary));
}

.deps-tom-select .ts-wrapper.plugin-remove_button .item .remove {
  border-left-color: hsl(var(--border));
  color: hsl(var(--muted-foreground));
}

.deps-tom-select .ts-wrapper.plugin-remove_button .item .remove:hover {
  background: hsl(var(--destructive) / 0.08);
  color: hsl(var(--destructive));
}
</style>
