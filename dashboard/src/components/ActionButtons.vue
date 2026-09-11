<script setup lang="ts">
import { computed, ref, nextTick } from 'vue';
import { useProgramStore } from '@/stores/program';
import { useAuthStore } from '@/stores/auth';
import type { ProcessStatus } from '@/types';
import { Trash2, RotateCw, Loader2, Play, Square, AlertTriangle, MoreHorizontal } from 'lucide-vue-next';
import UiButton from '@/components/ui/UiButton.vue';
import UiDialog from '@/components/ui/UiDialog.vue';

const props = defineProps<{
  id: string;
  status: ProcessStatus;
  name?: string;
}>();

const store = useProgramStore();
const authStore = useAuthStore();

const isGlobalBusy = computed(() => store.operatingIds.has(props.id));
const currentAction = ref<'start' | 'stop' | 'restart' | 'remove' | null>(null);
const showModal = ref(false);
const pendingAction = ref<'start' | 'stop' | 'restart' | 'remove' | null>(null);

const isRunning = computed(() =>
  ['Running', 'Healthy', 'Starting', 'Backoff', 'Stopping', 'Waiting'].includes(props.status)
);

const canStart = computed(() => authStore.canOperate && !isRunning.value && !isGlobalBusy.value);
const canStop = computed(() => authStore.canOperate && isRunning.value && !isGlobalBusy.value);
const canRestart = computed(() => authStore.canOperate && isRunning.value && !isGlobalBusy.value);
const showLifecycle = computed(() => authStore.canOperate);
const isStopped = computed(() => !isRunning.value);
const canDelete = computed(() => authStore.canManage && isStopped.value && !isGlobalBusy.value);
const showDelete = computed(() => authStore.canManage);

const showOverflow = computed(() =>
  (showLifecycle.value && isRunning.value) || showDelete.value
);

const dropdownOpen = ref(false);
const overflowBtnRef = ref<HTMLElement | null>(null);
const dropdownStyle = ref<Record<string, string>>({});

function closeDropdown() {
  dropdownOpen.value = false;
}

async function toggleDropdown() {
  dropdownOpen.value = !dropdownOpen.value;
  if (dropdownOpen.value) {
    await nextTick();
    const btn = overflowBtnRef.value;
    if (btn) {
      const rect = btn.getBoundingClientRect();
      dropdownStyle.value = {
        position: 'fixed',
        top: `${rect.bottom + 4}px`,
        right: `${window.innerWidth - rect.right}px`,
      };
    }
  }
}

function openDialog(action: 'start' | 'stop' | 'restart' | 'remove') {
  if (action === 'remove' && !authStore.canManage) return;
  if (action !== 'remove' && !authStore.canOperate) return;
  closeDropdown();
  pendingAction.value = action;
  showModal.value = true;
}

async function executeAction() {
  if (!pendingAction.value) return;
  const action = pendingAction.value;
  showModal.value = false;
  currentAction.value = action;
  try {
    if (action === 'start') await store.startProgram(props.id);
    else if (action === 'stop') await store.stopProgram(props.id);
    else if (action === 'restart') await store.restartProgram(props.id);
    else if (action === 'remove') await store.removeProgram(props.id);
  } finally {
    currentAction.value = null;
    pendingAction.value = null;
  }
}

const modalContent = computed(() => {
  const action = pendingAction.value;
  switch (action) {
    case 'start': return { title: 'Start Process', msg: 'Are you sure you want to start this process?', variant: 'default' as const, icon: Play };
    case 'stop': return { title: 'Stop Process', msg: 'Are you sure you want to stop this process?', variant: 'destructive' as const, icon: Square };
    case 'restart': return { title: 'Restart Process', msg: 'Are you sure you want to restart this process?', variant: 'secondary' as const, icon: RotateCw };
    case 'remove': return { title: 'Delete Configuration', msg: 'This will permanently remove the process configuration.', variant: 'destructive' as const, icon: Trash2 };
    default: return { title: 'Confirm Action', msg: 'Are you sure?', variant: 'default' as const, icon: AlertTriangle };
  }
});

const programLabel = computed(() => {
  if (props.name) return props.name;
  return props.id.slice(0, 8);
});
</script>

<template>
  <div class="flex items-center justify-end gap-0.5">
    <!-- Start -->
    <button
      v-if="showLifecycle"
      class="inline-flex items-center gap-1.5 text-xs font-medium h-7 px-2.5 rounded-lg transition-colors"
      :class="canStart ? 'text-success hover:bg-success/10' : 'text-muted-foreground/25 pointer-events-none'"
      :disabled="!canStart"
      @click="openDialog('start')"
    >
      <Loader2 v-if="currentAction === 'start'" class="w-3.5 h-3.5 animate-spin" />
      <Play v-else class="w-3.5 h-3.5 fill-current" />
      Start
    </button>
    <!-- Stop -->
    <button
      v-if="showLifecycle"
      class="inline-flex items-center gap-1.5 text-xs font-medium h-7 px-2.5 rounded-lg transition-colors"
      :class="canStop ? 'text-destructive hover:bg-destructive/10' : 'text-muted-foreground/25 pointer-events-none'"
      :disabled="!canStop"
      @click="openDialog('stop')"
    >
      <Loader2 v-if="currentAction === 'stop'" class="w-3.5 h-3.5 animate-spin" />
      <Square v-else class="w-3.5 h-3.5 fill-current" />
      Stop
    </button>

    <!-- Overflow dropdown: Restart + Delete -->
    <div v-if="showOverflow" class="relative">
      <button
        ref="overflowBtnRef"
        class="inline-flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground/40 hover:text-muted-foreground hover:bg-muted transition-colors"
        @click="toggleDropdown"
      >
        <MoreHorizontal class="w-3.5 h-3.5" />
      </button>
      <Teleport to="body">
        <div v-if="dropdownOpen" class="fixed inset-0 z-[9998]" @click="closeDropdown" />
        <div
          v-if="dropdownOpen"
          class="fixed bg-card border border-border rounded-xl shadow-lg z-[9999] min-w-[130px] p-1.5 flex flex-col gap-0.5"
          :style="dropdownStyle"
        >
          <!-- Restart -->
          <button
            v-if="showLifecycle && isRunning"
            class="flex items-center gap-2 px-3 py-2 rounded-lg text-sm text-muted-foreground hover:text-foreground hover:bg-muted/80 transition-colors"
            :class="!canRestart ? 'opacity-20 pointer-events-none' : ''"
            :disabled="!canRestart"
            @click="openDialog('restart')"
          >
            <Loader2 v-if="currentAction === 'restart'" class="w-4 h-4 animate-spin" />
            <RotateCw v-else class="w-4 h-4" />
            Restart
          </button>
          <!-- Delete -->
          <div v-if="showDelete" class="relative group/delete">
            <button
              class="flex items-center gap-2 px-3 py-2 rounded-lg text-sm w-full transition-colors"
              :class="canDelete ? 'text-destructive hover:bg-destructive/10' : 'text-muted-foreground/15 pointer-events-none'"
              :disabled="!canDelete"
              @click="openDialog('remove')"
            >
              <Loader2 v-if="currentAction === 'remove'" class="w-4 h-4 animate-spin" />
              <Trash2 v-else class="w-4 h-4" />
              Delete
            </button>
            <span
              v-if="!canDelete && !isStopped"
              class="absolute right-full mr-2 top-1/2 -translate-y-1/2 opacity-0 group-hover/delete:opacity-100 transition-opacity pointer-events-none inline-flex items-center px-1.5 py-px rounded-md border text-xs leading-none bg-warning/10 text-warning border-warning/20 font-medium whitespace-nowrap"
            >Stop first</span>
          </div>
        </div>
      </Teleport>
    </div>

    <!-- Confirmation Dialog -->
    <UiDialog :open="showModal" @update:open="val => showModal = val" :title="modalContent.title">
      <template #description>
        {{ modalContent.msg }}
      </template>
      <p class="mt-1 font-bold text-foreground">{{ programLabel }}</p>
      <div class="flex items-center gap-3 mt-4">
        <UiButton variant="secondary" @click="showModal = false">Cancel</UiButton>
        <UiButton :variant="modalContent.variant" @click="executeAction">Confirm</UiButton>
      </div>
    </UiDialog>
  </div>
</template>
