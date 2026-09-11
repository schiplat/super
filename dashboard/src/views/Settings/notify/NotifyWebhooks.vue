<script setup lang="ts">
import { ref, onMounted, onUnmounted, reactive, computed, inject, type Ref } from 'vue';
import {
  Bell, Plus, Trash2, Save, Send, Link, Shield, Tag,
  ChevronDown, ChevronRight, ExternalLink,
} from 'lucide-vue-next';
import apiClient from '@/api/client';
import {
  type NotificationConfig, type ChannelConfig, type NotifyStats,
  TRIGGER_OPTIONS, NOTIFICATION_TYPES,
} from '@/types/notification';
import { v4 as uuidv4 } from 'uuid';
import { useAuthStore } from '@/stores/auth';
import UiSwitch from '@/components/ui/UiSwitch.vue';
import UiButton from '@/components/ui/UiButton.vue';
import { alertDialog, confirmDialog } from '@/lib/app-dialog';
import type { NotifyHeaderAction } from './NotifyLayout.vue';

const authStore = useAuthStore();
const canEdit = computed(() => authStore.canOperate);
const headerAction = inject<Ref<NotifyHeaderAction>>('notifyHeaderAction');

function getTypeInfo(type: string) {
  const normalized = type === 'lark' || type === 'feishu' ? 'feishu'
    : type === 'wechat' || type === 'wechat_work' ? 'wecom'
    : type === 'msteams' ? 'teams'
    : type;
  return NOTIFICATION_TYPES.find(t => t.value === normalized) || NOTIFICATION_TYPES[0];
}

function updateChannelType(ch: ChannelConfig, newType: string) {
  const info = NOTIFICATION_TYPES.find(t => t.value === newType);
  if (info) {
    ch.type = info.value;
  }
}

/** Defaults from docs (storm-suppression examples). */
const DEFAULT_COOLDOWN_SECS = 60;
const DEFAULT_WINDOW_SECS = 30;
const DEFAULT_MAX_EVENTS = 10;

const loading = ref(false);
const savingId = ref<string | null>(null);
const testingId = ref<string | null>(null);
const expandedId = ref<string | null>(null);

const config = ref<NotificationConfig>({ channels: [], inhibition_rules: [] });
const stats = ref<NotifyStats>({ success: 0, failed: 0, suppressed: 0, snapshots: [], channels: {} });
const headersMap = reactive(new Map<string, { key: string; value: string }[]>());

function channelStat(chId: string): { success: number; failed: number; suppressed: number; aggregated: number } {
  return stats.value.channels?.[chId] ?? { success: 0, failed: 0, suppressed: 0, aggregated: 0 };
}

onMounted(async () => {
  if (headerAction) {
    headerAction.value = canEdit.value
      ? { label: 'Add Webhook', onClick: addChannel }
      : null;
  }
  loading.value = true;
  try {
    const [cfgRes, statsRes] = await Promise.all([
      apiClient.get<NotificationConfig>('/api/v1/system/notify'),
      apiClient.get<NotifyStats>('/api/v1/system/notify/stats').catch(() => ({
        data: { success: 0, failed: 0, suppressed: 0, aggregated: 0, snapshots: [], channels: {} } as NotifyStats,
      })),
    ]);
    config.value = cfgRes.data;
    stats.value = statsRes.data;
    config.value.channels.forEach(ch => {
      ch.type = getTypeInfo(ch.type).value;
      ensureStrategyDefaults(ch);
      initHeaders(ch);
    });
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

function initHeaders(ch: ChannelConfig) {
  const headersObj = ch.config.headers || {};
  const arr = Object.entries(headersObj).map(([k, v]) => ({ key: k, value: v }));
  if (arr.length === 0) arr.push({ key: '', value: '' });
  headersMap.set(ch.id, arr);
}

async function validateChannel(ch: ChannelConfig): Promise<boolean> {
  if (!ch.name || !ch.name.trim()) {
    await alertDialog(`Channel "${ch.id.slice(0, 8)}...": Name is required.`, {
      title: 'Validation',
    });
    return false;
  }
  if (!ch.config.url || !ch.config.url.trim()) {
    await alertDialog(`Channel "${ch.name}": Webhook URL is required.`, {
      title: 'Validation',
    });
    return false;
  }
  const url = ch.config.url.trim().toLowerCase();
  if (!url.startsWith('http://') && !url.startsWith('https://')) {
    await alertDialog(`Channel "${ch.name}": URL must start with http:// or https://`, {
      title: 'Validation',
    });
    return false;
  }
  return true;
}

function syncHeadersToChannel(ch: ChannelConfig) {
  const arr = headersMap.get(ch.id) || [];
  const obj: Record<string, string> = {};
  arr.forEach(item => {
    if (item.key.trim()) obj[item.key.trim()] = item.value.trim();
  });
  ch.config.headers = obj;
}

function toggleExpand(id: string) {
  expandedId.value = expandedId.value === id ? null : id;
}

function ensureStrategyDefaults(ch: ChannelConfig) {
  if (!ch.strategy?.mode) {
    ch.strategy = ch.cooldown_secs > 0
      ? { mode: 'cooldown', cooldown_secs: ch.cooldown_secs }
      : { mode: 'immediate' };
  }
  if (ch.strategy.mode === 'cooldown') {
    const n = ch.strategy.cooldown_secs ?? ch.cooldown_secs;
    ch.strategy.cooldown_secs = n && n > 0 ? n : DEFAULT_COOLDOWN_SECS;
  }
  if (ch.strategy.mode === 'batch') {
    ch.strategy.window_secs = ch.strategy.window_secs && ch.strategy.window_secs > 0
      ? ch.strategy.window_secs
      : DEFAULT_WINDOW_SECS;
    ch.strategy.max_events = ch.strategy.max_events && ch.strategy.max_events > 0
      ? ch.strategy.max_events
      : DEFAULT_MAX_EVENTS;
  }
}

function addChannel() {
  if (!authStore.canOperate) return;
  const newId = uuidv4();
  const newChannel: ChannelConfig = {
    id: newId,
    name: 'New Channel',
    type: 'webhook',
    triggers: ['process_fatal'],
    include_log_tail: true,
    cooldown_secs: 0,
    strategy: { mode: 'immediate' },
    config: { url: '', headers: {} },
  };
  config.value.channels.push(newChannel);
  initHeaders(newChannel);
  expandedId.value = newId;
}

async function removeChannel(idx: number) {
  if (!authStore.canOperate) return;
  const ok = await confirmDialog('Delete this notification channel?', {
    title: 'Delete channel',
    confirmLabel: 'Delete',
    variant: 'destructive',
  });
  if (!ok) return;
  const ch = config.value.channels[idx];
  if (!ch) return;
  headersMap.delete(ch.id);
  config.value.channels.splice(idx, 1);
}

function addHeaderRow(chId: string) {
  const arr = headersMap.get(chId);
  if (arr) arr.push({ key: '', value: '' });
}

function removeHeaderRow(chId: string, idx: number) {
  const arr = headersMap.get(chId);
  if (arr) arr.splice(idx, 1);
}

async function saveChannel(ch: ChannelConfig) {
  if (!authStore.canOperate) return;
  syncHeadersToChannel(ch);
  if (!(await validateChannel(ch))) return;
  savingId.value = ch.id;
  try {
    await apiClient.put('/api/v1/system/notify/channel', ch);
    await alertDialog(`Channel "${ch.name}" saved!`, { title: 'Saved' });
  } catch (e: any) {
    await alertDialog(`Save failed: ${e.response?.data || e.message}`, {
      title: 'Save failed',
      variant: 'destructive',
    });
  } finally {
    savingId.value = null;
  }
}

async function testChannel(ch: ChannelConfig) {
  if (!authStore.canOperate) return;
  syncHeadersToChannel(ch);
  if (!ch.config.url || !ch.config.url.trim()) {
    await alertDialog('Please enter a Webhook URL before testing.', {
      title: 'Missing URL',
    });
    return;
  }
  testingId.value = ch.id;
  try {
    await apiClient.post('/api/v1/system/notify/test', ch);
    await alertDialog(`Test sent to "${ch.name}"! Check your webhook receiver.`, {
      title: 'Test sent',
    });
    try {
      const res = await apiClient.get<NotifyStats>('/api/v1/system/notify/stats');
      stats.value = res.data;
    } catch { /* ignore */ }
  } catch (e: any) {
    await alertDialog(`Test failed: ${e.response?.data || e.message}`, {
      title: 'Test failed',
      variant: 'destructive',
    });
  } finally {
    testingId.value = null;
  }
}

function effectiveStrategy(ch: ChannelConfig): string {
  if (ch.strategy?.mode) return ch.strategy.mode;
  return ch.cooldown_secs > 0 ? 'cooldown' : 'immediate';
}

function onStrategyChange(ch: ChannelConfig, mode: string) {
  if (!ch.strategy) ch.strategy = { mode: 'immediate' };
  ch.strategy.mode = mode as 'immediate' | 'cooldown' | 'batch';
  if (mode === 'cooldown') {
    const n = ch.strategy.cooldown_secs ?? ch.cooldown_secs;
    ch.strategy.cooldown_secs = n && n > 0 ? n : DEFAULT_COOLDOWN_SECS;
  } else if (mode === 'batch') {
    if (!ch.strategy.window_secs || ch.strategy.window_secs <= 0) {
      ch.strategy.window_secs = DEFAULT_WINDOW_SECS;
    }
    if (!ch.strategy.max_events || ch.strategy.max_events <= 0) {
      ch.strategy.max_events = DEFAULT_MAX_EVENTS;
    }
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
      <div v-if="config.channels.length === 0" class="surface-card border border-border py-14 flex flex-col items-center text-center">
        <Bell class="w-8 h-8 text-muted-foreground/30 mb-3" />
        <p class="text-sm font-medium">No webhooks yet</p>
        <p class="text-xs text-muted-foreground mt-1 max-w-xs">Connect Slack, DingTalk, Feishu, WeCom, Teams, or a custom webhook.</p>
        <UiButton v-if="canEdit" size="sm" variant="outline" class="mt-5 gap-1.5" @click="addChannel">
          <Plus class="w-3.5 h-3.5" /> Create Webhook
        </UiButton>
      </div>

      <div v-else class="space-y-3">
        <div
          v-for="(ch, idx) in config.channels"
          :key="ch.id"
          class="surface-card border border-border overflow-hidden transition-colors"
          :class="expandedId === ch.id ? 'border-primary/40' : 'hover:border-border'"
        >
          <div class="flex items-center gap-3 px-4 py-3 cursor-pointer select-none" @click="toggleExpand(ch.id)">
            <ChevronRight class="w-4 h-4 text-muted-foreground/50 shrink-0 transition-transform" :class="expandedId === ch.id ? 'rotate-90' : ''" />

            <span class="inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium bg-muted text-muted-foreground shrink-0">
              {{ getTypeInfo(ch.type).label }}
            </span>

            <div class="min-w-0 flex-1">
              <div class="text-sm font-semibold truncate">{{ ch.name || 'Untitled' }}</div>
              <div class="text-xs text-muted-foreground truncate font-mono mt-0.5">{{ ch.config.url || 'No URL' }}</div>
            </div>

            <div class="hidden sm:flex items-center gap-3 shrink-0 text-xs tabular-nums">
              <span v-if="channelStat(ch.id).success || channelStat(ch.id).failed" class="text-muted-foreground">
                <span class="text-success">{{ channelStat(ch.id).success }}</span>
                <span class="mx-1 opacity-30">/</span>
                <span class="text-destructive">{{ channelStat(ch.id).failed }}</span>
              </span>
              <span class="w-1.5 h-1.5 rounded-full" :class="ch.triggers.length ? 'bg-success' : 'bg-border'"></span>
            </div>

            <div v-if="canEdit" class="flex items-center gap-0.5 shrink-0" @click.stop>
              <button
                type="button"
                class="inline-flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
                :disabled="!!testingId"
                title="Test"
                @click="testChannel(ch)"
              >
                <span v-if="testingId === ch.id" class="inline-block w-3 h-3 border-2 border-muted-foreground/30 border-t-foreground/60 rounded-full animate-spin"></span>
                <Send v-else class="w-3.5 h-3.5" />
              </button>
              <button
                type="button"
                class="inline-flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors"
                title="Delete"
                @click="removeChannel(idx)"
              >
                <Trash2 class="w-3.5 h-3.5" />
              </button>
            </div>
          </div>

          <fieldset v-if="expandedId === ch.id" class="border-t border-border px-4 py-5 space-y-6" :disabled="!canEdit">
            <section class="space-y-3">
              <h4 class="text-xs font-semibold uppercase tracking-[0.06em] text-muted-foreground">Identity</h4>
              <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <div>
                  <label class="text-xs text-muted-foreground mb-1.5 block">Name <span class="text-destructive">*</span></label>
                  <div class="relative">
                    <Tag class="w-3.5 h-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground/40" />
                    <input v-model="ch.name" type="text" placeholder="Production Alerts" class="field-control !pl-9 !h-9 !text-sm" />
                  </div>
                </div>
                <div>
                  <label class="text-xs text-muted-foreground mb-1.5 block">Platform</label>
                  <div class="relative">
                    <select
                      class="field-control !h-9 !text-sm appearance-none pr-8"
                      :value="getTypeInfo(ch.type).value"
                      @change="(e: Event) => updateChannelType(ch, (e.target as HTMLSelectElement).value)"
                    >
                      <option v-for="pt in NOTIFICATION_TYPES" :key="pt.value" :value="pt.value">{{ pt.label }}</option>
                    </select>
                    <ChevronDown class="w-3.5 h-3.5 absolute right-2.5 top-1/2 -translate-y-1/2 pointer-events-none text-muted-foreground/50" />
                  </div>
                </div>
              </div>
              <div>
                <label class="text-xs text-muted-foreground mb-1.5 block">Webhook URL <span class="text-destructive">*</span></label>
                <div class="relative">
                  <Link class="w-3.5 h-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground/40" />
                  <input v-model="ch.config.url" type="url" :placeholder="getTypeInfo(ch.type).urlHint" class="field-control !pl-9 !h-9 !text-xs font-mono" />
                </div>
              </div>
              <div v-if="getTypeInfo(ch.type).showSecret">
                <label class="text-xs text-muted-foreground mb-1.5 block">
                  Signing Secret
                  <span class="ml-1.5 text-muted-foreground/50 font-mono">{{ getTypeInfo(ch.type).secretLabel }}</span>
                </label>
                <div class="relative">
                  <Shield class="w-3.5 h-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground/40" />
                  <input v-model="ch.config.secret" type="password" placeholder="Optional HMAC secret" class="field-control !pl-9 !h-9 !text-xs font-mono" />
                </div>
              </div>
            </section>

            <section class="space-y-3">
              <h4 class="text-xs font-semibold uppercase tracking-[0.06em] text-muted-foreground">Custom Headers</h4>
              <div class="space-y-2">
                <div v-for="(header, hIdx) in headersMap.get(ch.id)" :key="hIdx" class="grid grid-cols-[1fr_1fr_auto] gap-2">
                  <input v-model="header.key" placeholder="Header" class="field-control !h-8 !text-xs font-mono" />
                  <input v-model="header.value" placeholder="Value" class="field-control !h-8 !text-xs font-mono" />
                  <button type="button" class="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground/40 hover:text-destructive hover:bg-destructive/10" @click="removeHeaderRow(ch.id, hIdx)">
                    <Trash2 class="w-3.5 h-3.5" />
                  </button>
                </div>
                <button type="button" class="w-full h-8 rounded-lg border border-dashed border-border text-xs text-muted-foreground hover:text-foreground hover:bg-muted transition-colors" @click="addHeaderRow(ch.id)">
                  + Add Header
                </button>
              </div>
            </section>

            <section class="space-y-3">
              <h4 class="text-xs font-semibold uppercase tracking-[0.06em] text-muted-foreground">Trigger Events</h4>
              <div class="flex flex-wrap gap-2">
                <label
                  class="inline-flex items-center gap-2 px-2.5 py-1.5 rounded-lg border text-xs cursor-pointer transition-colors"
                  :class="ch.triggers.includes('*') ? 'border-primary bg-primary/5 text-primary' : 'border-border hover:bg-muted'"
                >
                  <input type="checkbox" class="w-3.5 h-3.5 rounded accent-primary"
                    :checked="ch.triggers.includes('*')"
                    @change="e => { if ((e.target as HTMLInputElement).checked) ch.triggers = ['*']; else ch.triggers = []; }"
                  />
                  All Events
                </label>
                <label
                  v-for="opt in TRIGGER_OPTIONS.filter(o => o.value !== '*')"
                  :key="opt.value"
                  class="inline-flex items-center gap-2 px-2.5 py-1.5 rounded-lg border text-xs cursor-pointer transition-colors"
                  :class="[
                    ch.triggers.includes('*') ? 'opacity-40 pointer-events-none' : '',
                    !ch.triggers.includes('*') && ch.triggers.includes(opt.value) ? 'border-primary/40 bg-primary/5' : 'border-border hover:bg-muted',
                  ]"
                >
                  <input type="checkbox" class="w-3.5 h-3.5 rounded accent-primary" :value="opt.value" v-model="ch.triggers" :disabled="ch.triggers.includes('*')" />
                  {{ opt.label }}
                </label>
              </div>
              <label class="flex items-center gap-3 pt-1 cursor-pointer">
                <UiSwitch v-model="ch.include_log_tail" />
                <span class="text-sm">Attach log tail <span class="text-xs text-muted-foreground">(last 2KB stderr)</span></span>
              </label>
            </section>

            <section class="space-y-3">
              <h4 class="text-xs font-semibold uppercase tracking-[0.06em] text-muted-foreground flex items-center gap-1.5">
                Delivery Strategy
                <a href="https://super.docs.sconts.com/docs/05-advanced-management/event-notifications/#storm-suppression" target="_blank" class="text-muted-foreground/40 hover:text-primary" title="Docs">
                  <ExternalLink class="w-3 h-3" />
                </a>
              </h4>
              <select
                :value="effectiveStrategy(ch)"
                class="field-control !h-9 !text-sm"
                @change="(e: Event) => onStrategyChange(ch, (e.target as HTMLSelectElement).value)"
              >
                <option value="immediate">Immediate — every event</option>
                <option value="cooldown">Cooldown — skip repeats</option>
                <option value="batch">Batch — send summary</option>
              </select>
              <div v-if="effectiveStrategy(ch) === 'cooldown'" class="flex items-center gap-2">
                <input
                  v-model.number="ch.strategy.cooldown_secs"
                  type="number"
                  min="5"
                  max="300"
                  step="5"
                  class="field-control !h-8 !w-20 !text-sm font-mono text-center"
                />
                <span class="text-xs text-muted-foreground">seconds between sends <span class="text-muted-foreground/50">(default {{ DEFAULT_COOLDOWN_SECS }})</span></span>
              </div>
              <div v-if="effectiveStrategy(ch) === 'batch'" class="flex items-center gap-4">
                <label class="flex items-center gap-1.5 text-xs text-muted-foreground">
                  Window
                  <input
                    v-model.number="ch.strategy.window_secs"
                    type="number"
                    min="5"
                    max="600"
                    step="5"
                    class="field-control !h-8 !w-16 !text-sm font-mono text-center"
                  />
                  s
                </label>
                <label class="flex items-center gap-1.5 text-xs text-muted-foreground">
                  Max
                  <input
                    v-model.number="ch.strategy.max_events"
                    type="number"
                    min="2"
                    max="50"
                    class="field-control !h-8 !w-16 !text-sm font-mono text-center"
                  />
                  events
                </label>
              </div>
            </section>

            <div v-if="canEdit" class="flex items-center justify-end gap-2 pt-1 border-t border-border">
              <UiButton size="sm" variant="outline" class="gap-1.5" :disabled="!!testingId" @click="testChannel(ch)">
                <span v-if="testingId === ch.id" class="inline-block w-3 h-3 border-2 border-muted-foreground/30 border-t-foreground/60 rounded-full animate-spin"></span>
                <Send v-else class="w-3.5 h-3.5" />
                Test
              </UiButton>
              <UiButton size="sm" class="gap-1.5" :disabled="savingId === ch.id" @click="saveChannel(ch)">
                <span v-if="savingId === ch.id" class="inline-block w-3 h-3 border-2 border-primary-foreground/30 border-t-primary-foreground rounded-full animate-spin"></span>
                <Save v-else class="w-3.5 h-3.5" />
                Save Channel
              </UiButton>
            </div>
          </fieldset>
        </div>
      </div>
    </template>
  </div>
</template>
