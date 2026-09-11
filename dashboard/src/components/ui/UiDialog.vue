<script setup lang="ts">
import { X } from 'lucide-vue-next'
import { onMounted, onUnmounted, watch } from 'vue'
import { cn } from '@/lib/utils'

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

function close() {
  open.value = false
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape' && open.value) {
    e.preventDefault()
    close()
  }
}

watch(open, (v) => {
  document.body.style.overflow = v ? 'hidden' : ''
})

onMounted(() => window.addEventListener('keydown', onKey))
onUnmounted(() => {
  window.removeEventListener('keydown', onKey)
  document.body.style.overflow = ''
})
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-[200] flex items-end justify-center p-4 sm:items-center"
      role="dialog"
      aria-modal="true"
      :aria-label="title"
    >
      <div class="absolute inset-0 bg-foreground/25 backdrop-blur-[2px]" @click="close()" />
      <div
        :class="
          cn(
            'relative z-10 flex max-h-[min(90vh,640px)] w-full flex-col overflow-hidden rounded-2xl bg-card shadow-[0_24px_64px_rgba(28,25,23,0.18)]',
            props.size === 'lg' ? 'max-w-lg' : 'max-w-md',
          )
        "
      >
        <div class="flex items-start justify-between gap-4 px-5 pb-2 pt-5">
          <div class="min-w-0 space-y-1">
            <h2 class="text-base font-semibold tracking-tight">{{ title }}</h2>
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
            @click="close()"
          >
            <X class="h-4 w-4" />
          </button>
        </div>
        <div class="overflow-y-auto px-5 pb-5 pt-3">
          <slot />
        </div>
      </div>
    </div>
  </Teleport>
</template>
