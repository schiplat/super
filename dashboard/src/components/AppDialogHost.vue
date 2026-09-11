<script setup lang="ts">
import { computed } from 'vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiButton from '@/components/ui/UiButton.vue'
import { appDialogState, settleAppDialog } from '@/lib/app-dialog'

const open = computed({
  get: () => appDialogState.open,
  set: (v: boolean) => {
    if (!v) settleAppDialog(false)
  },
})
</script>

<template>
  <UiDialog v-model:open="open" :title="appDialogState.title">
    <p class="text-sm leading-relaxed text-muted-foreground whitespace-pre-wrap">
      {{ appDialogState.message }}
    </p>
    <div class="mt-5 flex justify-end gap-2">
      <UiButton
        v-if="appDialogState.kind === 'confirm'"
        variant="secondary"
        @click="settleAppDialog(false)"
      >
        {{ appDialogState.cancelLabel }}
      </UiButton>
      <UiButton
        :variant="appDialogState.variant"
        @click="settleAppDialog(true)"
      >
        {{ appDialogState.confirmLabel }}
      </UiButton>
    </div>
  </UiDialog>
</template>
