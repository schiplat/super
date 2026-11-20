<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import ProcessDetailDrawer from '@/components/ProcessDetailDrawer.vue'

const route = useRoute()
const router = useRouter()

const processId = computed(() => String(route.params.id ?? ''))

function back() {
  void router.push('/programs')
}

function changeProcess(id: string) {
  void router.replace(`/programs/${id}`)
}
</script>

<template>
  <div class="h-[calc(100vh-5rem)]">
    <ProcessDetailDrawer
      :model-value="true"
      :process-id="processId"
      inline
      @update:model-value="back"
      @change-process="changeProcess"
    />
  </div>
</template>
