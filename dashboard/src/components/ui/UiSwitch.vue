<script setup lang="ts">
import { SwitchRoot, SwitchThumb } from 'reka-ui'
import { computed, type HTMLAttributes } from 'vue'
import { cn } from '@/lib/utils'

/**
 * shadcn-style switch backed by reka-ui primitives. Public API unchanged
 * (`v-model`, `disabled`, `ariaLabel`, `class`); reka-ui adds keyboard
 * toggling (Space) and the real role="switch" aria contract.
 */
const props = withDefaults(
  defineProps<{
    modelValue?: boolean
    disabled?: boolean
    class?: HTMLAttributes['class']
    ariaLabel?: string
  }>(),
  {
    modelValue: false,
    disabled: false,
  },
)

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
}>()

const checked = computed({
  get: () => props.modelValue,
  set: (v: boolean) => emit('update:modelValue', v),
})
</script>

<template>
  <SwitchRoot
    v-model="checked"
    :disabled="disabled"
    :aria-label="ariaLabel"
    :class="
      cn(
        'relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors',
        'focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
        'disabled:cursor-not-allowed disabled:opacity-40',
        checked ? 'bg-primary' : 'bg-muted',
        props.class,
      )
    "
  >
    <SwitchThumb
      :class="
        cn(
          'pointer-events-none inline-block h-3.5 w-3.5 rounded-full bg-background shadow-sm transition-transform',
          checked ? 'translate-x-[1.125rem]' : 'translate-x-1',
        )
      "
    />
  </SwitchRoot>
</template>
