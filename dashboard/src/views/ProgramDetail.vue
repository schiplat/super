<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import ProcessDetailDrawer from '@/components/ProcessDetailDrawer.vue'
import { useProgramStore } from '@/stores/program'

const route = useRoute()
const router = useRouter()
const store = useProgramStore()

const processId = computed(() => String(route.params.id ?? ''))

function back() {
  void router.push('/programs')
}

function changeProcess(id: string) {
  void router.replace(`/programs/${id}`)
}

// Deep link / refresh: the drawer's summary (name, status, pid) resolves from
// the program store, so the list must be fetched here too.
onMounted(() => {
  void store.fetchPrograms()
})
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
