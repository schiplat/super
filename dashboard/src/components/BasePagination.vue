<script setup lang="ts">
import { computed } from 'vue';
import { ChevronLeft, ChevronRight } from 'lucide-vue-next';

const props = withDefaults(defineProps<{
  total: number;
  page?: number;
  pageSize?: number;
  pageSizeOptions?: number[];
  disabled?: boolean;
}>(), {
  page: 1,
  pageSize: 10,
  pageSizeOptions: () => [10, 20, 50, 100],
  disabled: false,
});

const emit = defineEmits<{
  (e: 'update:page', value: number): void;
  (e: 'update:pageSize', value: number): void;
}>();

const displayOptions = computed(() => {
  const opts = new Set(props.pageSizeOptions);
  if (props.pageSize) opts.add(props.pageSize);
  return Array.from(opts).sort((a, b) => a - b);
});

const totalPages = computed(() => Math.ceil(props.total / props.pageSize) || 1);
const start = computed(() => (props.total === 0 ? 0 : (props.page - 1) * props.pageSize + 1));
const end = computed(() => Math.min(start.value + props.pageSize - 1, props.total));

type PageItem = number | 'ellipsis';

/**
 * Compact page list: show every page when few; otherwise keep first/last
 * and a small window around the current page, collapsing the middle with ….
 *
 * Examples (20 pages):
 *   p1  → 1 2 3 4 … 20
 *   p9  → 1 … 8 9 10 … 20
 *   p20 → 1 … 17 18 19 20
 */
const pageItems = computed((): PageItem[] => {
  const total = totalPages.value;
  const current = Math.min(Math.max(props.page, 1), total);

  // Small sets: no ellipsis.
  if (total <= 5) {
    return Array.from({ length: total }, (_, i) => i + 1);
  }

  // Near the start: 1 2 3 4 … N
  if (current <= 3) {
    return [1, 2, 3, 4, 'ellipsis', total];
  }
  // Near the end: 1 … N-3 N-2 N-1 N
  if (current >= total - 2) {
    return [1, 'ellipsis', total - 3, total - 2, total - 1, total];
  }
  // Middle: 1 … c-1 c c+1 … N
  return [1, 'ellipsis', current - 1, current, current + 1, 'ellipsis', total];
});

function changePage(newPage: number) {
  if (newPage >= 1 && newPage <= totalPages.value && !props.disabled) {
    emit('update:page', newPage);
  }
}
</script>

<template>
  <div class="flex flex-col sm:flex-row items-center justify-between px-4 sm:px-6 py-4 border-t border-border bg-muted/30 text-sm gap-4">
    <div class="text-muted-foreground font-medium order-2 sm:order-1 text-center sm:text-left w-full sm:w-auto">
      Showing <span class="text-foreground font-bold">{{ start }}-{{ end }}</span> of <span class="text-foreground font-bold">{{ total }}</span>
    </div>
    <div class="flex items-center justify-between sm:justify-end w-full sm:w-auto gap-3 sm:gap-4 order-1 sm:order-2 flex-wrap">
      <div class="flex items-center gap-2">
        <span class="text-muted-foreground whitespace-nowrap text-xs">Rows:</span>
        <select
          :value="pageSize"
          @change="e => { emit('update:pageSize', Number((e.target as HTMLSelectElement).value)); emit('update:page', 1); }"
          :disabled="disabled"
          class="field-control !h-8 !w-16 !text-xs !px-2"
        >
          <option v-for="opt in displayOptions" :key="opt" :value="opt">{{ opt }}</option>
        </select>
      </div>
      <nav class="flex items-center gap-0.5" aria-label="Pagination">
        <button
          type="button"
          class="inline-flex h-8 w-8 items-center justify-center rounded-lg border border-border bg-card hover:bg-muted transition-colors disabled:opacity-30"
          :disabled="page === 1 || disabled"
          aria-label="Previous page"
          @click="changePage(page - 1)"
        >
          <ChevronLeft class="w-4 h-4 text-muted-foreground" />
        </button>

        <template v-for="(item, idx) in pageItems" :key="`${item}-${idx}`">
          <span
            v-if="item === 'ellipsis'"
            class="inline-flex h-8 min-w-7 items-center justify-center px-1 text-muted-foreground/60 select-none text-xs tracking-widest"
            aria-hidden="true"
          >…</span>
          <button
            v-else
            type="button"
            class="inline-flex h-8 min-w-8 items-center justify-center rounded-lg border text-xs font-semibold tabular-nums transition-colors disabled:opacity-30 px-1.5"
            :class="item === page
              ? 'border-primary/40 bg-primary/10 text-primary'
              : 'border-border bg-card text-muted-foreground hover:bg-muted hover:text-foreground'"
            :disabled="disabled"
            :aria-label="`Page ${item}`"
            :aria-current="item === page ? 'page' : undefined"
            @click="changePage(item)"
          >
            {{ item }}
          </button>
        </template>

        <button
          type="button"
          class="inline-flex h-8 w-8 items-center justify-center rounded-lg border border-border bg-card hover:bg-muted transition-colors disabled:opacity-30"
          :disabled="page === totalPages || disabled"
          aria-label="Next page"
          @click="changePage(page + 1)"
        >
          <ChevronRight class="w-4 h-4 text-muted-foreground" />
        </button>
      </nav>
    </div>
  </div>
</template>
