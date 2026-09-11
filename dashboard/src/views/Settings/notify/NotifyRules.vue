<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed, inject, type Ref } from 'vue';
import { Trash2, Save, Shield, ExternalLink, ArrowRight, Plus } from 'lucide-vue-next';
import apiClient from '@/api/client';
import {
  type NotificationConfig, type InhibitionRule,
  TRIGGER_OPTIONS,
} from '@/types/notification';
import { v4 as uuidv4 } from 'uuid';
import { useAuthStore } from '@/stores/auth';
import UiButton from '@/components/ui/UiButton.vue';
import { alertDialog } from '@/lib/app-dialog';
import type { NotifyHeaderAction } from './NotifyLayout.vue';

const EVENT_OPTIONS = TRIGGER_OPTIONS.filter(t => t.value !== '*');

const authStore = useAuthStore();
const canEdit = computed(() => authStore.canOperate);
const headerAction = inject<Ref<NotifyHeaderAction>>('notifyHeaderAction');

const loading = ref(false);
const savingRules = ref(false);
const config = ref<NotificationConfig>({ channels: [], inhibition_rules: [] });
const inhibitionRules = computed(() => config.value.inhibition_rules || []);

onMounted(async () => {
  if (headerAction) {
    headerAction.value = canEdit.value
      ? { label: 'Add Rule', onClick: addInhibitionRule, variant: 'outline' }
      : null;
  }
  loading.value = true;
  try {
    const cfgRes = await apiClient.get<NotificationConfig>('/api/v1/system/notify');
    config.value = cfgRes.data;
    if (!config.value.inhibition_rules) config.value.inhibition_rules = [];
  } catch (e) {
    console.error(e);
    await alertDialog('Failed to load notification config', {
      title: 'Load failed',
      variant: 'destructive',
    });
  } finally {
    loading.value = false;
  }
});

onUnmounted(() => {
  if (headerAction) headerAction.value = null;
});

function eventLabel(value: string): string {
  return EVENT_OPTIONS.find(o => o.value === value)?.label ?? value;
}

function formatMuteDuration(secs: number): string {
  if (!secs || secs < 60) return `${secs || 0}s`;
  const m = Math.round(secs / 60);
  return secs % 60 === 0 ? `${m} min` : `${secs}s`;
}

function shortEventLabel(value: string): string {
  const full = eventLabel(value);
  return full
    .replace(/^Process /, '')
    .replace(/^System /, '')
    .replace(' / Crash', '')
    .replace(' (Backoff)', '');
}

function toggleRuleEvent(rule: InhibitionRule, side: 'sources' | 'targets', value: string, checked: boolean) {
  const list = side === 'sources' ? rule.sources : rule.targets;
  if (checked) {
    if (!list.includes(value)) list.push(value);
  } else {
    const i = list.indexOf(value);
    if (i >= 0) list.splice(i, 1);
  }
}

function ruleHasOverlap(rule: InhibitionRule): boolean {
  return rule.sources.some(s => rule.targets.includes(s));
}

function addInhibitionRule() {
  if (!authStore.canOperate) return;
  if (!config.value.inhibition_rules) config.value.inhibition_rules = [];
  config.value.inhibition_rules.push({
    id: uuidv4(),
    sources: ['process_fatal'],
    targets: ['process_backoff'],
    match_on: ['program_name'],
    ttl_secs: 300,
  });
}

function removeInhibitionRule(idx: number) {
  config.value.inhibition_rules?.splice(idx, 1);
}

async function saveInhibitionRules() {
  if (!authStore.canOperate) return;
  const rules = config.value.inhibition_rules || [];
  for (const [i, rule] of rules.entries()) {
    if (!rule.sources.length || !rule.targets.length) {
      await alertDialog(`Rule ${i + 1}: pick at least one event on each side (When / Mute).`, {
        title: 'Validation',
      });
      return;
    }
  }
  savingRules.value = true;
  try {
    await apiClient.put('/api/v1/system/notify', {
      channels: config.value.channels,
      inhibition_rules: config.value.inhibition_rules,
      ...(config.value.delivery_keep_days != null ? { delivery_keep_days: config.value.delivery_keep_days } : {}),
      ...(config.value.delivery_db != null ? { delivery_db: config.value.delivery_db } : {}),
    });
    await alertDialog('Inhibition rules saved!', { title: 'Saved' });
  } catch (e: any) {
    await alertDialog(`Save rules failed: ${e.response?.data || e.message}`, {
      title: 'Save failed',
      variant: 'destructive',
    });
  } finally {
    savingRules.value = false;
  }
}
</script>

<template>
  <div class="space-y-6">
    <p v-if="!canEdit" class="text-xs text-muted-foreground -mt-2">View only — secrets redacted. Admin or Operator required to edit.</p>

    <div v-if="loading" class="flex justify-center py-16">
      <span class="inline-block w-6 h-6 border-2 border-muted-foreground/20 border-t-foreground/50 rounded-full animate-spin"></span>
    </div>

    <template v-else>
      <p class="text-sm text-muted-foreground -mt-2">
        Set up in order: when a serious event fires, mute related alerts for the same program.
        <a href="https://super.docs.sconts.com/docs/05-advanced-management/event-notifications/#storm-suppression" target="_blank" class="inline-flex items-center gap-0.5 text-primary hover:underline ml-1">
          Docs <ExternalLink class="w-3 h-3" />
        </a>
      </p>

      <div v-if="!inhibitionRules.length" class="surface-card border border-border p-6 space-y-5">
        <div class="text-center">
          <Shield class="w-8 h-8 text-muted-foreground/30 mx-auto mb-3" />
          <p class="text-sm font-medium">No inhibition rules yet</p>
          <p class="text-xs text-muted-foreground mt-1 max-w-sm mx-auto">
            Follow three steps: When → Mute targets → Duration.
          </p>
        </div>
        <div v-if="canEdit" class="rounded-lg border border-border bg-muted/30 px-4 py-4 space-y-3">
          <p class="text-xs font-semibold text-muted-foreground uppercase tracking-[0.06em]">Suggested flow</p>
          <ol class="space-y-2 text-sm">
            <li class="flex items-start gap-2.5">
              <span class="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-primary/10 text-primary text-[11px] font-semibold">1</span>
              <span><span class="font-medium">When</span> Process Fatal fires</span>
            </li>
            <li class="flex items-start gap-2.5">
              <span class="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-destructive/10 text-destructive text-[11px] font-semibold">2</span>
              <span><span class="font-medium">Mute targets</span> Process Restarting (Backoff)</span>
            </li>
            <li class="flex items-start gap-2.5">
              <span class="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground text-[11px] font-semibold">3</span>
              <span><span class="font-medium">For</span> 5 min · same program</span>
            </li>
          </ol>
          <UiButton size="sm" class="gap-1.5" @click="addInhibitionRule">
            <Plus class="w-3.5 h-3.5" /> Use this rule
          </UiButton>
        </div>
      </div>

      <div v-else class="space-y-3">
        <div
          v-for="(rule, ri) in inhibitionRules"
          :key="rule.id || ri"
          class="surface-card border border-border overflow-hidden"
        >
          <div class="flex items-start justify-between gap-3 px-4 py-3 bg-muted/25 border-b border-border/60">
            <div class="min-w-0 flex-1">
              <div class="text-[11px] font-semibold uppercase tracking-[0.06em] text-muted-foreground mb-2">
                Rule {{ ri + 1 }} · preview
              </div>
              <div class="flex flex-wrap items-center gap-1.5 text-xs">
                <span class="inline-flex items-center gap-1.5 rounded-md bg-primary/10 text-primary px-2.5 py-1.5">
                  <span class="text-[10px] uppercase tracking-wide text-primary/60 font-normal">When</span>
                  <template v-if="rule.sources.length">
                    <span v-for="(s, si) in rule.sources" :key="'ps-'+s" class="inline-flex items-center">
                      <span v-if="si" class="mx-1 text-primary/40 font-normal">or</span>
                      <span class="font-bold">{{ shortEventLabel(s) }}</span>
                    </span>
                  </template>
                  <span v-else class="font-bold opacity-40">…</span>
                </span>
                <ArrowRight class="w-3.5 h-3.5 text-muted-foreground/35 shrink-0" />
                <span class="inline-flex items-center gap-1.5 rounded-md bg-destructive/10 text-destructive px-2.5 py-1.5">
                  <span class="text-[10px] uppercase tracking-wide text-destructive/55 font-normal">Mute targets</span>
                  <template v-if="rule.targets.length">
                    <span v-for="(t, ti) in rule.targets" :key="'pt-'+t" class="inline-flex items-center">
                      <span v-if="ti" class="mr-1 text-destructive/40 font-normal">,</span>
                      <span class="font-bold">{{ shortEventLabel(t) }}</span>
                    </span>
                  </template>
                  <span v-else class="font-bold opacity-40">…</span>
                </span>
                <ArrowRight class="w-3.5 h-3.5 text-muted-foreground/35 shrink-0" />
                <span class="inline-flex items-center gap-1.5 rounded-md bg-muted/80 text-muted-foreground px-2 py-1">
                  <span class="text-[10px] uppercase tracking-wide text-muted-foreground/55 font-normal">For</span>
                  <span class="font-bold text-foreground/80">{{ formatMuteDuration(rule.ttl_secs) }}</span>
                </span>
                <span class="text-[11px] text-muted-foreground/45 font-normal">· same program</span>
              </div>
            </div>
            <button
              v-if="canEdit"
              type="button"
              class="inline-flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground hover:text-destructive hover:bg-destructive/10 shrink-0"
              title="Remove rule"
              @click="removeInhibitionRule(ri)"
            >
              <Trash2 class="w-3.5 h-3.5" />
            </button>
          </div>

          <fieldset class="p-4" :disabled="!canEdit">
            <ol class="space-y-0">
              <li class="relative flex gap-3 pb-5">
                <div class="flex flex-col items-center shrink-0">
                  <span class="inline-flex h-6 w-6 items-center justify-center rounded-full bg-primary text-primary-foreground text-xs font-semibold">1</span>
                  <span class="w-px flex-1 bg-border mt-1.5" aria-hidden="true"></span>
                </div>
                <div class="min-w-0 flex-1 pb-1">
                  <div class="text-sm font-semibold">When this happens</div>
                  <p class="text-xs text-muted-foreground mt-0.5 mb-2.5">Events that start or refresh the mute window.</p>
                  <div class="flex flex-wrap gap-1.5">
                    <label
                      v-for="ev in EVENT_OPTIONS"
                      :key="'src-'+ev.value"
                      class="inline-flex items-center gap-1.5 px-2 py-1 rounded-md border text-xs cursor-pointer transition-colors"
                      :class="rule.sources.includes(ev.value) ? 'border-primary/40 bg-primary/5 text-foreground' : 'border-border hover:bg-muted'"
                    >
                      <input
                        type="checkbox"
                        class="w-3.5 h-3.5 rounded accent-primary"
                        :checked="rule.sources.includes(ev.value)"
                        :disabled="!canEdit"
                        @change="(e: Event) => toggleRuleEvent(rule, 'sources', ev.value, (e.target as HTMLInputElement).checked)"
                      />
                      {{ ev.label }}
                    </label>
                  </div>
                </div>
              </li>

              <li class="relative flex gap-3 pb-5">
                <div class="flex flex-col items-center shrink-0">
                  <span class="inline-flex h-6 w-6 items-center justify-center rounded-full bg-destructive/90 text-white text-xs font-semibold">2</span>
                  <span class="w-px flex-1 bg-border mt-1.5" aria-hidden="true"></span>
                </div>
                <div class="min-w-0 flex-1 pb-1">
                  <div class="text-sm font-semibold">Mute targets</div>
                  <p class="text-xs text-muted-foreground mt-0.5 mb-2.5">
                    Action targets: which events to silence for the same program while the window is active.
                    An event may appear in both steps (e.g. Restarting): the first still notifies, then repeats are muted.
                  </p>
                  <div class="flex flex-wrap gap-1.5">
                    <label
                      v-for="ev in EVENT_OPTIONS"
                      :key="'tgt-'+ev.value"
                      class="inline-flex items-center gap-1.5 px-2 py-1 rounded-md border text-xs cursor-pointer transition-colors"
                      :class="rule.targets.includes(ev.value) ? 'border-destructive/40 bg-destructive/5 text-foreground' : 'border-border hover:bg-muted'"
                    >
                      <input
                        type="checkbox"
                        class="w-3.5 h-3.5 rounded accent-primary"
                        :checked="rule.targets.includes(ev.value)"
                        :disabled="!canEdit"
                        @change="(e: Event) => toggleRuleEvent(rule, 'targets', ev.value, (e.target as HTMLInputElement).checked)"
                      />
                      {{ ev.label }}
                    </label>
                  </div>
                  <p v-if="ruleHasOverlap(rule)" class="text-[11px] text-muted-foreground mt-2">
                    Overlap: events in both When and Mute still send once, then further repeats are muted for the duration.
                  </p>
                </div>
              </li>

              <li class="relative flex gap-3">
                <div class="flex flex-col items-center shrink-0">
                  <span class="inline-flex h-6 w-6 items-center justify-center rounded-full bg-muted text-muted-foreground text-xs font-semibold ring-1 ring-border">3</span>
                </div>
                <div class="min-w-0 flex-1">
                  <div class="text-sm font-semibold">Mute for how long</div>
                  <p class="text-xs text-muted-foreground mt-0.5 mb-2.5">Applies only to the same program name.</p>
                  <label class="inline-flex items-center gap-2 text-sm text-muted-foreground">
                    <input
                      v-model.number="rule.ttl_secs"
                      type="number"
                      min="30"
                      max="3600"
                      step="30"
                      class="field-control !h-9 !w-24 !text-sm font-mono text-center"
                    />
                    seconds
                    <span class="text-xs text-muted-foreground/50">≈ {{ formatMuteDuration(rule.ttl_secs) }}</span>
                  </label>
                </div>
              </li>
            </ol>
          </fieldset>
        </div>

        <div class="flex items-center justify-between pt-1">
          <UiButton v-if="canEdit" size="sm" variant="ghost" class="gap-1.5" @click="addInhibitionRule">
            <Plus class="w-3.5 h-3.5" /> Add Rule
          </UiButton>
          <UiButton v-if="canEdit" size="sm" class="gap-1.5 ml-auto" :disabled="savingRules" @click="saveInhibitionRules">
            <Save class="w-3.5 h-3.5" />
            {{ savingRules ? 'Saving…' : 'Save Inhibition rules' }}
          </UiButton>
        </div>
      </div>
    </template>
  </div>
</template>
