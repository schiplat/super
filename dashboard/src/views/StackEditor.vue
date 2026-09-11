<script setup lang="ts">
import { ref, onMounted, watch, computed } from 'vue';
import { VueMonacoEditor } from '@guolao/vue-monaco-editor';
import { FONT_MONO } from '@/constants/fonts';
import { useDark } from '@vueuse/core';
import {
  Save, Download, Layers, AlertTriangle, Trash2, Plus, RefreshCw,
} from 'lucide-vue-next';
import UiButton from '@/components/ui/UiButton.vue';
import UiDialog from '@/components/ui/UiDialog.vue';
import UiInput from '@/components/ui/UiInput.vue';
import UiBadge from '@/components/ui/UiBadge.vue';
import apiClient from '@/api/client';
import { API_PATHS } from '@/api/paths';
import { useAuthStore } from '@/stores/auth';
import { alertDialog, confirmDialog } from '@/lib/app-dialog';
import { cn } from '@/lib/utils';

/** Must match CLI `PRUNE_CONFIRM_TOKEN` — soft yes/confirm is not enough. */
const PRUNE_CONFIRM_TOKEN = 'confirmed';

interface ApplyPlan {
  create: string[];
  keep: string[];
  remove: string[];
}

const authStore = useAuthStore();
const canApply = computed(() => authStore.canManage);

// 1. Editor theme
const isDark = useDark({ selector: 'html', attribute: 'class', valueDark: 'dark', valueLight: 'light' });
const editorTheme = ref(isDark.value ? 'vs-dark' : 'vs');
watch(isDark, (val) => { editorTheme.value = val ? 'vs-dark' : 'vs'; });

// 2. Core state
const code = ref('');
const loading = ref(false);
const applying = ref(false);

// 3. Result modal state
const showResultModal = ref(false);
const resultLogs = ref<string[]>([]);

// 4. Prune gate (diff + type `confirmed`)
const showPruneModal = ref(false);
const prunePlan = ref<ApplyPlan | null>(null);
const pruneConfirmInput = ref('');
const pendingPayload = ref<Record<string, unknown> | null>(null);
const pruneConfirmReady = computed(
  () => pruneConfirmInput.value.trim() === PRUNE_CONFIRM_TOKEN,
);

// 5. Default template (includes prune: false as a hint)
const defaultTemplate = {
  "prune": false,
  "services": [
    {
      "name": "example-service",
      "command": "sleep",
      "args": ["1000"],
      "autostart": true,
      "retry_limit": 3
    }
  ]
};

onMounted(() => {
  if (code.value.length < 20) {
    code.value = JSON.stringify(defaultTemplate, null, 2);
  }
});

function buildApplyPlan(
  stackNames: string[],
  currentNames: string[],
  prune: boolean,
): ApplyPlan {
  const stack = new Set(stackNames);
  const current = new Set(currentNames);
  const create = [...stack].filter((n) => !current.has(n)).sort();
  const keep = [...stack].filter((n) => current.has(n)).sort();
  const remove = prune
    ? [...current].filter((n) => !stack.has(n)).sort()
    : [];
  return { create, keep, remove };
}

function closePruneModal() {
  showPruneModal.value = false;
  prunePlan.value = null;
  pendingPayload.value = null;
  pruneConfirmInput.value = '';
}

// 6. Load current stack from API
async function loadCurrentStack() {
  if (code.value.length > 50) {
    const ok = await confirmDialog('Discard current changes and load from system?', {
      title: 'Discard changes?',
      confirmLabel: 'Load',
      variant: 'destructive',
    });
    if (!ok) return;
  }

  loading.value = true;
  try {
    const res = await apiClient.get('/api/v1/stack');
    // Pretty-print full JSON including prune
    code.value = JSON.stringify(res.data, null, 2);
  } catch (e: any) {
    await alertDialog('Failed to load stack: ' + e.message, {
      title: 'Load failed',
      variant: 'destructive',
    });
  } finally {
    loading.value = false;
  }
}

async function executeApply(payload: Record<string, unknown>) {
  applying.value = true;
  try {
    const res = await apiClient.put('/api/v1/stack', payload);
    resultLogs.value = res.data || [];
    showResultModal.value = true;
  } catch (e: any) {
    const d = e.response?.data;
    const msg = typeof d === 'string' ? d : (d?.message || e.message);
    await alertDialog(`Apply failed: ${msg}`, {
      title: 'Apply failed',
      variant: 'destructive',
    });
  } finally {
    applying.value = false;
  }
}

/** Apply stack — when prune=true would remove programs, show diff + type gate first. */
async function applyStack() {
  if (!authStore.canManage) return;
  try {
    const payload = JSON.parse(code.value) as Record<string, unknown>;

    // Guard: prune must be boolean if present
    if (typeof payload.prune !== 'boolean') {
      const ok = await confirmDialog(
        'Warning: "prune" field is missing or invalid (default is false). Continue?',
        {
          title: 'Continue apply?',
          confirmLabel: 'Continue',
        },
      );
      if (!ok) return;
    }

    const prune = payload.prune === true;
    if (prune) {
      applying.value = true;
      try {
        const listRes = await apiClient.get(API_PATHS.PROGRAMS.LIST);
        const currentNames = (listRes.data || [])
          .map((p: { name?: string }) => p.name)
          .filter((n: string | undefined): n is string => !!n);
        const services = Array.isArray(payload.services) ? payload.services : [];
        const stackNames = services
          .map((s: { name?: string }) => s?.name)
          .filter((n: string | undefined): n is string => !!n);
        const plan = buildApplyPlan(stackNames, currentNames, true);

        if (plan.remove.length > 0) {
          prunePlan.value = plan;
          pendingPayload.value = payload;
          pruneConfirmInput.value = '';
          showPruneModal.value = true;
          return;
        }
      } finally {
        applying.value = false;
      }
    }

    await executeApply(payload);
  } catch (e: any) {
    if (e instanceof SyntaxError) {
      await alertDialog(`Invalid JSON: ${e.message}`, {
        title: 'Invalid JSON',
        variant: 'destructive',
      });
    } else {
      await alertDialog(`Apply failed: ${e.message}`, {
        title: 'Apply failed',
        variant: 'destructive',
      });
    }
  }
}

async function confirmPruneApply() {
  if (!pruneConfirmReady.value || !pendingPayload.value) return;
  const payload = pendingPayload.value;
  closePruneModal();
  await executeApply(payload);
}
</script>

<template>
  <div class="max-w-5xl mx-auto py-6 pb-20">
  <div class="h-[calc(100vh-8rem)] flex flex-col bg-card border border-border rounded-xl shadow-sm overflow-hidden">

    <!-- Toolbar Area -->
    <div class="border-b border-border bg-muted/30">

      <!-- Top Row: Title & Actions -->
      <div class="px-6 pt-5 pb-3 flex flex-col md:flex-row md:items-center justify-between gap-4">

        <!-- Left: Title -->
        <div class="flex gap-4">
          <div class="p-3 bg-primary/10 text-primary rounded-xl h-fit shrink-0">
            <Layers class="w-6 h-6" />
          </div>
          <div>
            <h2 class="text-xl font-bold text-foreground tracking-tight">Stack Editor</h2>
            <p class="text-sm text-foreground/50 font-medium">Declarative configuration via JSON.</p>
          </div>
        </div>

        <!-- Right: Action Buttons -->
        <div class="flex items-center gap-3">
          <button class="inline-flex items-center gap-2 px-3 py-1.5 rounded-xl text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-muted transition-colors" @click="loadCurrentStack" :disabled="loading">
            <Download class="w-4 h-4" />
            Load Current
          </button>

          <button
            v-if="canApply"
            class="inline-flex items-center justify-center gap-2 rounded-xl font-medium px-3.5 text-sm h-9 bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
            :disabled="applying"
            @click="applyStack"
          >
            <span v-if="applying" class="inline-block w-3 h-3 border-2 border-foreground/20 border-t-foreground/50 rounded-full animate-spin"></span>
            <Save v-else class="w-4 h-4" />
            Apply Stack
          </button>
          <span v-else class="text-xs text-foreground/50 px-2">View only — Admin required to apply</span>
        </div>
      </div>

      <!-- Bottom Row: Warning (100% Width) -->
      <div class="px-6 pb-5 w-full">
        <div class="flex items-start sm:items-center gap-3 px-4 py-3 rounded-r-lg border border-l-4 border-border border-l-warning bg-card shadow-sm w-full">
          <AlertTriangle class="w-5 h-5 shrink-0 text-warning mt-0.5 sm:mt-0" />
          <span class="text-sm leading-relaxed font-medium text-foreground/80">
            <span class="font-bold text-foreground">Caution:</span>
            Setting
            <code class="font-mono font-bold mx-1 px-1.5 py-0.5 rounded bg-muted text-foreground text-xs">"prune": true</code>
            will
            <span class="font-bold text-destructive">PERMANENTLY DESTROY</span>
            any running services not defined in this JSON. Apply shows a diff and requires typing
            <code class="font-mono font-bold mx-1 px-1.5 py-0.5 rounded bg-muted text-foreground text-xs">{{ PRUNE_CONFIRM_TOKEN }}</code>
            before removals run.
          </span>
        </div>
      </div>

    </div>

    <!-- Editor Area -->
    <div class="flex-1 relative">
      <VueMonacoEditor
        v-model:value="code"
        language="json"
        :theme="editorTheme"
        :options="{
          automaticLayout: true,
          minimap: { enabled: false },
          fontSize: 13,
          fontFamily: FONT_MONO,
          scrollBeyondLastLine: false,
          tabSize: 2,
          padding: { top: 20, bottom: 20 },
          readOnly: !canApply
        }"
        class="h-full w-full"
      />
    </div>

    <!-- Prune confirmation: apply diff + type confirmed -->
    <UiDialog
      :open="showPruneModal"
      size="lg"
      title="Confirm irreversible prune"
      @update:open="(val) => { if (!val) closePruneModal() }"
    >
      <template #description>
        <span class="font-medium text-destructive">prune: true</span>
        stops and unregisters every managed program missing from this stack.
        Process definitions are gone until you re-apply a full inventory.
      </template>

      <div v-if="prunePlan" class="space-y-5">
        <!-- REMOVE first — the irreversible part -->
        <section
          class="rounded-xl border border-destructive/25 bg-destructive/5 overflow-hidden"
          aria-labelledby="prune-remove-heading"
        >
          <div class="flex items-center gap-2 px-3.5 py-2.5 border-b border-destructive/15">
            <Trash2 class="w-4 h-4 text-destructive shrink-0" />
            <h3 id="prune-remove-heading" class="text-sm font-semibold text-destructive tracking-tight">
              Will be removed
            </h3>
            <UiBadge variant="destructive" size="sm" class="ml-auto tabular-nums">
              {{ prunePlan.remove.length }}
            </UiBadge>
          </div>
          <ul class="flex flex-wrap gap-1.5 p-3 max-h-40 overflow-y-auto">
            <li
              v-for="n in prunePlan.remove"
              :key="'r-' + n"
              class="inline-flex items-center rounded-lg border border-destructive/20 bg-card px-2 py-1 font-mono text-xs text-destructive"
            >
              {{ n }}
            </li>
          </ul>
        </section>

        <!-- Secondary: keep / create summary -->
        <section class="grid grid-cols-2 gap-2.5" aria-label="Also in this apply">
          <div class="rounded-xl border border-border bg-muted/30 px-3 py-2.5 min-w-0">
            <div class="flex items-center gap-1.5 text-muted-foreground mb-2">
              <RefreshCw class="w-3.5 h-3.5 shrink-0" />
              <span class="text-[11px] font-semibold uppercase tracking-wide">Keep / update</span>
              <span class="ml-auto text-[11px] tabular-nums font-medium text-foreground/70">{{ prunePlan.keep.length }}</span>
            </div>
            <div v-if="prunePlan.keep.length" class="flex flex-wrap gap-1 max-h-20 overflow-y-auto">
              <span
                v-for="n in prunePlan.keep"
                :key="'k-' + n"
                class="inline-flex max-w-full truncate rounded-md bg-card border border-border px-1.5 py-0.5 font-mono text-[11px] text-foreground/80"
              >{{ n }}</span>
            </div>
            <p v-else class="text-[11px] text-muted-foreground italic">None</p>
          </div>
          <div class="rounded-xl border border-border bg-muted/30 px-3 py-2.5 min-w-0">
            <div class="flex items-center gap-1.5 text-muted-foreground mb-2">
              <Plus class="w-3.5 h-3.5 shrink-0" />
              <span class="text-[11px] font-semibold uppercase tracking-wide">Create</span>
              <span class="ml-auto text-[11px] tabular-nums font-medium text-foreground/70">{{ prunePlan.create.length }}</span>
            </div>
            <div v-if="prunePlan.create.length" class="flex flex-wrap gap-1 max-h-20 overflow-y-auto">
              <span
                v-for="n in prunePlan.create"
                :key="'c-' + n"
                class="inline-flex max-w-full truncate rounded-md bg-card border border-border px-1.5 py-0.5 font-mono text-[11px] text-foreground/80"
              >{{ n }}</span>
            </div>
            <p v-else class="text-[11px] text-muted-foreground italic">None</p>
          </div>
        </section>

        <!-- Typed confirmation -->
        <section class="space-y-2.5 pt-1 border-t border-border">
          <label class="block text-sm text-foreground leading-relaxed" for="prune-confirm-input">
            Type
            <kbd
              class="mx-1 inline-flex items-center rounded-md border border-warning/40 bg-warning/10 px-2 py-0.5 font-mono text-[13px] font-semibold tracking-wide text-warning align-baseline shadow-[inset_0_-1px_0_hsl(var(--warning)/0.25)]"
            >{{ PRUNE_CONFIRM_TOKEN }}</kbd>
            to continue
            <span class="block mt-1 text-xs text-muted-foreground font-normal">
              Lowercase, no quotes.
              <code class="font-mono text-[11px]">y</code>
              /
              <code class="font-mono text-[11px]">yes</code>
              are rejected.
            </span>
          </label>
          <UiInput
            id="prune-confirm-input"
            v-model="pruneConfirmInput"
            :placeholder="PRUNE_CONFIRM_TOKEN"
            autocomplete="off"
            autofocus
            spellcheck="false"
            :class="cn(
              'font-mono text-sm tracking-wide',
              pruneConfirmReady && 'border-success/50 focus-visible:ring-success/30',
              pruneConfirmInput.length > 0 && !pruneConfirmReady && 'border-destructive/40',
            )"
            @keydown.enter.prevent="confirmPruneApply"
          />
        </section>

        <div class="flex justify-end gap-2 pt-0.5">
          <UiButton variant="secondary" @click="closePruneModal">Cancel</UiButton>
          <UiButton
            variant="destructive"
            :disabled="!pruneConfirmReady || applying"
            @click="confirmPruneApply"
          >
            Remove {{ prunePlan.remove.length }} program{{ prunePlan.remove.length === 1 ? '' : 's' }}
          </UiButton>
        </div>
      </div>
    </UiDialog>

    <!-- Result Modal -->
    <UiDialog :open="showResultModal" @update:open="val => showResultModal = val" title="Stack Applied Successfully">
      <div class="py-4">
        <div class="bg-[#1e1e1e] text-gray-300 p-4 rounded-lg font-mono text-xs max-h-60 overflow-y-auto border border-border">
          <div v-if="resultLogs.length === 0" class="opacity-50 italic">No changes detected.</div>
          <div v-for="(log, idx) in resultLogs" :key="idx" class="flex gap-2">
            <span class="text-gray-500">&gt;</span>
            <span>{{ log }}</span>
          </div>
        </div>
      </div>
      <div class="flex justify-end">
        <UiButton @click="showResultModal = false">Close</UiButton>
      </div>
    </UiDialog>

  </div>
  </div>
</template>
