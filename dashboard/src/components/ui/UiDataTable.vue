<script setup lang="ts">
import { computed } from 'vue'
import UiEmptyState from '@/components/ui/UiEmptyState.vue'
import type { DataColumn } from '@/types/ui'

/**
 * List card: no padding on the card, one 20px gutter across toolbar, header,
 * rows and footer so columns line up. `rowsCount` is explicit because a
 * default slot cannot be inspected reliably to decide "empty".
 */
const props = withDefaults(
  defineProps<{
    columns: DataColumn[]
    rowsCount: number
    title?: string
    description?: string
    empty?: string
    emptyHint?: string
    loading?: boolean
    skeletonRows?: number
  }>(),
  { skeletonRows: 3 },
)

const hasHeader = computed(() => !!props.title || !!props.description)
</script>

<template>
  <section
    class="overflow-hidden rounded-2xl bg-card ring-1 ring-border/60 shadow-[0_1px_2px_hsl(220_20%_20%/0.04),0_2px_8px_hsl(220_20%_20%/0.03)]"
  >
    <header v-if="hasHeader" class="px-5 pt-5">
      <h2 v-if="title" class="text-sm font-semibold tracking-tight">{{ title }}</h2>
      <p v-if="description" class="mt-1 text-sm text-muted-foreground">{{ description }}</p>
    </header>

    <div
      v-if="$slots.toolbar"
      class="flex flex-wrap items-center justify-between gap-3 border-b border-border/40 px-5 py-3"
      :class="hasHeader ? 'mt-4' : ''"
    >
      <slot name="toolbar" />
    </div>

    <div class="overflow-x-auto">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b border-border/40">
            <th
              v-for="column in columns"
              :key="column.key"
              class="px-5 py-2.5 text-[11px] font-medium uppercase tracking-[0.04em] text-muted-foreground/80"
              :class="[
                column.align === 'right'
                  ? 'text-right'
                  : column.align === 'center'
                    ? 'text-center'
                    : 'text-left',
                column.class,
              ]"
              scope="col"
            >
              {{ column.label }}
            </th>
          </tr>
        </thead>
        <tbody v-if="loading">
          <tr
            v-for="row in skeletonRows"
            :key="row"
            class="border-b border-border/40 last:border-0"
          >
            <td :colspan="columns.length" class="px-5 py-3">
              <div class="h-4 w-full animate-pulse rounded bg-muted/40"></div>
            </td>
          </tr>
        </tbody>
        <tbody v-else-if="rowsCount === 0">
          <tr>
            <td :colspan="columns.length">
              <UiEmptyState :title="empty || 'Nothing here yet'" :hint="emptyHint" />
            </td>
          </tr>
        </tbody>
        <tbody v-else>
          <slot />
        </tbody>
      </table>
    </div>

    <div v-if="$slots.footer" class="border-t border-border/40 px-5 py-2.5">
      <slot name="footer" />
    </div>
  </section>
</template>
