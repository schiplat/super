<script setup lang="ts">
import { X } from 'lucide-vue-next'
import {
  DialogContent,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from 'reka-ui'
import { computed } from 'vue'
import { cn } from '@/lib/utils'

/**
 * shadcn-style dialog backed by reka-ui primitives. Public API unchanged
 * (`v-model:open`, `title`, `description`, `size`, default + `description`
 * slots) — reka-ui adds focus trapping, initial focus, aria wiring, and
 * scroll locking on top of the previous hand-written version.
 */
const open = defineModel<boolean>('open', { required: true })

const props = withDefaults(
  defineProps<{
    title: string
    description?: string
    /** Panel width. Default `md` (~28rem); `lg` for denser content like apply diffs. */
    size?: 'md' | 'lg'
  }>(),
  { size: 'md' },
)

const widthClass = computed(() =>
  props.size === 'lg' ? 'max-w-lg' : 'max-w-md',
)
</script>

<template>
  <DialogRoot v-model:open="open">
    <DialogPortal>
      <DialogOverlay
        class="fixed inset-0 z-[200] bg-foreground/25 backdrop-blur-[2px]"
      />
      <DialogContent
        :class="
          cn(
            'fixed left-1/2 top-1/2 z-[200] flex max-h-[min(90vh,640px)] w-full -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl bg-card shadow-[0_24px_64px_rgba(28,25,23,0.18)] focus:outline-hidden',
            widthClass,
          )
        "
        :aria-describedby="undefined"
      >
        <div class="flex items-start justify-between gap-4 px-5 pb-2 pt-5">
          <div class="min-w-0 space-y-1">
            <DialogTitle class="text-base font-semibold tracking-tight">{{ title }}</DialogTitle>
            <p
              v-if="$slots.description || props.description"
              class="text-sm leading-relaxed text-muted-foreground"
            >
              <slot name="description">{{ props.description }}</slot>
            </p>
          </div>
          <button
            type="button"
            class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-xl text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            aria-label="Close"
            @click="open = false"
          >
            <X class="h-4 w-4" />
          </button>
        </div>
        <div class="overflow-y-auto px-5 pb-5 pt-3">
          <slot />
        </div>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>
