<script setup lang="ts">
import { ref, reactive, computed, onMounted, watch } from 'vue';
import { useRouter } from 'vue-router';
import {
  Save, ArrowLeft, Trash2, Terminal, Zap,
  Cpu, Activity, Clock, Layers, Box, AlertTriangle, ExternalLink
} from 'lucide-vue-next';
import apiClient from '@/api/client';
import { API_PATHS } from '@/api/paths';
import type { CreateProgramRequest, HealthCheckConfig } from '@/types';
import { useAuthStore } from '@/stores/auth';
import { useCapabilitiesStore } from '@/stores/capabilities';
import UiSwitch from '@/components/ui/UiSwitch.vue';
import ProgramDependencyFields from '@/components/ProgramDependencyFields.vue';
import ProgramSectionCard from '@/components/ProgramSectionCard.vue';

const router = useRouter();
const authStore = useAuthStore();
const caps = useCapabilitiesStore();
const submitting = ref(false);
const submitError = ref('');

// Optional right-column section (resource limits)
const showLimits = ref(false);

// Doc links for field-level help
const DOC_BASE = 'https://super.docs.sconts.com/docs/06-internals/config-reference';
function docRef(section: string) {
  return `${DOC_BASE}/#${section}`;
}

onMounted(async () => {
  // Wait for profile on hard refresh — canOperate is false while user is still null.
  await authStore.ensureProfile();
  if (!authStore.canOperate) {
    router.replace('/');
  }
});

// Form state
const form = reactive({
  name: '',
  command: '',
  args: [] as { value: string }[], // UI helper array
  env: [] as { key: string, value: string }[], // UI helper array
  cwd: '',
  user: '',
  group: '',
  autostart: true,
  retry_limit: 3,
  autorestart: 'unexpected' as 'unexpected' | 'true' | 'false',
  exitcodes: [] as { value: number }[],
  startsecs: 10,
  stopsecs: null as number | null,
  priority: 999,
  stdout_logfile: '',
  stderr_logfile: '',
  depends_on: [] as { value: string }[],
  numprocs: 1,
  process_name: '{name}-{num}',

  // Hooks
  hooks: {
    pre_start: '',
    post_start: '',
    pre_stop: '',
    post_stop: '',
  },

  // Health Check
  enableHealthCheck: false,
  healthCheckType: 'tcp' as 'tcp' | 'http' | 'exec',
  hc_host: '127.0.0.1',
  hc_port: 8080,
  hc_url: 'http://127.0.0.1:8080/health',
  hc_method: 'GET',
  hc_command: '',
  // Health tuning (0 / empty = backend default; max_failures 0 disables auto-restart)
  hc_interval: '',
  hc_timeout: '',
  hc_start_period: '',
  hc_max_failures: '',

  enableHooks: false,
  enableLogs: false,

  // Premium
  cron: '',
  enableCron: false,
  // Cron policy (empty = backend default)
  on_overlap: 'skip' as 'skip' | 'queue' | 'kill',
  catchup: 'skip' as 'skip' | 'latest' | 'all',
  jitter_sec: '',
  max_concurrent: '',
  max_queued: '',
  cpu_quota: null as number | null,
  memory_mb: null as number | null,
  memory_warn_percent: null as number | null,
  memory_warn_headroom: null as number | null,
  memory_high: null as number | null,

  // OTA artifact
  enableArtifact: false,
  artifact_source: '',
  artifact_checksum: '',
  artifact_extract: false,
  artifact_destination: '',
  artifact_restart_policy: 'immediate',
  artifact_download_timeout: 60,
  artifact_verify_timeout: 60,
});

const isSignalRestartPolicy = computed(
  () =>
    form.enableArtifact &&
    form.artifact_restart_policy.trim().toLowerCase().startsWith('signal')
);

watch(isSignalRestartPolicy, (need) => {
  if (need && !form.enableHealthCheck) {
    form.enableHealthCheck = true;
  }
});

// Dynamic array helpers
const addArg = () => form.args.push({ value: '' });
const removeArg = (idx: number) => form.args.splice(idx, 1);
const addEnv = () => form.env.push({ key: '', value: '' });
const removeEnv = (idx: number) => form.env.splice(idx, 1);
const addExitcode = () => form.exitcodes.push({ value: 0 });
const removeExitcode = (idx: number) => form.exitcodes.splice(idx, 1);

/** Enter in an arg field: split on spaces if pasted as a line, else append a blank row. */
function onArgKeydown(idx: number, e: KeyboardEvent) {
  if (e.key !== 'Enter') return;
  e.preventDefault();
  const cur = form.args[idx];
  if (!cur) return;
  const raw = cur.value.trim();
  if (!raw) return;
  if (/\s/.test(raw)) {
    const parts = raw.split(/\s+/).filter(Boolean);
    form.args.splice(idx, 1, ...parts.map((value) => ({ value })));
  } else if (idx === form.args.length - 1) {
    addArg();
  }
}

// Command preview: join command + args for visual confirmation
const fullCommandLine = computed(() => {
  const parts = [form.command.trim()];
  const activeArgs = form.args.map(a => a.value).filter(v => v);
  parts.push(...activeArgs);
  return parts.join(' ');
});

// Detect if user put arguments inside the command field
const commandHasArgs = computed(() => {
  const trimmed = form.command.trim();
  return trimmed.includes(' ') && form.args.every(a => !a.value.trim());
});

// Auto-split command into command + args
function autoSplitCommand() {
  const trimmed = form.command.trim();
  const parts = trimmed.split(/\s+/);
  if (parts.length <= 1) return;
  const [first, ...detected] = parts;
  form.command = first!;
  if (form.args.length === 0 || form.args.every(a => !a.value.trim())) {
    form.args = detected.map(v => ({ value: v }));
  } else {
    const existing = form.args.filter(a => a.value.trim());
    if (existing.length === 0) {
      form.args = detected.map(v => ({ value: v }));
    }
  }
}

function formatApiError(e: any): string {
  const d = e?.response?.data;
  if (!d) return e?.message || 'Request failed';
  if (typeof d === 'string') return d;
  if (typeof d.message === 'string') return d.message;
  try {
    return JSON.stringify(d);
  } catch {
    return 'Request failed';
  }
}

async function handleSubmit() {
  // Auto-split command if it contains args and none were provided separately
  if (commandHasArgs.value) {
    autoSplitCommand();
  }

  if (!form.command.trim()) {
    submitError.value = 'Command is required.';
    return;
  }
  if (
    form.enableArtifact &&
    form.artifact_source.trim() &&
    form.artifact_restart_policy.trim().toLowerCase().startsWith('signal') &&
    !form.enableHealthCheck
  ) {
    submitError.value =
      'signal restart policy requires an enabled Health Check (tcp / http / exec).';
    return;
  }

  submitting.value = true;
  submitError.value = '';

  try {
    // 1. Build request payload
    const payload: CreateProgramRequest = {
      name: form.name || undefined,
      command: form.command,
      args: form.args.map(a => a.value).filter(v => v),
      env: form.env.reduce((acc, cur) => {
        if (cur.key) acc[cur.key] = cur.value;
        return acc;
      }, {} as Record<string, string>),
      cwd: form.cwd || undefined,
      user: form.user || undefined,
      group: form.group || undefined,
      autostart: form.autostart,
      retry_limit: form.retry_limit,
      autorestart: form.autorestart,
      startsecs: form.startsecs,
      stopsecs: form.stopsecs ?? undefined,
      priority: form.priority,
      stdout_logfile: form.enableLogs ? (form.stdout_logfile || undefined) : undefined,
      stderr_logfile: form.enableLogs ? (form.stderr_logfile || undefined) : undefined,
      depends_on: form.depends_on.map(d => d.value).filter(v => v),
      exitcodes: form.exitcodes.map(e => e.value).length > 0 ? form.exitcodes.map(e => e.value) : undefined,
      numprocs: form.numprocs,
      process_name: form.numprocs > 1 ? form.process_name : undefined,
      hooks: form.enableHooks
        ? {
            pre_start: form.hooks.pre_start || undefined,
            post_start: form.hooks.post_start || undefined,
            pre_stop: form.hooks.pre_stop || undefined,
            post_stop: form.hooks.post_stop || undefined,
          }
        : {},
      cron: form.enableCron && form.cron.trim() ? form.cron.trim() : undefined,
      on_overlap: form.enableCron && form.cron.trim() ? form.on_overlap : undefined,
      catchup: form.enableCron && form.cron.trim() ? form.catchup : undefined,
      jitter_sec: form.enableCron && form.cron.trim() && form.jitter_sec !== '' ? Number(form.jitter_sec) : undefined,
      max_concurrent: form.enableCron && form.cron.trim() && form.max_concurrent !== '' ? Number(form.max_concurrent) : undefined,
      max_queued: form.enableCron && form.cron.trim() && form.max_queued !== '' ? Number(form.max_queued) : undefined,
      resource_limits: undefined,
    };

    // 2. Health check
    if (form.enableHealthCheck) {
      let hc: HealthCheckConfig;
      if (form.healthCheckType === 'tcp') {
        hc = { type: 'tcp', host: form.hc_host, port: form.hc_port };
      } else if (form.healthCheckType === 'http') {
        hc = { type: 'http', url: form.hc_url, method: form.hc_method };
      } else {
        hc = { type: 'exec', command: form.hc_command };
      }
      // Tuning: empty input is omitted (backend default applies); 0 is sent as-is.
      if (form.hc_interval !== '') hc.interval_secs = Number(form.hc_interval);
      if (form.hc_timeout !== '') hc.timeout_secs = Number(form.hc_timeout);
      if (form.hc_start_period !== '') hc.start_period_secs = Number(form.hc_start_period);
      if (form.hc_max_failures !== '') hc.max_failures = Number(form.hc_max_failures);
      payload.health_check = hc;
    }

    // 3. Resource limits (isolation plugin only)
    if (
      caps.isolation &&
      (form.cpu_quota || form.memory_mb || form.memory_warn_percent || form.memory_warn_headroom || form.memory_high)
    ) {
      payload.resource_limits = {};
      if (form.cpu_quota) payload.resource_limits.cpu_quota = form.cpu_quota;
      if (form.memory_mb) payload.resource_limits.memory_limit = form.memory_mb;
      if (form.memory_warn_percent) payload.resource_limits.memory_warn_percent = form.memory_warn_percent;
      if (form.memory_warn_headroom) payload.resource_limits.memory_warn_headroom = form.memory_warn_headroom;
      if (form.memory_high) payload.resource_limits.memory_high = form.memory_high;
    }

    // 3.5 Artifact
    if (form.enableArtifact && form.artifact_source) {
      payload.artifact = {
        source: form.artifact_source,
        checksum: form.artifact_checksum,
        extract: form.artifact_extract,
        destination: form.artifact_destination,
        restart_policy: form.artifact_restart_policy,
        download_timeout: Number.isFinite(Number(form.artifact_download_timeout))
          ? Number(form.artifact_download_timeout)
          : 60,
        verify_timeout: Number.isFinite(Number(form.artifact_verify_timeout))
          ? Number(form.artifact_verify_timeout)
          : 60,
      };
    }

    // 4. Submit
    const res = await apiClient.post<string[]>(API_PATHS.PROGRAMS.CREATE, payload);
    if (!Array.isArray(res.data) || res.data.length === 0) {
      submitError.value = 'No program was created. The name may already exist — check the program list.';
      return;
    }

    // Redirect on success
    router.push('/');
  } catch (e: any) {
    submitError.value = formatApiError(e);
  } finally {
    submitting.value = false;
  }
}
</script>

<template>
  <div class="max-w-5xl mx-auto py-6 space-y-6 pb-20">

    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-4">
        <button class="inline-flex h-9 w-9 items-center justify-center rounded-xl text-muted-foreground hover:text-foreground hover:bg-muted transition-colors" @click="router.back()">
          <ArrowLeft class="w-5 h-5" />
        </button>
        <div>
          <h1 class="text-xl font-semibold tracking-tight">Create Program</h1>
          <p class="text-sm text-foreground/60">Define a new service or worker process.</p>
        </div>
      </div>
      <button class="h-9 gap-2 shadow-sm shadow-foreground/20 whitespace-nowrap inline-flex items-center justify-center rounded-xl font-medium px-3.5 text-sm bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50" :disabled="submitting" @click="handleSubmit">
        <span v-if="submitting" class="inline-block w-3 h-3 border-2 border-foreground/20 border-t-foreground/50 rounded-full animate-spin"></span>
        <Save v-else class="w-4 h-4" />
        Create Program
      </button>
    </div>

    <div v-if="submitError" class="alert alert-error text-sm" role="alert">
      {{ submitError }}
    </div>

    <div class="grid grid-cols-1 lg:grid-cols-12 gap-6 items-start">

      <!-- LEFT COLUMN: Main Config (8/12) -->
      <div class="lg:col-span-8 space-y-6">

        <!-- 1. Essentials -->
        <div class="surface-card border border-border shadow-sm overflow-visible">
          <div class="p-6">
            <h3 class="text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground mb-4 flex items-center gap-2">
              <Box class="w-4 h-4" /> Essentials
              <a :href="docRef('identity--execution')" target="_blank" class="inline-flex ml-1.5 text-foreground/25 hover:text-primary transition-colors" title="Docs: Identity & execution"><ExternalLink class="w-3 h-3" /></a>
            </h3>
            <!-- Essentials -->
            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div class="form-control">
                <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Program Name<a :href="docRef('identity--execution')" target="_blank" class="inline-flex ml-1 text-foreground/25 hover:text-primary align-text-bottom" title="Docs"><ExternalLink class="w-3 h-3" /></a></span></label>
                <input v-model="form.name" type="text" placeholder="e.g. my-worker" class="field-control" />
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Group<a :href="docRef('identity--execution')" target="_blank" class="inline-flex ml-1 text-foreground/25 hover:text-primary align-text-bottom" title="Docs"><ExternalLink class="w-3 h-3" /></a></span></label>
                <input v-model="form.group" type="text" placeholder="e.g. backend" class="field-control" />
              </div>
            </div>
            <div class="form-control mt-4">
              <label class="label pt-0 pb-1.5 justify-start gap-2">
                <span class="label-text text-xs font-medium text-foreground/75">Command<a :href="docRef('identity--execution')" target="_blank" class="inline-flex ml-1 text-foreground/25 hover:text-primary align-text-bottom" title="Docs"><ExternalLink class="w-3 h-3" /></a></span>
                <span class="text-destructive text-xs">*</span>
              </label>
              <div class="relative flex gap-1">
                <div class="relative flex-1">
                  <Terminal class="w-4 h-4 absolute left-3 top-3 text-foreground/40" />
                  <input v-model="form.command" type="text" placeholder="/usr/bin/python3" class="field-control !pl-10 font-mono text-sm w-full" />
                </div>
                <button
                  v-if="commandHasArgs"
                    class="inline-flex items-center justify-center rounded-lg font-medium text-xs h-10 px-2 text-warning/70 hover:text-warning hover:bg-warning/10 transition-colors"
                  title="Split command into command + arguments"
                  @click="autoSplitCommand"
                >
                  <Zap class="w-3.5 h-3.5" />
                </button>
              </div>
              <!-- Warning when command contains spaces but no separate args -->
              <div v-if="commandHasArgs" class="flex items-center gap-1.5 mt-2 text-xs text-warning">
                <AlertTriangle class="w-3 h-3 shrink-0" />
                <span>Command contains spaces — did you mean to put arguments separately? <button class="underline hover:text-warning-focus font-medium" @click="autoSplitCommand">Auto-split</button></span>
              </div>
            </div>
            <div class="form-control mt-4">
              <div class="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1 mb-1.5">
                <label class="label p-0"><span class="label-text text-xs font-medium text-foreground/75">Arguments<a :href="docRef('identity--execution')" target="_blank" class="inline-flex ml-1 text-foreground/25 hover:text-primary align-text-bottom" title="Docs"><ExternalLink class="w-3 h-3" /></a></span></label>
                <span class="text-[11px] text-muted-foreground/80">One token per row. Paste a line and press Enter to split.</span>
              </div>
              <div class="space-y-2.5">
                <div v-for="(arg, idx) in form.args" :key="idx" class="flex gap-2">
                  <input
                    v-model="arg.value"
                    type="text"
                    class="field-control font-mono"
                    placeholder="--flag"
                    @keydown="onArgKeydown(idx, $event)"
                  />
                  <button type="button" class="inline-flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground hover:text-destructive hover:bg-destructive/5 transition-colors" @click="removeArg(idx)"><Trash2 class="w-3.5 h-3.5" /></button>
                </div>
              </div>
              <button type="button" class="inline-flex items-center px-2.5 py-1.5 rounded-lg text-xs border border-dashed border-border text-muted-foreground hover:text-foreground hover:bg-muted transition-colors w-full mt-3" @click="addArg">+ Add Argument</button>
            </div>
            <!-- Full command preview -->
            <div v-if="form.command.trim()" class="mt-3 p-3 rounded-lg bg-muted/60 border border-border/50">
              <span class="text-xs uppercase tracking-wider text-foreground/40 font-semibold">Preview</span>
              <code class="block mt-1 text-xs font-mono text-foreground/80 break-all">{{ fullCommandLine || '(empty)' }}</code>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mt-4">
              <div class="form-control">
                <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Working Directory<a :href="docRef('identity--execution')" target="_blank" class="inline-flex ml-1 text-foreground/25 hover:text-primary align-text-bottom" title="Docs"><ExternalLink class="w-3 h-3" /></a></span></label>
                <input v-model="form.cwd" type="text" placeholder="/app/src" class="field-control font-mono text-sm" />
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Run As User<a :href="docRef('identity--execution')" target="_blank" class="inline-flex ml-1 text-foreground/25 hover:text-primary align-text-bottom" title="Docs"><ExternalLink class="w-3 h-3" /></a></span></label>
                <input v-model="form.user" type="text" placeholder="www-data" class="field-control font-mono text-sm" />
              </div>
            </div>
          </div>
        </div>

        <!-- 2. Environment Variables -->
        <div class="surface-card border border-border shadow-sm overflow-visible">
          <div class="p-6">
            <h3 class="text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground mb-4 flex items-center gap-2">
              <Layers class="w-4 h-4" /> Environment Variables
              <a :href="docRef('identity--execution')" target="_blank" class="inline-flex ml-1.5 text-foreground/25 hover:text-primary transition-colors" title="Docs: Identity & execution"><ExternalLink class="w-3 h-3" /></a>
            </h3>
            <div class="space-y-2">
              <div v-for="(item, idx) in form.env" :key="idx" class="grid grid-cols-[1fr_1fr_auto] gap-2">
                <input v-model="item.key" placeholder="KEY" class="field-control font-mono" />
                <input v-model="item.value" placeholder="VALUE" class="field-control font-mono" />
                <button class="inline-flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground hover:text-destructive hover:bg-destructive/5 transition-colors" @click="removeEnv(idx)"><Trash2 class="w-3.5 h-3.5" /></button>
              </div>
              <button class="inline-flex items-center px-2.5 py-1 rounded-lg text-xs border border-dashed border-border text-muted-foreground hover:text-foreground hover:bg-muted transition-colors w-full" @click="addEnv">+ Add Variable</button>
            </div>
          </div>
        </div>

        <!-- 3. Dependencies -->
        <ProgramDependencyFields
          v-model="form.depends_on"
          :exclude-name="form.name"
          :doc-href="docRef('orchestration')"
        />

        <!-- 4. Health Check -->
        <ProgramSectionCard
          v-model="form.enableHealthCheck"
          title="Health Check"
          hint="TCP / HTTP / exec probes"
          :doc-href="docRef('programshealth_check')"
          doc-title="Docs: Health checks"
        >
          <template #icon><Activity /></template>
          <p v-if="isSignalRestartPolicy" class="text-xs text-amber-700 dark:text-amber-400 mb-4">
            Required for signal restart policy — use a real TCP/HTTP/exec probe, not <code class="bg-muted px-1 rounded">true</code>.
          </p>
          <div class="space-y-6">
              <div>
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Check Strategy</span></label>
                <div class="grid grid-cols-3 gap-3">
                  <button
                    type="button"
                    class="flex flex-col items-center justify-center p-4 rounded-xl border text-center transition-colors"
                    :class="form.healthCheckType === 'tcp'
                      ? 'border-primary bg-primary/5 text-primary'
                      : 'border-border bg-muted/50 text-foreground hover:bg-card'"
                    @click="form.healthCheckType = 'tcp'"
                  >
                    <span class="text-sm font-medium">TCP Port</span>
                    <span class="text-xs opacity-60 mt-1">Connect to socket</span>
                  </button>
                  <button
                    type="button"
                    class="flex flex-col items-center justify-center p-4 rounded-xl border text-center transition-colors"
                    :class="form.healthCheckType === 'http'
                      ? 'border-primary bg-primary/5 text-primary'
                      : 'border-border bg-muted/50 text-foreground hover:bg-card'"
                    @click="form.healthCheckType = 'http'"
                  >
                    <span class="text-sm font-medium">HTTP Request</span>
                    <span class="text-xs opacity-60 mt-1">Expect 2xx status</span>
                  </button>
                  <button
                    type="button"
                    class="flex flex-col items-center justify-center p-4 rounded-xl border text-center transition-colors"
                    :class="form.healthCheckType === 'exec'
                      ? 'border-primary bg-primary/5 text-primary'
                      : 'border-border bg-muted/50 text-foreground hover:bg-card'"
                    @click="form.healthCheckType = 'exec'"
                  >
                    <span class="text-sm font-medium">Shell Command</span>
                    <span class="text-xs opacity-60 mt-1">Exit code 0</span>
                  </button>
                </div>
              </div>

              <div class="h-px bg-border"></div>

              <div class="min-h-[6.5rem]">
                <div v-show="form.healthCheckType === 'tcp'" class="grid grid-cols-1 md:grid-cols-2 gap-6">
                  <div class="form-control">
                    <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Host</span></label>
                    <input v-model="form.hc_host" class="field-control font-mono text-sm" placeholder="127.0.0.1" />
                  </div>
                  <div class="form-control">
                    <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Port</span></label>
                    <input v-model="form.hc_port" type="number" class="field-control font-mono text-sm" placeholder="8080" />
                  </div>
                </div>

                <div v-show="form.healthCheckType === 'http'" class="grid grid-cols-1 md:grid-cols-12 gap-4">
                  <div class="form-control md:col-span-3">
                    <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Method</span></label>
                    <select v-model="form.hc_method" class="field-control font-mono text-sm">
                      <option>GET</option>
                      <option>POST</option>
                      <option>HEAD</option>
                    </select>
                  </div>
                  <div class="form-control md:col-span-9">
                    <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">URL</span></label>
                    <input v-model="form.hc_url" class="field-control font-mono text-sm" placeholder="http://127.0.0.1:8080/health" />
                  </div>
                </div>

                <div v-show="form.healthCheckType === 'exec'" class="space-y-3">
                  <div class="form-control">
                    <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Command (Exit 0 = Healthy)</span></label>
                    <textarea
                      v-model="form.hc_command"
                      class="field-control font-mono text-sm h-24 py-2 leading-relaxed resize-y"
                      placeholder="curl -f http://localhost:8080 || exit 1"
                    ></textarea>
                  </div>
                </div>
              </div>

              <div class="h-px bg-border"></div>

              <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div class="form-control">
                  <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Probe Interval (s)</span></label>
                  <input v-model="form.hc_interval" type="number" min="0" class="field-control" placeholder="10 (default)" />
                </div>
                <div class="form-control">
                  <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Probe Timeout (s)</span></label>
                  <input v-model="form.hc_timeout" type="number" min="0" class="field-control" placeholder="0 (default)" />
                </div>
                <div class="form-control">
                  <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Start Period (s)</span></label>
                  <input v-model="form.hc_start_period" type="number" min="0" class="field-control" placeholder="0" />
                </div>
                <div class="form-control">
                  <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Max Failures</span></label>
                  <input v-model="form.hc_max_failures" type="number" min="0" class="field-control" placeholder="3 (default)" />
                </div>
              </div>
              <p class="text-xs text-muted-foreground">0 / empty = backend default. <code class="bg-muted px-1 rounded">max_failures 0</code> disables health-triggered auto-restart.</p>
          </div>
        </ProgramSectionCard>

        <!-- 5. OTA Artifact -->
        <ProgramSectionCard
          v-model="form.enableArtifact"
          title="OTA Artifact"
          hint="Fail-safe binary upgrade"
          doc-href="https://super.docs.sconts.com/docs/04-production-scenarios/delivery/fail-safe-ota/"
          doc-title="Docs: Fail-Safe OTA"
        >
          <template #icon><Zap /></template>
          <p class="text-xs text-muted-foreground">Define an artifact to download and replace the binary before the next restart.</p>
          <div class="form-control">
            <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Source URL</span></label>
            <input v-model="form.artifact_source" type="url" placeholder="https://artifacts.example.com/releases/my-app.tar.gz" class="field-control font-mono" />
          </div>
          <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Checksum</span></label>
              <input v-model="form.artifact_checksum" type="text" placeholder="sha256:abc123..." class="field-control font-mono" />
            </div>
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Destination Path</span></label>
              <input v-model="form.artifact_destination" type="text" placeholder="/opt/my-app" class="field-control font-mono" />
            </div>
          </div>
          <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
            <label class="flex items-center justify-between cursor-pointer group">
              <span class="font-medium text-sm group-hover:text-primary transition-colors">Extract archive</span>
              <UiSwitch v-model="form.artifact_extract" />
            </label>
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Restart Policy</span></label>
              <div class="relative">
                <select v-model="form.artifact_restart_policy" class="appearance-none bg-muted/60 hover:bg-border/60 focus:bg-border/60 focus:outline-none cursor-pointer text-sm font-medium h-9 min-h-0 w-full pl-3 pr-8 rounded-md border border-transparent">
                  <option value="immediate">immediate — restart process (SIGTERM), then verify</option>
                  <option value="manual">manual — commit swap only (no restart / no verify)</option>
                  <option value="signal:hup">signal:hup — in-place reload (requires health check)</option>
                  <option value="signal:int">signal:int — in-place reload (requires health check)</option>
                  <option value="signal:term">signal:term — in-place reload (requires health check)</option>
                  <option value="signal:quit">signal:quit — in-place reload (requires health check)</option>
                  <option value="signal:usr1">signal:usr1 — in-place reload (requires health check)</option>
                  <option value="signal:usr2">signal:usr2 — in-place reload (requires health check)</option>
                </select>
                <span class="absolute right-2 top-1/2 -translate-y-1/2 pointer-events-none text-foreground/40">
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/></svg>
                </span>
              </div>
              <p v-if="isSignalRestartPolicy" class="text-xs text-amber-700 dark:text-amber-400 mt-1.5">
                Hot-reload does not exec a new process. An enabled Health Check is required and will be turned on automatically.
              </p>
            </div>
          </div>
          <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Download timeout (s)</span></label>
              <input v-model.number="form.artifact_download_timeout" type="number" min="0" placeholder="60" class="field-control font-mono" />
              <p class="text-xs text-muted-foreground mt-1">HTTP download deadline. <code class="text-[10px]">0</code> disables overall transfer timeout.</p>
            </div>
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Verify timeout (s)</span></label>
              <input v-model.number="form.artifact_verify_timeout" type="number" min="0" placeholder="60" class="field-control font-mono" />
              <p class="text-xs text-muted-foreground mt-1">Post-swap health window before auto-rollback. <code class="text-[10px]">0</code> disables.</p>
            </div>
          </div>
        </ProgramSectionCard>

        <!-- 6. Lifecycle Hooks -->
        <ProgramSectionCard
          v-model="form.enableHooks"
          title="Lifecycle Hooks"
          hint="Pre/post start & stop scripts"
          :doc-href="docRef('programshooks')"
          doc-title="Docs: Hooks"
        >
          <template #icon><Activity /></template>
          <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Pre-Start</span></label>
              <textarea v-model="form.hooks.pre_start" class="field-control font-mono h-24 py-2 leading-relaxed resize-y" placeholder="Scripts to run before process spawn..."></textarea>
            </div>
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Post-Start</span></label>
              <textarea v-model="form.hooks.post_start" class="field-control font-mono h-24 py-2 leading-relaxed resize-y" placeholder="Scripts to run after process spawns..."></textarea>
            </div>
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Pre-Stop</span></label>
              <textarea v-model="form.hooks.pre_stop" class="field-control font-mono h-24 py-2 leading-relaxed resize-y" placeholder="Scripts to run before sending kill signal..."></textarea>
            </div>
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Post-Stop</span></label>
              <textarea v-model="form.hooks.post_stop" class="field-control font-mono h-24 py-2 leading-relaxed resize-y" placeholder="Scripts to run after process exits..."></textarea>
            </div>
          </div>
        </ProgramSectionCard>

        <!-- 7. Log Files -->
        <ProgramSectionCard
          v-model="form.enableLogs"
          title="Log Files"
          hint="Custom stdout / stderr paths"
          :doc-href="docRef('logging-1')"
          doc-title="Docs: Logging"
        >
          <template #icon><Box /></template>
          <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Stdout Log File</span></label>
              <input v-model="form.stdout_logfile" type="text" placeholder="auto (managed in log dir)" class="field-control font-mono" />
            </div>
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Stderr Log File</span></label>
              <input v-model="form.stderr_logfile" type="text" placeholder="auto (managed in log dir)" class="field-control font-mono" />
            </div>
          </div>
        </ProgramSectionCard>

      </div>

      <!-- RIGHT COLUMN -->
      <div class="lg:col-span-4 space-y-6">

        <!-- Control + Lifecycle (merged) -->
        <div class="surface-card border border-border shadow-sm p-5 space-y-5">
          <h3 class="text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground flex items-center gap-2">
            Control & Lifecycle
            <a :href="docRef('restart--stop-behaviour')" target="_blank" class="inline-flex text-foreground/25 hover:text-primary transition-colors" title="Docs"><ExternalLink class="w-3 h-3" /></a>
          </h3>
          <label class="flex items-center justify-between cursor-pointer group">
            <span class="font-medium text-sm group-hover:text-primary transition-colors">Autostart</span>
            <UiSwitch v-model="form.autostart" />
          </label>
          <div class="form-control">
            <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Retry Limit</span></label>
            <input v-model="form.retry_limit" type="number" min="0" class="field-control" />
          </div>
          <div class="form-control">
            <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Process Count (Replicas)<a :href="DOC_BASE.replace('config-reference','cli-reference')" target="_blank" class="inline-flex ml-1 text-foreground/25 hover:text-primary align-text-bottom" title="Docs: CLI --numprocs"><ExternalLink class="w-3 h-3" /></a></span></label>
            <input v-model="form.numprocs" type="number" min="1" class="field-control" />
          </div>
          <div v-if="form.numprocs > 1" class="form-control animate-in fade-in slide-in-from-top-2">
            <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Name Pattern<a :href="DOC_BASE.replace('config-reference','cli-reference')" target="_blank" class="inline-flex ml-1 text-foreground/25 hover:text-primary align-text-bottom" title="Docs: CLI --process-name"><ExternalLink class="w-3 h-3" /></a></span></label>
            <input v-model="form.process_name" type="text" class="field-control font-mono" placeholder="{name}-{num}" />
            <div class="text-xs text-muted-foreground mt-1">Avail: {name}, {num}</div>
          </div>
          <div class="h-px bg-border"></div>
          <div class="form-control">
            <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Autorestart Policy</span></label>
            <div class="relative">
              <select v-model="form.autorestart" class="appearance-none bg-muted/60 hover:bg-border/60 focus:bg-border/60 focus:outline-none cursor-pointer text-sm font-medium h-9 min-h-0 w-full pl-3 pr-8 rounded-md border border-transparent">
                <option value="unexpected">unexpected — only on non-zero / unknown exits</option>
                <option value="true">true — always restart</option>
                <option value="false">false — never restart</option>
              </select>
              <span class="absolute right-2 top-1/2 -translate-y-1/2 pointer-events-none text-foreground/40">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/></svg>
              </span>
            </div>
          </div>
          <div class="grid grid-cols-2 gap-3">
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Startsecs</span></label>
              <input v-model="form.startsecs" type="number" min="0" class="field-control" placeholder="10" />
              <span class="text-xs text-muted-foreground mt-0.5">Sec to mark stable</span>
            </div>
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Stopsecs</span></label>
              <input v-model.number="form.stopsecs" type="number" min="0" class="field-control" placeholder="global timeout" />
              <span class="text-xs text-muted-foreground mt-0.5">SIGTERM → SIGKILL wait</span>
            </div>
          </div>
          <div class="form-control">
            <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Priority</span></label>
            <input v-model="form.priority" type="number" class="field-control" placeholder="999" />
            <span class="text-xs text-muted-foreground mt-0.5">Lower = earlier startup order</span>
          </div>
          <div class="form-control">
            <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Exit Codes</span></label>
            <p class="text-xs text-muted-foreground mb-2">Expected success codes. Defaults to <code class="bg-muted px-1 rounded">[0]</code>.</p>
            <div class="space-y-2">
              <div v-for="(ec, idx) in form.exitcodes" :key="idx" class="flex gap-2">
                <input v-model.number="ec.value" type="number" class="field-control font-mono" placeholder="0" />
                <button class="inline-flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground hover:text-destructive hover:bg-destructive/5 transition-colors" @click="removeExitcode(idx)"><Trash2 class="w-3.5 h-3.5" /></button>
              </div>
              <button class="inline-flex items-center px-2.5 py-1 rounded-lg text-xs border border-dashed border-border text-muted-foreground hover:text-foreground hover:bg-muted transition-colors w-full" @click="addExitcode">+ Add Exit Code</button>
            </div>
          </div>
        </div>

        <!-- Cron Schedule -->
        <ProgramSectionCard
          v-model="form.enableCron"
          title="Cron Schedule"
          hint="Timed runs"
          :doc-href="docRef('orchestration')"
          doc-title="Docs: Orchestration"
        >
          <template #icon><Clock /></template>
          <div class="form-control">
            <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Expression</span></label>
            <input v-model="form.cron" type="text" placeholder="0 * * * * *" class="field-control font-mono" />
          </div>
          <div class="grid grid-cols-2 gap-3">
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Max Concurrent</span></label>
              <input v-model="form.max_concurrent" type="number" min="0" max="64" placeholder="1 (default)" class="field-control" />
              <span class="text-xs text-muted-foreground mt-0.5">Overlapping runs allowed</span>
            </div>
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Max Queued</span></label>
              <input v-model="form.max_queued" type="number" min="0" max="10000" placeholder="100 (default)" class="field-control" />
              <span class="text-xs text-muted-foreground mt-0.5">Cap queued firings</span>
            </div>
          </div>
          <div class="grid grid-cols-2 gap-3">
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">On Overlap</span></label>
              <div class="relative">
                <select v-model="form.on_overlap" class="appearance-none bg-muted/60 hover:bg-border/60 focus:bg-border/60 focus:outline-none cursor-pointer text-sm font-medium h-9 min-h-0 w-full pl-3 pr-8 rounded-md border border-transparent">
                  <option value="skip">skip — drop tick</option>
                  <option value="queue">queue — run after exit</option>
                  <option value="kill">kill — terminate running</option>
                </select>
                <span class="absolute right-2 top-1/2 -translate-y-1/2 pointer-events-none text-foreground/40">
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/></svg>
                </span>
              </div>
            </div>
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Catchup</span></label>
              <div class="relative">
                <select v-model="form.catchup" class="appearance-none bg-muted/60 hover:bg-border/60 focus:bg-border/60 focus:outline-none cursor-pointer text-sm font-medium h-9 min-h-0 w-full pl-3 pr-8 rounded-md border border-transparent">
                  <option value="skip">skip — no backfill</option>
                  <option value="latest">latest — last missed</option>
                  <option value="all">all — every missed slot</option>
                </select>
                <span class="absolute right-2 top-1/2 -translate-y-1/2 pointer-events-none text-foreground/40">
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/></svg>
                </span>
              </div>
            </div>
          </div>
          <div class="form-control">
            <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Jitter (s)</span></label>
            <input v-model="form.jitter_sec" type="number" min="0" placeholder="0" class="field-control" />
            <span class="text-xs text-muted-foreground mt-0.5">Random delay before each trigger</span>
          </div>
        </ProgramSectionCard>

        <!-- Resource Limits (isolation plugin) -->
        <ProgramSectionCard
          v-if="caps.isolation"
          v-model="showLimits"
          title="Resource Limits"
          hint="CPU / memory"
          :doc-href="docRef('programsresource_limits-')"
          doc-title="Docs: Resource limits"
        >
          <template #icon><Cpu /></template>
          <div class="grid grid-cols-2 gap-3">
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75 flex items-center gap-1"><Cpu class="w-3 h-3" /> CPU (cores)</span></label>
              <input v-model="form.cpu_quota" type="number" step="0.1" placeholder="1.0" class="field-control" />
            </div>
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75 flex items-center gap-1"><Box class="w-3 h-3" /> Mem (MB)</span></label>
              <input v-model="form.memory_mb" type="number" placeholder="512" class="field-control" />
            </div>
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Warn %</span></label>
              <input v-model="form.memory_warn_percent" type="number" min="0" max="100" placeholder="80" class="field-control" />
            </div>
            <div class="form-control">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Warn Headroom (MB)</span></label>
              <input v-model="form.memory_warn_headroom" type="number" placeholder="0" class="field-control" />
            </div>
            <div class="form-control col-span-2">
              <label class="label pt-0 pb-1.5"><span class="label-text text-xs font-medium text-foreground/75">Mem High (MB, soft)</span></label>
              <input v-model="form.memory_high" type="number" placeholder="0" class="field-control" />
            </div>
          </div>
        </ProgramSectionCard>

      </div>

    </div>
  </div>
</template>
