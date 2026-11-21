<script setup lang="ts">
import { ref, reactive, computed, onMounted, watch } from 'vue';
import { useRouter, useRoute } from 'vue-router';
import {
  Save, Trash2, Terminal, Zap,
  Cpu, Activity, Clock, Layers, Box, AlertTriangle, ExternalLink
} from 'lucide-vue-next';
import apiClient from '@/api/client';
import { API_PATHS } from '@/api/paths';
import type { UpdateProgramRequest, ProgramDetail } from '@/types';
import { useAuthStore } from '@/stores/auth';
import { useCapabilitiesStore } from '@/stores/capabilities';
import UiSwitch from '@/components/ui/UiSwitch.vue';
import ProgramDependencyFields from '@/components/ProgramDependencyFields.vue';
import ProgramSectionCard from '@/components/ProgramSectionCard.vue';
import FormAnchorRail from '@/components/FormAnchorRail.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiNotice from '@/components/ui/UiNotice.vue';
import { alertDialog } from '@/lib/app-dialog';

const router = useRouter();
const route = useRoute();
const authStore = useAuthStore();
const caps = useCapabilitiesStore();
const programId = route.params.id as string;

const loading = ref(true);
const submitting = ref(false);
const submitError = ref('');

const showLimits = ref(false);

// Doc links for field-level help
const DOC_BASE = 'https://super.docs.sconts.com/docs/06-internals/config-reference';
function docRef(section: string) {
  return `${DOC_BASE}/#${section}`;
}

function initOptionalSections(cfg: ProgramDetail['config']) {
  form.enableHooks = !!(
    cfg.hooks &&
    (cfg.hooks.pre_start || cfg.hooks.post_start || cfg.hooks.pre_stop || cfg.hooks.post_stop)
  );
  form.enableLogs = !!(cfg.stdout_logfile || cfg.stderr_logfile);
  form.enableArtifact = !!cfg.artifact;
  form.enableHealthCheck = !!cfg.health_check;
  form.enableCron = !!cfg.cron;
  const rl = cfg.resource_limits;
  showLimits.value = !!(
    rl &&
    ((rl.cpu_quota != null && rl.cpu_quota > 0) ||
      (rl.memory_limit != null && rl.memory_limit > 0) ||
      (rl.memory_warn_percent != null && rl.memory_warn_percent > 0) ||
      (rl.memory_warn_headroom != null && rl.memory_warn_headroom > 0) ||
      (rl.memory_high != null && rl.memory_high > 0))
  );
}

// Form state
const form = reactive({
  name: '',
  command: '',
  args: [] as { value: string }[],
  env: [] as { key: string, value: string }[],
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
  // numprocs not editable in edit mode

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
  // Health tuning (0 = backend default; max_failures 0 disables auto-restart)
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
  kill_after_secs: '',
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

// Load detail and populate form
onMounted(async () => {
  // Wait for profile on hard refresh — canManage is false while user is still null.
  await authStore.ensureProfile();
  if (!authStore.canManage) {
    router.replace('/');
    return;
  }
  if (!programId) return;
  loading.value = true;
  try {
    const res = await apiClient.get<ProgramDetail>(API_PATHS.PROGRAMS.DETAIL(programId));
    const cfg = res.data.config;

    // 1. Essentials
    form.name = cfg.name;
    form.command = cfg.command;
    form.args = (cfg.args || []).map(v => ({ value: v }));
    form.cwd = cfg.cwd || '';
    form.user = cfg.user || '';
    form.group = cfg.group || '';

    // 2. Env (Obj -> Array)
    if (cfg.env) {
      form.env = Object.entries(cfg.env).map(([k, v]) => ({ key: k, value: v }));
    }

    // 3. Controls
    form.autostart = cfg.autostart;
    form.retry_limit = cfg.retry_limit;
    form.autorestart = cfg.autorestart || 'unexpected';
    form.exitcodes = (cfg.exitcodes || [0]).map(v => ({ value: v }));
    form.startsecs = cfg.startsecs ?? 10;
    form.stopsecs = cfg.stopsecs ?? null;
    form.priority = cfg.priority ?? 999;
    form.stdout_logfile = cfg.stdout_logfile || '';
    form.stderr_logfile = cfg.stderr_logfile || '';
    form.depends_on = (cfg.depends_on || []).map(v => ({ value: v }));

    // 4. Hooks
    if (cfg.hooks) {
      form.hooks.pre_start = cfg.hooks.pre_start || '';
      form.hooks.post_start = cfg.hooks.post_start || '';
      form.hooks.pre_stop = cfg.hooks.pre_stop || '';
      form.hooks.post_stop = cfg.hooks.post_stop || '';
    }

    // 5. Health Check
    if (cfg.health_check) {
      form.enableHealthCheck = true;
      const hc = cfg.health_check;
      if (hc.type === 'tcp') {
        form.healthCheckType = 'tcp';
        form.hc_host = hc.host || '127.0.0.1';
        form.hc_port = hc.port || 8080;
      } else if (hc.type === 'http') {
        form.healthCheckType = 'http';
        form.hc_url = hc.url || '';
        form.hc_method = hc.method || 'GET';
      } else if (hc.type === 'exec') {
        form.healthCheckType = 'exec';
        form.hc_command = hc.command || '';
      }
      // Tuning values come back as numbers (0 = backend default)
      form.hc_interval = String(hc.interval_secs ?? '');
      form.hc_timeout = String(hc.timeout_secs ?? '');
      form.hc_start_period = String(hc.start_period_secs ?? '');
      form.hc_max_failures = String(hc.max_failures ?? '');
    }

    // 6. Premium
    form.cron = cfg.cron || '';
    form.on_overlap = cfg.on_overlap || 'skip';
    form.catchup = cfg.catchup || 'skip';
    form.jitter_sec = cfg.jitter_sec != null ? String(cfg.jitter_sec) : '';
    form.max_concurrent = cfg.max_concurrent != null ? String(cfg.max_concurrent) : '';
    form.max_queued = cfg.max_queued != null ? String(cfg.max_queued) : '';
    form.kill_after_secs = (cfg as any).kill_after_secs != null ? String((cfg as any).kill_after_secs) : '';
    if ((cfg as any).resource_limits) {
      const rl = (cfg as any).resource_limits;
      form.cpu_quota = rl.cpu_quota || null;
      form.memory_mb = rl.memory_limit || null;
      form.memory_warn_percent = rl.memory_warn_percent || null;
      form.memory_warn_headroom = rl.memory_warn_headroom || null;
      form.memory_high = rl.memory_high || null;
    }

    // 7. Artifact
    if (cfg.artifact) {
      form.artifact_source = cfg.artifact.source;
      form.artifact_checksum = cfg.artifact.checksum;
      form.artifact_extract = cfg.artifact.extract ?? false;
      form.artifact_destination = cfg.artifact.destination;
      // Bare "signal" is equivalent to signal:hup (API default).
      const rp = (cfg.artifact.restart_policy || 'immediate').trim().toLowerCase();
      form.artifact_restart_policy = rp === 'signal' ? 'signal:hup' : (cfg.artifact.restart_policy || 'immediate');
      form.artifact_download_timeout = cfg.artifact.download_timeout ?? 60;
      form.artifact_verify_timeout = cfg.artifact.verify_timeout ?? 60;
    }

    initOptionalSections(cfg);

  } catch (e) {
    console.error(e);
    await alertDialog('Failed to load program details', {
      title: 'Load failed',
      variant: 'destructive',
    });
    router.back();
  } finally {
    loading.value = false;
  }
});

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
    const payload: UpdateProgramRequest = {
      name: form.name || undefined,
      command: form.command,
      args: form.args.map(a => a.value).filter(v => v),
      env: form.env.reduce((acc, cur) => {
        if (cur.key) acc[cur.key] = cur.value;
        return acc;
      }, {} as Record<string, string>),
      cwd: form.cwd, // Empty string allowed
      user: form.user, // Empty string allowed
      group: form.group, // Empty string allowed
      autostart: form.autostart,
      retry_limit: form.retry_limit,
      autorestart: form.autorestart,
      exitcodes: form.exitcodes.map(e => e.value),
      startsecs: form.startsecs,
      stopsecs: form.stopsecs ?? undefined,
      priority: form.priority,
      stdout_logfile: form.enableLogs ? (form.stdout_logfile || '') : '',
      stderr_logfile: form.enableLogs ? (form.stderr_logfile || '') : '',
      depends_on: form.depends_on.map(d => d.value).filter(v => v),
      hooks: form.enableHooks
        ? {
            pre_start: form.hooks.pre_start || undefined,
            post_start: form.hooks.post_start || undefined,
            pre_stop: form.hooks.pre_stop || undefined,
            post_stop: form.hooks.post_stop || undefined,
          }
        : {},
      // Empty string clears cron on the server (Enable off / expression cleared).
      cron: form.enableCron && form.cron.trim() ? form.cron.trim() : '',
      on_overlap: form.enableCron && form.cron.trim() ? form.on_overlap : undefined,
      catchup: form.enableCron && form.cron.trim() ? form.catchup : undefined,
      jitter_sec: form.enableCron && form.cron.trim() && form.jitter_sec !== '' ? Number(form.jitter_sec) : undefined,
      max_concurrent: form.enableCron && form.cron.trim() && form.max_concurrent !== '' ? Number(form.max_concurrent) : undefined,
      max_queued: form.enableCron && form.cron.trim() && form.max_queued !== '' ? Number(form.max_queued) : undefined,
      // Empty field sends 0 = explicitly disable the cap (undefined means "no change").
      kill_after_secs: form.enableCron && form.cron.trim() ? (form.kill_after_secs !== '' ? Number(form.kill_after_secs) : 0) : undefined,
      resource_limits: undefined, // Filled below
    };

    // Health Check Logic
    if (form.enableHealthCheck) {
      let hc: UpdateProgramRequest['health_check'];
      if (form.healthCheckType === 'tcp') {
        hc = { type: 'tcp', host: form.hc_host, port: form.hc_port };
      } else if (form.healthCheckType === 'http') {
        hc = { type: 'http', url: form.hc_url, method: form.hc_method };
      } else {
        hc = { type: 'exec', command: form.hc_command };
      }
      // Tuning: 0 / empty is passed as-is so backend defaults keep applying
      hc.interval_secs = form.hc_interval !== '' ? Number(form.hc_interval) : undefined;
      hc.timeout_secs = form.hc_timeout !== '' ? Number(form.hc_timeout) : undefined;
      hc.start_period_secs = form.hc_start_period !== '' ? Number(form.hc_start_period) : undefined;
      hc.max_failures = form.hc_max_failures !== '' ? Number(form.hc_max_failures) : undefined;
      payload.health_check = hc;
    } else {
      // Send disabled to clear health check config
      // @ts-ignore
      payload.health_check = { type: 'disabled' };
    }

    // Resource limits: only when isolation plugin is available.
    // Without the capability, omit the field so existing limits are left unchanged.
    if (caps.isolation) {
      if (showLimits.value) {
        payload.resource_limits = {
          cpu_quota: form.cpu_quota ? form.cpu_quota : -1.0,
          memory_limit: form.memory_mb ? form.memory_mb : 0,
          memory_warn_percent: form.memory_warn_percent ?? 0,
          memory_warn_headroom: form.memory_warn_headroom ?? 0,
          memory_high: form.memory_high ?? 0
        };
      } else {
        payload.resource_limits = {
          cpu_quota: -1.0,
          memory_limit: 0,
          memory_warn_percent: 0,
          memory_warn_headroom: 0,
          memory_high: 0
        };
      }
    }

    // Artifact: Enable off (or incomplete) sends clear-sentinel; omit is NOT enough
    // because UpdateProgramRequest treats missing fields as "leave unchanged".
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
    } else {
      payload.artifact = {
        source: '',
        checksum: '',
        extract: false,
        destination: '',
        restart_policy: 'immediate',
        download_timeout: 60,
        verify_timeout: 60,
      };
    }

    const url = API_PATHS.PROGRAMS.DETAIL(programId);
    await apiClient.put(url, payload);

    await alertDialog(
      'Program updated successfully. You may need to restart it for changes to take effect.',
      { title: 'Updated' },
    );
    router.push(`/programs/${programId}`);
  } catch (e: any) {
    submitError.value = formatApiError(e);
  } finally {
    submitting.value = false;
  }
}


// --------------------------------------------------------------- sections

const sections = [
  { key: 'run', label: 'Run', required: true },
  { key: 'lifecycle', label: 'Lifecycle', required: false },
  { key: 'scheduling', label: 'Scheduling', required: false },
  { key: 'health', label: 'Health', required: false },
  { key: 'updates', label: 'Updates', required: false },
  { key: 'hooks', label: 'Hooks', required: false },
  { key: 'logs', label: 'Logs', required: false },
  { key: 'dependencies', label: 'Depends on', required: false },
  { key: 'limits', label: 'Limits', required: false },
] as const;

// The Limits section only renders with the isolation plugin.
const visibleSections = computed(() =>
  sections.filter((section) => section.key !== 'limits' || caps.isolation),
);

const sectionDone = computed<Record<string, boolean>>(() => ({
  run: !!form.command.trim(),
  lifecycle: true,
  scheduling: form.enableCron && !!form.cron.trim(),
  health: form.enableHealthCheck,
  updates: form.enableArtifact,
  hooks: form.enableHooks && Object.values(form.hooks).some((value) => !!value.trim()),
  logs: form.enableLogs,
  dependencies: form.depends_on.some((d) => !!d.value.trim()),
  limits: showLimits.value && !!(form.cpu_quota || form.memory_mb),
}));

const railRef = ref<InstanceType<typeof FormAnchorRail> | null>(null);
function scrollToSection(key: string) {
  railRef.value?.scrollToSection(key);
}

</script>

<template>
  <div v-if="loading" class="flex flex-col items-center justify-center py-20 gap-4">
    <span class="inline-block w-8 h-8 border-3 border-foreground/20 border-t-foreground/50 rounded-full animate-spin"></span>
    <span class="text-foreground/50 font-medium">Loading configuration...</span>
  </div>

  <div v-else class="lg:grid lg:grid-cols-[13rem_1fr] lg:gap-6">
    <FormAnchorRail
      :sections="visibleSections"
      :done="sectionDone"
      title="Edit Program"
      :subtitle="programId"
      @navigate="scrollToSection"
      @back="router.push(`/programs/${programId}`)"
    />

    <div class="min-w-0">
      <UiNotice v-if="submitError" tone="danger" class="mb-6">{{ submitError }}</UiNotice>

      <div class="space-y-6 pb-24">
        <!-- RUN -->
        <section id="section-run" data-section="run" class="overflow-visible rounded-2xl bg-card ring-1 ring-border/60 shadow-[0_1px_2px_hsl(220_20%_20%/0.04),0_2px_8px_hsl(220_20%_20%/0.03)]">
          <div class="p-6">
            <h2 class="type-eyebrow text-muted-foreground mb-4 flex items-center gap-2">
              Run
              <span class="text-destructive">*</span>
              <a :href="docRef('identity--execution')" target="_blank" class="inline-flex text-foreground/25 hover:text-primary transition-colors" title="Docs: Identity & execution"><ExternalLink class="w-3 h-3" /></a>
            </h2>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Program Name<a :href="docRef('identity--execution')" target="_blank" class="inline-flex ml-1 text-foreground/25 hover:text-primary align-text-bottom" title="Docs"><ExternalLink class="w-3 h-3" /></a></span></label>
                <input v-model="form.name" type="text" class="field-control" />
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Group<a :href="docRef('identity--execution')" target="_blank" class="inline-flex ml-1 text-foreground/25 hover:text-primary align-text-bottom" title="Docs"><ExternalLink class="w-3 h-3" /></a></span></label>
                <input v-model="form.group" type="text" class="field-control" />
              </div>
            </div>
            <div class="form-control mt-4">
              <label class="label pt-0 pb-2 justify-start gap-2">
                <span class="label-text text-xs font-medium text-foreground/75">Command<a :href="docRef('identity--execution')" target="_blank" class="inline-flex ml-1 text-foreground/25 hover:text-primary align-text-bottom" title="Docs"><ExternalLink class="w-3 h-3" /></a></span>
                <span class="text-destructive text-xs">*</span>
              </label>
              <div class="relative flex gap-1">
                <div class="relative flex-1">
                  <Terminal class="w-4 h-4 absolute left-3 top-3 text-foreground/40" />
                  <input v-model="form.command" type="text" class="field-control !pl-10 font-mono text-sm w-full" />
                </div>
                <button
                  v-if="commandHasArgs"
                  class="inline-flex items-center justify-center rounded-[10px] font-medium text-xs h-10 px-2 text-warning/70 hover:text-warning hover:bg-warning/10 transition-colors"
                  title="Split command into command + arguments"
                  @click="autoSplitCommand"
                >
                  <Zap class="w-3.5 h-3.5" />
                </button>
              </div>
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
                  <button type="button" class="inline-flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors" @click="removeArg(idx)"><Trash2 class="w-3.5 h-3.5" /></button>
                </div>
              </div>
              <button type="button" class="inline-flex items-center px-2.5 py-1.5 rounded-lg text-xs border border-dashed border-border text-muted-foreground hover:text-foreground hover:bg-muted transition-colors w-full mt-3" @click="addArg">+ Add Argument</button>
            </div>
            <div v-if="form.command.trim()" class="mt-3 rounded-xl bg-muted/40 px-3 py-2">
              <span class="text-[11px] uppercase tracking-wider text-muted-foreground/70 font-semibold">Preview</span>
              <code class="block mt-1 text-xs font-mono text-foreground/80 break-all">{{ fullCommandLine || '(empty)' }}</code>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-3 gap-3 mt-4">
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Working Directory<a :href="docRef('identity--execution')" target="_blank" class="inline-flex ml-1 text-foreground/25 hover:text-primary align-text-bottom" title="Docs"><ExternalLink class="w-3 h-3" /></a></span></label>
                <input v-model="form.cwd" type="text" class="field-control font-mono text-sm" />
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Run As User<a :href="docRef('identity--execution')" target="_blank" class="inline-flex ml-1 text-foreground/25 hover:text-primary align-text-bottom" title="Docs"><ExternalLink class="w-3 h-3" /></a></span></label>
                <input v-model="form.user" type="text" class="field-control font-mono text-sm" />
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Run As Group</span></label>
                <input v-model="form.group" type="text" class="field-control font-mono text-sm" />
              </div>
            </div>

            <div class="mt-4 rounded-xl bg-muted/40 p-3">
              <p class="type-eyebrow text-muted-foreground mb-2">Environment</p>
              <div class="space-y-2">
                <div v-for="(item, idx) in form.env" :key="idx" class="grid grid-cols-[1fr_1fr_auto] gap-2">
                  <input v-model="item.key" placeholder="KEY" class="field-control field-control-sm font-mono" />
                  <input v-model="item.value" placeholder="VALUE" class="field-control field-control-sm font-mono" />
                  <button class="inline-flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors" @click="removeEnv(idx)"><Trash2 class="w-3.5 h-3.5" /></button>
                </div>
                <button class="inline-flex items-center px-2.5 py-1 rounded-lg text-xs border border-dashed border-border text-muted-foreground hover:text-foreground hover:bg-muted transition-colors w-full" @click="addEnv">+ Add Variable</button>
              </div>
            </div>
          </div>
        </section>

        <!-- LIFECYCLE -->
        <section id="section-lifecycle" data-section="lifecycle" class="overflow-visible rounded-2xl bg-card ring-1 ring-border/60 shadow-[0_1px_2px_hsl(220_20%_20%/0.04),0_2px_8px_hsl(220_20%_20%/0.03)]">
          <div class="p-6">
            <h2 class="type-eyebrow text-muted-foreground mb-4 flex items-center gap-2">
              Lifecycle
              <a :href="docRef('restart--stop-behaviour')" target="_blank" class="inline-flex text-foreground/25 hover:text-primary transition-colors" title="Docs"><ExternalLink class="w-3 h-3" /></a>
            </h2>
            <label class="mb-4 flex items-center justify-between cursor-pointer group">
              <span class="font-medium text-sm group-hover:text-primary transition-colors">Autostart</span>
              <UiSwitch v-model="form.autostart" />
            </label>
            <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Retry Limit</span></label>
                <input v-model="form.retry_limit" type="number" min="0" class="field-control" />
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Startsecs</span></label>
                <input v-model="form.startsecs" type="number" min="0" class="field-control" placeholder="10" />
                <span class="text-xs text-muted-foreground mt-0.5">Sec to mark stable</span>
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Stopsecs</span></label>
                <input v-model.number="form.stopsecs" type="number" min="0" class="field-control" placeholder="global timeout" />
                <span class="text-xs text-muted-foreground mt-0.5">SIGTERM → SIGKILL wait</span>
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Priority</span></label>
                <input v-model="form.priority" type="number" class="field-control" placeholder="999" />
                <span class="text-xs text-muted-foreground mt-0.5">Lower = earlier</span>
              </div>
            </div>
            <div class="mt-4 grid grid-cols-1 md:grid-cols-2 gap-3">
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Autorestart Policy</span></label>
                <select v-model="form.autorestart" class="field-control">
                  <option value="unexpected">unexpected — only on non-zero / unknown exits</option>
                  <option value="true">true — always restart</option>
                  <option value="false">false — never restart</option>
                </select>
              </div>
            </div>
            <div class="form-control mt-4 max-w-sm">
              <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Exit Codes</span></label>
              <p class="text-xs text-muted-foreground mb-2">Expected success codes. Defaults to <code class="bg-muted px-1 rounded">[0]</code>.</p>
              <div class="space-y-2">
                <div v-for="(ec, idx) in form.exitcodes" :key="idx" class="flex gap-2">
                  <input v-model.number="ec.value" type="number" class="field-control font-mono" placeholder="0" />
                  <button class="inline-flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors" @click="removeExitcode(idx)"><Trash2 class="w-3.5 h-3.5" /></button>
                </div>
                <button class="inline-flex items-center px-2.5 py-1 rounded-lg text-xs border border-dashed border-border text-muted-foreground hover:text-foreground hover:bg-muted transition-colors w-full" @click="addExitcode">+ Add Exit Code</button>
              </div>
            </div>
          </div>
        </section>

        <!-- SCHEDULING -->
        <section id="section-scheduling" data-section="scheduling">
          <ProgramSectionCard
            v-model="form.enableCron"
            title="Scheduling"
            hint="No cron schedule"
            :doc-href="docRef('orchestration')"
            doc-title="Docs: Orchestration"
          >
            <template #icon><Clock /></template>
            <div class="form-control">
              <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Expression</span></label>
              <input v-model="form.cron" type="text" placeholder="0 * * * * *" class="field-control font-mono" />
            </div>
            <div class="grid grid-cols-2 gap-3">
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Max Concurrent</span></label>
                <input v-model="form.max_concurrent" type="number" min="0" max="64" placeholder="1 (default)" class="field-control" />
                <span class="text-xs text-muted-foreground mt-0.5">Overlapping runs allowed</span>
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Max Queued</span></label>
                <input v-model="form.max_queued" type="number" min="0" max="10000" placeholder="100 (default)" class="field-control" />
                <span class="text-xs text-muted-foreground mt-0.5">Cap queued firings</span>
              </div>
            </div>
            <div class="form-control">
              <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Kill After (secs)</span></label>
              <input v-model="form.kill_after_secs" type="number" min="0" placeholder="Disabled" class="field-control" />
              <span class="text-xs text-muted-foreground mt-0.5">Terminate a run exceeding this wall-clock time (SIGTERM, then SIGKILL after stopsecs)</span>
            </div>
            <div class="grid grid-cols-2 gap-3">
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">On Overlap</span></label>
                <select v-model="form.on_overlap" class="field-control">
                  <option value="skip">skip — drop tick</option>
                  <option value="queue">queue — run after exit</option>
                  <option value="kill">kill — terminate running</option>
                </select>
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Catchup</span></label>
                <select v-model="form.catchup" class="field-control">
                  <option value="skip">skip — no backfill</option>
                  <option value="latest">latest — last missed</option>
                  <option value="all">all — every missed slot</option>
                </select>
              </div>
            </div>
            <div class="form-control">
              <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Jitter (s)</span></label>
              <input v-model="form.jitter_sec" type="number" min="0" placeholder="0" class="field-control" />
              <span class="text-xs text-muted-foreground mt-0.5">Random delay before each trigger</span>
            </div>
          </ProgramSectionCard>
        </section>

        <!-- HEALTH -->
        <section id="section-health" data-section="health">
          <ProgramSectionCard
            v-model="form.enableHealthCheck"
            title="Health Check"
            hint="No probe configured"
            :doc-href="docRef('programshealth_check')"
            doc-title="Docs: Health checks"
          >
            <template #icon><Activity /></template>
            <UiNotice v-if="isSignalRestartPolicy" tone="warning" class="mb-4">
              Required for signal restart policy — use a real TCP/HTTP/exec probe, not <code class="bg-muted px-1 rounded">true</code>.
            </UiNotice>
            <div class="space-y-6">
              <div>
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Check Strategy</span></label>
                <div class="grid grid-cols-3 gap-3">
                  <button
                    type="button"
                    class="flex flex-col items-center justify-center p-4 rounded-xl border text-center transition-colors"
                    :class="form.healthCheckType === 'tcp'
                      ? 'border-primary bg-primary/10 text-primary'
                      : 'border-border bg-muted/40 text-foreground hover:bg-muted/40'"
                    @click="form.healthCheckType = 'tcp'"
                  >
                    <span class="text-sm font-medium">TCP Port</span>
                    <span class="text-xs opacity-60 mt-1">Connect to socket</span>
                  </button>
                  <button
                    type="button"
                    class="flex flex-col items-center justify-center p-4 rounded-xl border text-center transition-colors"
                    :class="form.healthCheckType === 'http'
                      ? 'border-primary bg-primary/10 text-primary'
                      : 'border-border bg-muted/40 text-foreground hover:bg-muted/40'"
                    @click="form.healthCheckType = 'http'"
                  >
                    <span class="text-sm font-medium">HTTP Request</span>
                    <span class="text-xs opacity-60 mt-1">Expect 2xx status</span>
                  </button>
                  <button
                    type="button"
                    class="flex flex-col items-center justify-center p-4 rounded-xl border text-center transition-colors"
                    :class="form.healthCheckType === 'exec'
                      ? 'border-primary bg-primary/10 text-primary'
                      : 'border-border bg-muted/40 text-foreground hover:bg-muted/40'"
                    @click="form.healthCheckType = 'exec'"
                  >
                    <span class="text-sm font-medium">Shell Command</span>
                    <span class="text-xs opacity-60 mt-1">Exit code 0</span>
                  </button>
                </div>
              </div>

              <div class="min-h-[6.5rem]">
                <div v-show="form.healthCheckType === 'tcp'" class="grid grid-cols-1 md:grid-cols-2 gap-6">
                  <div class="form-control">
                    <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Host</span></label>
                    <input v-model="form.hc_host" class="field-control font-mono text-sm" placeholder="127.0.0.1" />
                  </div>
                  <div class="form-control">
                    <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Port</span></label>
                    <input v-model="form.hc_port" type="number" class="field-control font-mono text-sm" placeholder="8080" />
                  </div>
                </div>

                <div v-show="form.healthCheckType === 'http'" class="grid grid-cols-1 md:grid-cols-12 gap-4">
                  <div class="form-control md:col-span-3">
                    <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Method</span></label>
                    <select v-model="form.hc_method" class="field-control font-mono text-sm">
                      <option>GET</option>
                      <option>POST</option>
                      <option>HEAD</option>
                    </select>
                  </div>
                  <div class="form-control md:col-span-9">
                    <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">URL</span></label>
                    <input v-model="form.hc_url" class="field-control font-mono text-sm" placeholder="http://127.0.0.1:8080/health" />
                  </div>
                </div>

                <div v-show="form.healthCheckType === 'exec'" class="space-y-3">
                  <div class="form-control">
                    <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Command (Exit 0 = Healthy)</span></label>
                    <textarea v-model="form.hc_command" class="field-control font-mono text-sm h-24 py-2 leading-relaxed resize-y" placeholder="curl -f http://localhost:8080 || exit 1"></textarea>
                  </div>
                </div>
              </div>

              <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div class="form-control">
                  <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Probe Interval (s)</span></label>
                  <input v-model="form.hc_interval" type="number" min="0" class="field-control" placeholder="10 (default)" />
                </div>
                <div class="form-control">
                  <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Probe Timeout (s)</span></label>
                  <input v-model="form.hc_timeout" type="number" min="0" class="field-control" placeholder="0 (default)" />
                </div>
                <div class="form-control">
                  <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Start Period (s)</span></label>
                  <input v-model="form.hc_start_period" type="number" min="0" class="field-control" placeholder="0" />
                </div>
                <div class="form-control">
                  <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Max Failures</span></label>
                  <input v-model="form.hc_max_failures" type="number" min="0" class="field-control" placeholder="3 (default)" />
                </div>
              </div>
              <p class="text-xs text-muted-foreground">0 / empty = backend default. <code class="bg-muted px-1 rounded">max_failures 0</code> disables health-triggered auto-restart.</p>
            </div>
          </ProgramSectionCard>
        </section>

        <!-- UPDATES (OTA) -->
        <section id="section-updates" data-section="updates">
          <ProgramSectionCard
            v-model="form.enableArtifact"
            title="OTA Artifact"
            hint="No artifact configured"
            doc-href="https://super.docs.sconts.com/docs/04-production-scenarios/delivery/fail-safe-ota/"
            doc-title="Docs: Fail-Safe OTA"
          >
            <template #icon><Zap /></template>
            <p class="text-xs text-muted-foreground">Define an artifact to download and replace the binary before the next restart.</p>
            <div class="form-control">
              <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Source URL</span></label>
              <input v-model="form.artifact_source" type="url" placeholder="https://artifacts.example.com/releases/my-app.tar.gz" class="field-control font-mono" />
            </div>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Checksum</span></label>
                <input v-model="form.artifact_checksum" type="text" placeholder="sha256:abc123..." class="field-control font-mono" />
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Destination Path</span></label>
                <input v-model="form.artifact_destination" type="text" placeholder="/opt/my-app" class="field-control font-mono" />
              </div>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
              <label class="flex items-center justify-between cursor-pointer group">
                <span class="font-medium text-sm group-hover:text-primary transition-colors">Extract archive</span>
                <UiSwitch v-model="form.artifact_extract" />
              </label>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Restart Policy</span></label>
                <select v-model="form.artifact_restart_policy" class="field-control">
                  <option value="immediate">immediate — restart process (SIGTERM), then verify</option>
                  <option value="manual">manual — commit swap only (no restart / no verify)</option>
                  <option value="signal:hup">signal:hup — in-place reload (requires health check)</option>
                  <option value="signal:int">signal:int — in-place reload (requires health check)</option>
                  <option value="signal:term">signal:term — in-place reload (requires health check)</option>
                  <option value="signal:quit">signal:quit — in-place reload (requires health check)</option>
                  <option value="signal:usr1">signal:usr1 — in-place reload (requires health check)</option>
                  <option value="signal:usr2">signal:usr2 — in-place reload (requires health check)</option>
                </select>
                <UiNotice v-if="isSignalRestartPolicy" tone="warning" class="mt-2">
                  Hot-reload does not exec a new process. An enabled Health Check is required and will be turned on automatically.
                </UiNotice>
              </div>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Download timeout (s)</span></label>
                <input v-model.number="form.artifact_download_timeout" type="number" min="0" placeholder="60" class="field-control font-mono" />
                <p class="text-xs text-muted-foreground mt-1">HTTP download deadline. <code class="text-[10px]">0</code> disables overall transfer timeout.</p>
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Verify timeout (s)</span></label>
                <input v-model.number="form.artifact_verify_timeout" type="number" min="0" placeholder="60" class="field-control font-mono" />
                <p class="text-xs text-muted-foreground mt-1">Post-swap health window before auto-rollback. <code class="text-[10px]">0</code> disables.</p>
              </div>
            </div>
          </ProgramSectionCard>
        </section>

        <!-- HOOKS -->
        <section id="section-hooks" data-section="hooks">
          <ProgramSectionCard
            v-model="form.enableHooks"
            title="Hooks"
            hint="No hook scripts"
            :doc-href="docRef('programshooks')"
            doc-title="Docs: Hooks"
          >
            <template #icon><Activity /></template>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Pre-Start</span></label>
                <textarea v-model="form.hooks.pre_start" class="field-control font-mono h-24 py-2 leading-relaxed resize-y" placeholder="Scripts to run before process spawn..."></textarea>
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Post-Start</span></label>
                <textarea v-model="form.hooks.post_start" class="field-control font-mono h-24 py-2 leading-relaxed resize-y" placeholder="Scripts to run after process spawns..."></textarea>
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Pre-Stop</span></label>
                <textarea v-model="form.hooks.pre_stop" class="field-control font-mono h-24 py-2 leading-relaxed resize-y" placeholder="Scripts to run before sending kill signal..."></textarea>
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Post-Stop</span></label>
                <textarea v-model="form.hooks.post_stop" class="field-control font-mono h-24 py-2 leading-relaxed resize-y" placeholder="Scripts to run after process exits..."></textarea>
              </div>
            </div>
          </ProgramSectionCard>
        </section>

        <!-- LOGS -->
        <section id="section-logs" data-section="logs">
          <ProgramSectionCard
            v-model="form.enableLogs"
            title="Log Files"
            hint="Auto (managed in log dir)"
            :doc-href="docRef('logging-1')"
            doc-title="Docs: Logging"
          >
            <template #icon><Box /></template>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Stdout Log File</span></label>
                <input v-model="form.stdout_logfile" type="text" placeholder="auto (managed in log dir)" class="field-control font-mono" />
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Stderr Log File</span></label>
                <input v-model="form.stderr_logfile" type="text" placeholder="auto (managed in log dir)" class="field-control font-mono" />
              </div>
            </div>
          </ProgramSectionCard>
        </section>

        <!-- DEPENDENCIES -->
        <section id="section-dependencies" data-section="dependencies">
          <ProgramDependencyFields
            v-model="form.depends_on"
            :exclude-name="form.name"
            :doc-href="docRef('orchestration')"
          />
        </section>

        <!-- LIMITS -->
        <section v-if="caps.isolation" id="section-limits" data-section="limits">
          <ProgramSectionCard
            v-model="showLimits"
            title="Resource Limits"
            hint="No limits configured"
            :doc-href="docRef('programsresource_limits-')"
            doc-title="Docs: Resource limits"
          >
            <template #icon><Cpu /></template>
            <div class="grid grid-cols-2 gap-3">
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75 flex items-center gap-1"><Cpu class="w-3 h-3" /> CPU (cores)</span></label>
                <input v-model="form.cpu_quota" type="number" step="0.1" placeholder="1.0" class="field-control" />
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75 flex items-center gap-1"><Box class="w-3 h-3" /> Mem (MB)</span></label>
                <input v-model="form.memory_mb" type="number" placeholder="512" class="field-control" />
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Warn %</span></label>
                <input v-model="form.memory_warn_percent" type="number" min="0" max="100" placeholder="80" class="field-control" />
              </div>
              <div class="form-control">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Warn Headroom (MB)</span></label>
                <input v-model="form.memory_warn_headroom" type="number" placeholder="0" class="field-control" />
              </div>
              <div class="form-control col-span-2">
                <label class="label pt-0 pb-2"><span class="label-text text-xs font-medium text-foreground/75">Mem High (MB, soft)</span></label>
                <input v-model="form.memory_high" type="number" placeholder="0" class="field-control" />
              </div>
            </div>
          </ProgramSectionCard>
        </section>
      </div>

      <!-- Sticky action bar -->
      <div class="sticky bottom-0 z-10 -mx-4 border-t border-border/40 bg-background/95 px-4 py-3 backdrop-blur-sm sm:-mx-6 sm:px-6">
        <UiNotice v-if="submitError" tone="danger" class="mb-3">{{ submitError }}</UiNotice>
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div class="min-w-0 text-xs text-muted-foreground">
            <span class="font-medium text-foreground">{{ form.name || 'unnamed' }}</span>
            <span v-if="fullCommandLine" class="ml-2 hidden truncate font-mono sm:inline">{{ fullCommandLine }}</span>
          </div>
          <UiButton :disabled="submitting" @click="handleSubmit">
            <span v-if="submitting" class="inline-block h-3 w-3 animate-spin rounded-full border-2 border-primary-foreground/30 border-t-primary-foreground"></span>
            <Save v-else class="h-4 w-4" />
            Update Configuration
          </UiButton>
        </div>
      </div>
    </div>
  </div>
</template>
