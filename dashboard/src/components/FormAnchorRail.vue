<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'

export interface RailSection {
  key: string
  label: string
  required?: boolean
}

const props = defineProps<{
  sections: RailSection[]
  done: Record<string, boolean>
  title: string
  subtitle: string
}>()

const emit = defineEmits<{ navigate: [key: string]; back: [] }>()

/**
 * The rail owns the highlight machinery for both form pages:
 * - scroll-spy via a scroll listener plus a 250 ms position poll (some
 *   embedded webviews never dispatch scroll events for programmatic scrolling)
 * - hover-follow: the pointer over the form is the attention anchor, so the
 *   highlight follows the section under it (recomputed on every scroll too)
 */
const scrollActive = ref(props.sections[0]?.key ?? '')
const hoverSection = ref<string | null>(null)
/** Hover wins over the scroll position. */
const activeSection = computed(() => hoverSection.value ?? scrollActive.value)

let sectionScroller: HTMLElement | null = null
let cursorPoint: { x: number; y: number } | null = null
let hoverFrame = 0
let pollFrame = 0
let lastKnownTop = -1

function sectionAtPoint(x: number, y: number): string | null {
  const el = document.elementFromPoint(x, y)
  const host = el?.closest('[data-section]')
  return (host?.getAttribute('data-section') as string | undefined) ?? null
}

function sectionUnderFold(): string {
  const scroller = sectionScroller
  if (!scroller) return props.sections[0]?.key ?? ''
  const line = scroller.getBoundingClientRect().top + 120
  let current = props.sections[0]?.key ?? ''
  for (const section of props.sections) {
    const el = document.getElementById(`section-${section.key}`)
    if (el && el.getBoundingClientRect().top <= line) current = section.key
  }
  return current
}

function refreshHighlight() {
  scrollActive.value = sectionUnderFold()
  hoverSection.value = cursorPoint
    ? sectionAtPoint(cursorPoint.x, cursorPoint.y)
    : null
}

function onScrollSpy() {
  const scroller = sectionScroller
  if (!scroller || scroller.scrollTop === lastKnownTop) return
  lastKnownTop = scroller.scrollTop
  refreshHighlight()
}

function onRailHover(event: MouseEvent) {
  cursorPoint = { x: event.clientX, y: event.clientY }
  // Trailing-edge throttle: recomputes at most every 50 ms. setTimeout, not
  // rAF — some embedded webviews never fire animation frames.
  if (!hoverFrame) {
    hoverFrame = window.setTimeout(() => {
      hoverFrame = 0
      refreshHighlight()
    }, 50)
  }
}

function onRailLeave() {
  cursorPoint = null
  hoverSection.value = null
}

function scrollToSection(key: string) {
  const el = document.getElementById(`section-${key}`)
  if (!el) return
  const scroller = el.closest('main')
  if (!scroller) {
    el.scrollIntoView({ block: 'start' })
    return
  }
  const target = Math.max(
    0,
    Math.min(
      el.getBoundingClientRect().top -
        scroller.getBoundingClientRect().top +
        scroller.scrollTop -
        16,
      scroller.scrollHeight - scroller.clientHeight,
    ),
  )
  // Instant jump: some embedded browsers silently drop programmatic smooth
  // scrolls on this container, and navigation must not depend on the animation.
  scroller.scrollTo({ top: target, behavior: 'instant' })
  lastKnownTop = target
  scrollActive.value = key
  emit('navigate', key)
}

onMounted(() => {
  const scroller = document.querySelector('main')
  if (!scroller) return
  sectionScroller = scroller
  lastKnownTop = scroller.scrollTop
  scroller.addEventListener('scroll', onScrollSpy, { passive: true })
  scroller.addEventListener('mousemove', onRailHover, { passive: true })
  scroller.addEventListener('mouseleave', onRailLeave)
  pollFrame = window.setInterval(onScrollSpy, 250)
  onScrollSpy()
})

onUnmounted(() => {
  if (sectionScroller) {
    sectionScroller.removeEventListener('scroll', onScrollSpy)
    sectionScroller.removeEventListener('mousemove', onRailHover)
    sectionScroller.removeEventListener('mouseleave', onRailLeave)
  }
  clearTimeout(hoverFrame)
  clearInterval(pollFrame)
})

defineExpose({ scrollToSection })
</script>

<template>
  <nav class="hidden lg:block" aria-label="Form sections">
    <div class="sticky top-6 py-1">
      <div class="mb-4">
        <div class="flex items-center gap-2">
          <button
            type="button"
            class="inline-flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground"
            title="Back"
            @click="emit('back')"
          >
            <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6" /></svg>
          </button>
          <h1 class="text-sm font-semibold tracking-tight">{{ title }}</h1>
        </div>
        <p class="mt-2 text-xs leading-snug text-muted-foreground">{{ subtitle }}</p>
      </div>
      <div class="space-y-1">
        <button
          v-for="section in sections"
          :key="section.key"
          type="button"
          class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
          :class="activeSection === section.key
            ? 'bg-muted text-foreground'
            : 'text-muted-foreground hover:bg-muted/40 hover:text-foreground'"
          @click="scrollToSection(section.key)"
        >
          <span
            class="h-1.5 w-1.5 shrink-0 rounded-full"
            :class="done[section.key]
              ? 'bg-success'
              : section.required
                ? 'bg-destructive'
                : 'bg-muted-foreground/30'"
            aria-hidden="true"
          />
          <span class="truncate">{{ section.label }}</span>
        </button>
      </div>
      <p class="mt-3 px-2.5 text-[11px] leading-snug text-muted-foreground">
        <span class="inline-block h-1.5 w-1.5 rounded-full bg-success align-middle" />
        configured ·
        <span class="inline-block h-1.5 w-1.5 rounded-full bg-muted-foreground/30 align-middle" />
        defaults
      </p>
    </div>
  </nav>
</template>
