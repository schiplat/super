<script setup lang="ts">
/**
 * Authentication settings, composed after buz's SSO page: a status strip,
 * then a two-column grid — the provider connection (and the registration
 * URIs) in the main column, the access policy in the 320px sidebar. The role
 * rules, sessions, and audit log follow full-width below.
 *
 * The client secret is write-only — the API never echoes it back; typing one
 * replaces the stored value on the next "Save & apply".
 */
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ArrowDown, ArrowUp, Check, Copy, Plus, Trash2 } from 'lucide-vue-next'
import apiClient from '@/api/client'
import { useCapabilitiesStore } from '@/stores/capabilities'
import BasePagination from '@/components/BasePagination.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiDataTable from '@/components/ui/UiDataTable.vue'
import type { DataColumn } from '@/types/ui'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiField from '@/components/ui/UiField.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiSection from '@/components/ui/UiSection.vue'
import UiStatusChip from '@/components/ui/UiStatusChip.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'
import type {
  RoleRule,
  SsoAuditEvent,
  SsoConfig,
  SsoConfigView,
  SsoGate,
  SsoProbe,
  SsoRole,
  SsoSession,
} from '@/types/sso'

const caps = useCapabilitiesStore()
const router = useRouter()

interface RuleDraft {
  sub: string
  email: string
  groups: string
  role: SsoRole
}

const view = ref<SsoConfigView | null>(null)
const config = ref<SsoConfig>(emptyConfig())
const rules = ref<RuleDraft[]>([])
const secret = ref('')
const replacingSecret = ref(false)
const sessionHours = ref('12')
const idleMinutes = ref('60')

const loading = ref(true)
const saving = ref(false)
const testing = ref(false)
const disabling = ref(false)
const showDisable = ref(false)

const errors = ref<string[]>([])
const notice = ref<{ tone: 'info' | 'success' | 'warning' | 'danger'; text: string } | null>(null)
const probe = ref<SsoProbe | null>(null)

const sessions = ref<SsoSession[]>([])
const audit = ref<SsoAuditEvent[]>([])
const auditPage = ref(1)
const auditPageSize = ref(20)
const auditTotal = ref(0)
const auditLoading = ref(false)
const copiedKey = ref('')

/** Live gate facts from /auth/status (secret source) — best-effort probe. */
const authStatus = ref<{ token_management: boolean; secret_source: string | null } | null>(null)

const breakGlassLabel = computed(() => {
  switch (authStatus.value?.secret_source) {
    case 'config':
      return 'set in conf/super.toml'
    case 'generated':
      return 'auto-generated · data/auth_secret.generated'
    case 'placeholder':
      return 'loopback placeholder only'
    default:
      return 'not configured'
  }
})

const pluginOwnsAuth = computed(() => caps.security)
const gate = computed<SsoGate>(() => view.value?.gate ?? 'none')
const gateLabel: Record<SsoGate, string> = {
  none: 'No authentication',
  secret: 'Static auth_secret',
  plugin: 'Security plugin',
  sso: 'SSO (OIDC)',
}
const gateTone = computed(
  () =>
    ({
      none: 'warning',
      secret: 'info',
      plugin: 'info',
      sso: 'success',
    })[gate.value] as 'warning' | 'info' | 'success',
)
const secretStored = computed(() => !!view.value?.secret_updated_at)
const ssoConfigured = computed(() => !!view.value?.configured)

const redirectUri = computed(
  () =>
    view.value?.redirect_uri ??
    `${config.value.public_base_url.replace(/\/$/, '')}/api/v1/auth/sso/callback`,
)
const backchannelUri = computed(
  () =>
    view.value?.backchannel_logout_uri ??
    `${config.value.public_base_url.replace(/\/$/, '')}/api/v1/auth/sso/backchannel-logout`,
)
const postLogoutUri = computed(
  () =>
    view.value?.post_logout_redirect_uri ??
    `${config.value.public_base_url.replace(/\/$/, '')}/login?signedout=1`,
)

/** The three URIs an IdP app registration needs. */
const registrationUris = computed(() => [
  { key: 'callback', label: 'Callback', what: 'sign-in', value: redirectUri.value },
  {
    key: 'backchannel',
    label: 'Back-channel logout',
    what: 'instant revocation',
    value: backchannelUri.value,
  },
  {
    key: 'landing',
    label: 'Post-logout landing',
    what: 'unified sign-out',
    value: postLogoutUri.value,
  },
])

/** null = unknown: neither a probe nor cached discovery answered yet. */
const endSessionAvailable = computed<boolean | null>(() => {
  if (probe.value?.ok) return probe.value.end_session_available === true
  if (view.value?.end_session_available !== null && view.value?.end_session_available !== undefined)
    return view.value.end_session_available
  return null
})

const sessionColumns: DataColumn[] = [
  { key: 'user', label: 'User' },
  { key: 'role', label: 'Role' },
  { key: 'seen', label: 'Last seen' },
  { key: 'expires', label: 'Expires' },
  { key: 'actions', label: '', align: 'right' },
]

const auditColumns: DataColumn[] = [
  { key: 'time', label: 'Time' },
  { key: 'kind', label: 'Event' },
  { key: 'user', label: 'User' },
  { key: 'ip', label: 'IP' },
  { key: 'detail', label: 'Detail' },
]

function emptyConfig(): SsoConfig {
  return {
    enabled: false,
    issuer: '',
    client_id: '',
    public_base_url: typeof window === 'undefined' ? '' : window.location.origin,
    scopes: 'openid profile email',
    groups_claim: 'groups',
    token_auth_method: 'client_secret_basic',
    role_rules: [],
    session_ttl_secs: 12 * 60 * 60,
    idle_timeout_secs: 60 * 60,
    allow_insecure_issuer: false,
  }
}

function apiError(error: any): string {
  const data = error?.response?.data
  if (Array.isArray(data?.errors) && data.errors.length) return data.errors.join('; ')
  return data?.message || data?.error || error?.message || 'Request failed'
}

function formatTime(seconds: number | null | undefined): string {
  if (!seconds) return '—'
  return new Date(seconds * 1000).toLocaleString()
}

function shortSub(sub: string): string {
  return sub.length > 14 ? `${sub.slice(0, 14)}…` : sub
}

function rulesFromConfig(config: SsoConfig): RuleDraft[] {
  return config.role_rules.map((rule) => ({
    sub: rule.match.sub ?? '',
    email: rule.match.email ?? '',
    groups: (rule.match.groups_any ?? []).join(', '),
    role: rule.role,
  }))
}

function rulesPayload(): RoleRule[] {
  return rules.value.map((rule) => {
    const match: RoleRule['match'] = {}
    if (rule.sub.trim()) match.sub = rule.sub.trim()
    if (rule.email.trim()) match.email = rule.email.trim()
    const groups = rule.groups
      .split(',')
      .map((group) => group.trim())
      .filter(Boolean)
    if (groups.length) match.groups_any = groups
    return { match, role: rule.role }
  })
}

async function load() {
  loading.value = true
  notice.value = null
  try {
    const [configResponse, sessionsResponse, auditResponse] = await Promise.all([
      apiClient.get<SsoConfigView>('/api/v1/auth/sso/config'),
      apiClient.get<{ sessions: SsoSession[] }>('/api/v1/auth/sso/sessions'),
      apiClient.get<{ events: SsoAuditEvent[]; total: number }>('/api/v1/auth/sso/audit', {
        params: { page: auditPage.value, page_size: auditPageSize.value },
      }),
    ])
    view.value = configResponse.data
    config.value = configResponse.data.config ?? emptyConfig()
    rules.value = rulesFromConfig(config.value)
    sessionHours.value = String(Math.round(config.value.session_ttl_secs / 3600))
    idleMinutes.value = String(Math.round(config.value.idle_timeout_secs / 60))
    sessions.value = sessionsResponse.data.sessions ?? []
    audit.value = auditResponse.data.events ?? []
    auditTotal.value = auditResponse.data.total ?? 0
  } catch (error) {
    notice.value = { tone: 'danger', text: apiError(error) }
  } finally {
    loading.value = false
  }

  // Gate facts are a separate, best-effort probe: `/auth/status` answers 404
  // when no gate exists (loopback dev), which must not fail the page.
  try {
    const statusResponse = await apiClient.get('/api/v1/auth/status')
    authStatus.value = {
      token_management: statusResponse.data?.token_management === true,
      secret_source: statusResponse.data?.secret_source ?? null,
    }
  } catch {
    authStatus.value = { token_management: false, secret_source: null }
  }
}

async function save() {
  errors.value = []
  notice.value = null
  probe.value = null

  const hours = Number(sessionHours.value)
  const minutes = Number(idleMinutes.value)
  if (!Number.isFinite(hours) || hours <= 0 || !Number.isFinite(minutes) || minutes <= 0) {
    errors.value = ['Session lifetime and idle timeout must be positive numbers.']
    return
  }
  const roleRules = rulesPayload()
  if (roleRules.some((rule) => Object.keys(rule.match).length === 0)) {
    errors.value = ['Every role rule needs at least one selector (sub, email, or group).']
    return
  }
  if (config.value.enabled && roleRules.length === 0) {
    errors.value = ['Enabling SSO needs at least one role rule; otherwise nobody can sign in.']
    return
  }

  saving.value = true
  try {
    // A typed secret always replaces the stored one (write-only field).
    if (secret.value) {
      await apiClient.post('/api/v1/auth/sso/secret', { secret: secret.value })
      secret.value = ''
      replacingSecret.value = false
    }
    const payload: SsoConfig = {
      ...config.value,
      role_rules: roleRules,
      session_ttl_secs: Math.round(hours * 3600),
      idle_timeout_secs: Math.round(minutes * 60),
    }
    const response = await apiClient.put<SsoConfigView>('/api/v1/auth/sso/config', payload)
    view.value = response.data
    config.value = response.data.config ?? payload
    rules.value = rulesFromConfig(config.value)

    // Enabling SSO switches the gate off the credential this browser used.
    // Continuing would just produce 401s, so hand over to the SSO login page.
    if (response.data.gate === 'sso') {
      localStorage.removeItem('super_token')
      await router.replace('/login')
      return
    }

    notice.value = {
      tone: 'success',
      text: secretStored.value
        ? 'Saved and applied.'
        : 'Saved. SSO is configured but disabled.',
    }
    await refreshLists()
  } catch (error: any) {
    const data = error?.response?.data
    errors.value = Array.isArray(data?.errors) ? data.errors : []
    if (!errors.value.length) notice.value = { tone: 'danger', text: apiError(error) }
  } finally {
    saving.value = false
  }
}

async function testConnection() {
  testing.value = true
  probe.value = null
  try {
    const response = await apiClient.post<SsoProbe>('/api/v1/auth/sso/test')
    probe.value = response.data
  } catch (error) {
    probe.value = { ok: false, error: apiError(error) }
  } finally {
    testing.value = false
  }
}

async function disable() {
  disabling.value = true
  notice.value = null
  try {
    const response = await apiClient.post<{ revoked_sessions: number }>(
      '/api/v1/auth/sso/disable',
    )
    showDisable.value = false
    notice.value = {
      tone: 'success',
      text: `SSO disabled. ${response.data.revoked_sessions} session(s) revoked.`,
    }
    await load()
  } catch (error) {
    notice.value = { tone: 'danger', text: apiError(error) }
  } finally {
    disabling.value = false
  }
}

async function refreshLists() {
  const [sessionsResponse, auditResponse] = await Promise.all([
    apiClient.get<{ sessions: SsoSession[] }>('/api/v1/auth/sso/sessions'),
    apiClient.get<{ events: SsoAuditEvent[]; total: number }>('/api/v1/auth/sso/audit', {
      params: { page: auditPage.value, page_size: auditPageSize.value },
    }),
  ])
  sessions.value = sessionsResponse.data.sessions ?? []
  audit.value = auditResponse.data.events ?? []
  auditTotal.value = auditResponse.data.total ?? 0
}

/** Pagination changes only refetch the audit list, not the whole page. */
async function changeAuditPage(page: number) {
  auditPage.value = page
  auditLoading.value = true
  try {
    const response = await apiClient.get<{ events: SsoAuditEvent[]; total: number }>(
      '/api/v1/auth/sso/audit',
      { params: { page: auditPage.value, page_size: auditPageSize.value } },
    )
    audit.value = response.data.events ?? []
    auditTotal.value = response.data.total ?? 0
  } catch (error) {
    notice.value = { tone: 'danger', text: apiError(error) }
  } finally {
    auditLoading.value = false
  }
}

async function changeAuditPageSize(size: number) {
  auditPageSize.value = size
  await changeAuditPage(1)
}

async function revoke(session: SsoSession) {
  try {
    await apiClient.delete(`/api/v1/auth/sso/sessions/${session.id}`)
    await refreshLists()
  } catch (error) {
    notice.value = { tone: 'danger', text: apiError(error) }
  }
}

async function copyUri(key: string, value: string) {
  try {
    await navigator.clipboard.writeText(value)
    copiedKey.value = key
    window.setTimeout(() => {
      if (copiedKey.value === key) copiedKey.value = ''
    }, 1500)
  } catch {
    // clipboard unavailable — the value stays selectable on screen
  }
}

function scrollToRules() {
  document.getElementById('role-rules')?.scrollIntoView({ block: 'start' })
}

function addRule() {
  rules.value.push({ sub: '', email: '', groups: '', role: 'viewer' })
}

function removeRule(index: number) {
  rules.value.splice(index, 1)
}

function moveRule(index: number, direction: -1 | 1) {
  const target = index + direction
  if (target < 0 || target >= rules.value.length) return
  const [rule] = rules.value.splice(index, 1)
  rules.value.splice(target, 0, rule)
}

onMounted(async () => {
  if (pluginOwnsAuth.value) {
    loading.value = false
    return
  }
  await load()
})
</script>

<template>
  <div class="page-stack">
    <UiNotice
      v-if="pluginOwnsAuth"
      tone="info"
      title="Authentication is managed by the security plugin"
    >
      SSO configuration is unavailable while the commercial security plugin owns auth. Its
      tokens and RBAC apply to this instance.
    </UiNotice>

    <UiNotice v-if="notice" :tone="notice.tone">{{ notice.text }}</UiNotice>

    <div
      v-if="loading"
      class="rounded-2xl bg-card px-6 py-6 text-sm text-muted-foreground ring-1 ring-border/60 shadow-[0_1px_2px_hsl(220_20%_20%/0.04),0_2px_8px_hsl(220_20%_20%/0.03)]"
    >
      Loading…
    </div>

    <template v-else-if="!pluginOwnsAuth">
      <!-- status strip: the gate, the enable switch, break-glass, and the way out -->
      <div
        class="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-2xl bg-card px-5 py-4 ring-1 ring-border/60 shadow-[0_1px_2px_hsl(220_20%_20%/0.04),0_2px_8px_hsl(220_20%_20%/0.03)]"
      >
        <UiStatusChip :tone="gateTone">{{ gateLabel[gate] }}</UiStatusChip>
        <label class="inline-flex items-center gap-2 text-sm font-medium">
          <UiSwitch v-model="config.enabled" aria-label="Enable SSO" />
          Sign-in enabled
        </label>
        <span v-if="ssoConfigured" class="text-xs text-muted-foreground">
          configured {{ formatTime(view?.updated_at) }}
        </span>
        <UiStatusChip v-if="view?.break_glass" tone="neutral">break-glass secret set</UiStatusChip>
        <UiStatusChip v-else tone="warning">no break-glass secret</UiStatusChip>
        <div class="ml-auto">
          <UiButton
            type="button"
            variant="ghost"
            size="sm"
            class="text-destructive hover:bg-destructive/10"
            :disabled="!ssoConfigured || !config.enabled"
            @click="showDisable = true"
          >
            Disable SSO
          </UiButton>
        </div>
      </div>

      <!-- two-column composition: main column + 320px sidebar -->
      <div class="grid items-start gap-6 lg:grid-cols-[minmax(0,1fr)_320px]">
        <!-- main column -->
        <div class="flex min-w-0 flex-col gap-6">
          <UiSection
            title="Provider connection"
            description="Standard OIDC discovery; the issuer must serve /.well-known/openid-configuration."
          >
            <div class="grid gap-3 md:grid-cols-2">
              <UiField label="Issuer URL" required hint="https only, unless it is a loopback dev IdP">
                <UiInput v-model="config.issuer" placeholder="https://idp.example.com" />
              </UiField>
              <UiField label="Client ID" required>
                <UiInput v-model="config.client_id" placeholder="super" />
              </UiField>
              <UiField
                label="Public base URL"
                required
                hint="Origin only — every callback is derived from it"
              >
                <UiInput v-model="config.public_base_url" placeholder="https://super.example.com" />
              </UiField>
              <UiField label="Scopes" hint="Comma or space separated · must include openid">
                <UiInput v-model="config.scopes" placeholder="openid, profile, email" />
              </UiField>
              <UiField label="Groups claim" hint="ID-token claim holding group membership">
                <UiInput v-model="config.groups_claim" placeholder="groups" />
              </UiField>
              <UiField label="Token endpoint auth">
                <select v-model="config.token_auth_method" class="field-control">
                  <option value="client_secret_basic">client_secret_basic</option>
                  <option value="client_secret_post">client_secret_post</option>
                </select>
              </UiField>
            </div>

            <!-- write-only secret: stored values are never echoed back -->
            <div class="mt-4 space-y-2">
              <UiField label="Client secret">
                <UiInput
                  v-if="secretStored && !replacingSecret"
                  type="password"
                  value="····························"
                  disabled
                />
                <UiInput
                  v-else
                  v-model="secret"
                  type="password"
                  autocomplete="new-password"
                  placeholder="Paste once — stored owner-only"
                />
              </UiField>
              <p class="text-xs text-muted-foreground">
                <template v-if="secretStored && !replacingSecret">
                  Stored in data/auth.db, never returned by the API. The previous value stays
                  valid for 15 minutes after a rotation.
                  <button
                    type="button"
                    class="underline hover:text-foreground"
                    @click="replacingSecret = true"
                  >
                    Replace secret
                  </button>
                </template>
                <template v-else-if="secretStored">
                  Leave blank to keep the current one.
                  <button
                    type="button"
                    class="underline hover:text-foreground"
                    @click="replacingSecret = false"
                  >
                    Cancel
                  </button>
                </template>
                <template v-else>No secret stored yet — SSO cannot be enabled without one.</template>
              </p>
            </div>

            <div class="mt-4 flex items-center justify-between gap-4 rounded-xl bg-muted/40 px-3 py-2">
              <div class="min-w-0">
                <p class="text-sm font-medium">Allow plain-http issuer</p>
                <p class="text-xs text-muted-foreground">
                  Only honored for loopback hosts; for local development.
                </p>
              </div>
              <UiSwitch v-model="config.allow_insecure_issuer" aria-label="Allow plain-http issuer" />
            </div>

            <UiNotice v-if="errors.length" tone="danger" title="Configuration rejected" class="mt-4">
              <ul class="list-disc space-y-1 pl-4">
                <li v-for="error in errors" :key="error">{{ error }}</li>
              </ul>
            </UiNotice>

            <!-- probe result (persistent until the next test or save) -->
            <div v-if="probe" class="mt-4 rounded-xl p-4" :class="probe.ok ? 'bg-muted/40' : 'bg-destructive/10'">
              <template v-if="probe.ok">
                <p class="flex items-center gap-1.5 text-sm font-medium text-success">
                  <Check class="h-3.5 w-3.5" />
                  Discovery OK
                </p>
                <div class="mt-2 grid grid-cols-[110px_minmax(0,1fr)] items-baseline gap-x-3 gap-y-1 text-xs">
                  <span class="text-muted-foreground">Issuer match</span>
                  <span>{{
                    probe.issuer_match
                      ? 'yes'
                      : `no — provider reports ${probe.discovered_issuer}`
                  }}</span>
                  <span class="text-muted-foreground">Token endpoint</span>
                  <span class="truncate font-mono" :title="probe.token_endpoint">{{
                    probe.token_endpoint
                  }}</span>
                  <span class="text-muted-foreground">End-session</span>
                  <span>
                    {{
                      probe.end_session_available
                        ? 'available — sign-out ends the provider session'
                        : 'not advertised — sign-out is local only'
                    }}
                  </span>
                  <span class="text-muted-foreground">Client auth</span>
                  <span class="font-mono">{{ probe.client_auth }}</span>
                </div>
              </template>
              <template v-else>
                <p class="text-sm font-medium">Discovery failed</p>
                <p class="mt-1 text-xs text-muted-foreground">{{ probe.error }}</p>
              </template>
            </div>

            <template #footer>
              <div class="flex flex-wrap items-center gap-3">
                <UiButton type="button" :disabled="saving" @click="save">
                  {{ saving ? 'Saving…' : 'Save & apply' }}
                </UiButton>
                <UiButton
                  type="button"
                  variant="secondary"
                  :disabled="testing || !ssoConfigured"
                  @click="testConnection"
                >
                  {{ testing ? 'Testing…' : 'Test connection' }}
                </UiButton>
                <p class="text-xs text-muted-foreground">
                  The enable switch applies on save and takes effect immediately.
                </p>
              </div>
            </template>
          </UiSection>

          <UiSection
            title="Register at your provider"
            description="Copy these into your provider's app registration."
          >
            <div class="space-y-1">
              <div
                v-for="uri in registrationUris"
                :key="uri.key"
                class="-mx-2 flex items-center gap-3 rounded-lg px-2 py-2 transition-colors hover:bg-muted/40"
              >
                <div class="w-40 flex-none">
                  <p class="text-sm font-medium">{{ uri.label }}</p>
                  <p class="text-xs text-muted-foreground">{{ uri.what }}</p>
                </div>
                <code
                  class="min-w-0 flex-1 overflow-hidden overflow-ellipsis whitespace-nowrap rounded-xl bg-muted/40 px-2.5 py-1.5 font-mono text-xs"
                  :title="uri.value"
                >
                  {{ uri.value || 'available after the base URL is saved' }}
                </code>
                <UiButton
                  v-if="uri.value"
                  variant="outline"
                  size="sm"
                  class="w-[30px] px-0"
                  :class="copiedKey === uri.key ? 'text-success' : 'text-muted-foreground'"
                  :title="copiedKey === uri.key ? 'Copied' : 'Copy'"
                  @click="copyUri(uri.key, uri.value)"
                >
                  <Check v-if="copiedKey === uri.key" class="h-3.5 w-3.5" />
                  <Copy v-else class="h-3.5 w-3.5" />
                </UiButton>
              </div>
            </div>

            <details class="mt-2">
              <summary class="cursor-pointer text-xs text-muted-foreground hover:text-foreground">
                If a URL isn't registered
              </summary>
              <ul class="mt-2 list-disc space-y-1 pl-5 text-xs text-muted-foreground">
                <li>Callback missing → sign-in fails.</li>
                <li>
                  Back-channel missing → an IdP-side disable waits for the session's natural
                  expiry instead of revoking immediately.
                </li>
                <li>Landing missing → the browser stays on the provider page after sign-out.</li>
              </ul>
            </details>

            <p v-if="endSessionAvailable !== null" class="mt-3 text-xs text-muted-foreground">
              {{
                endSessionAvailable
                  ? 'The provider supports RP-initiated logout — Sign out ends its session too.'
                  : 'The provider does not advertise an end-session endpoint — Sign out clears the local session only.'
              }}
            </p>
            <p v-else class="mt-3 text-xs text-muted-foreground">
              Run Test connection to check whether sign-out can end the provider session.
            </p>
          </UiSection>
        </div>

        <!-- sidebar (320px) -->
        <div class="flex min-w-0 flex-col gap-6">
          <UiSection
            title="Access policy"
            description="How long sign-ins last, and who gets in."
          >
            <div class="space-y-3">
              <UiField label="Session lifetime (hours)">
                <UiInput v-model="sessionHours" type="number" />
              </UiField>
              <UiField label="Idle timeout (minutes)">
                <UiInput v-model="idleMinutes" type="number" />
              </UiField>

              <div class="rounded-xl bg-muted/40 p-3">
                <p class="text-sm font-medium">
                  {{ rules.length }} role rule{{ rules.length === 1 ? '' : 's' }}
                </p>
                <p class="mt-1 text-xs text-muted-foreground">
                  First match wins; an unmapped account cannot sign in. Both fields apply on
                  Save &amp; apply.
                </p>
                <button
                  type="button"
                  class="mt-2 text-xs font-medium underline hover:text-foreground"
                  @click="scrollToRules"
                >
                  Edit rules below
                </button>
              </div>
            </div>
          </UiSection>

          <UiSection
            title="Break-glass credential"
            description="The static secret behind the login form."
          >
            <div class="space-y-3">
              <div class="rounded-xl bg-muted/40 p-3">
                <p class="text-sm font-medium">{{ breakGlassLabel }}</p>
                <p class="mt-1 text-xs text-muted-foreground">
                  Authenticates as root/Admin — every request is allowed. Dormant once SSO owns
                  the gate; it is the way back in when the SSO store is unreadable.
                </p>
              </div>
              <p class="text-xs leading-snug text-muted-foreground">
                Rotation is a config-file operation: change
                <code class="font-mono">[server].auth_secret</code> in
                <code class="font-mono">conf/super.toml</code> and restart superd. If the SSO
                store cannot be read at startup, superd falls back to this secret — and refuses to
                start when it is missing.
              </p>
            </div>
          </UiSection>
        </div>
      </div>

      <UiSection
        title="Role rules"
        description="First match wins. No match rejects the login; email selectors require a verified address."
      >
        <div id="role-rules" class="scroll-mt-6">
          <div class="space-y-3">
            <div
              v-for="(rule, index) in rules"
              :key="index"
              class="rounded-xl bg-muted/40 p-3"
            >
              <div class="grid gap-3 md:grid-cols-12">
                <UiField label="Subject" class="md:col-span-4">
                  <UiInput v-model="rule.sub" placeholder="immutable sub" />
                </UiField>
                <UiField label="Email" class="md:col-span-3">
                  <UiInput v-model="rule.email" placeholder="requires verification" />
                </UiField>
                <UiField label="Groups (any of)" class="md:col-span-3">
                  <UiInput v-model="rule.groups" placeholder="admins, ops" />
                </UiField>
                <UiField label="Role" class="md:col-span-2">
                  <select v-model="rule.role" class="field-control">
                    <option value="viewer">Viewer</option>
                    <option value="operator">Operator</option>
                    <option value="admin">Admin</option>
                  </select>
                </UiField>
              </div>
              <div class="mt-2 flex items-center justify-end gap-1">
                <button
                  type="button"
                  class="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground disabled:opacity-30"
                  title="Move up"
                  :disabled="index === 0"
                  @click="moveRule(index, -1)"
                >
                  <ArrowUp class="h-4 w-4" />
                </button>
                <button
                  type="button"
                  class="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground disabled:opacity-30"
                  title="Move down"
                  :disabled="index === rules.length - 1"
                  @click="moveRule(index, 1)"
                >
                  <ArrowDown class="h-4 w-4" />
                </button>
                <button
                  type="button"
                  class="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                  title="Remove rule"
                  @click="removeRule(index)"
                >
                  <Trash2 class="h-4 w-4" />
                </button>
              </div>
            </div>
            <div>
              <UiButton type="button" variant="secondary" size="sm" @click="addRule">
                <Plus class="h-4 w-4" />
                Add rule
              </UiButton>
            </div>
          </div>
        </div>
      </UiSection>

      <UiDataTable
        title="Sessions"
        description="Local sessions issued after an SSO login. Revocation is immediate; the provider's back-channel logout revokes by identity."
        :columns="sessionColumns"
        :rows-count="sessions.length"
        empty="No active sessions"
      >
        <tr
          v-for="session in sessions"
          :key="session.id"
          class="border-b border-border/40 last:border-0 hover:bg-muted/40"
        >
          <td class="px-5 py-3">
            <p class="font-medium">{{ session.name || session.email || session.sub }}</p>
            <p class="text-xs text-muted-foreground">
              {{ session.email || 'no email' }} ·
              <span class="font-mono">{{ shortSub(session.sub) }}</span>
            </p>
          </td>
          <td class="px-5 py-3">
            <UiStatusChip :tone="session.role ? 'neutral' : 'danger'">
              {{ session.role || 'unmapped' }}
            </UiStatusChip>
          </td>
          <td class="px-5 py-3 text-xs text-muted-foreground">
            {{ formatTime(session.last_seen_at) }}
          </td>
          <td class="px-5 py-3 text-xs text-muted-foreground">
            {{ formatTime(session.expires_at) }}
          </td>
          <td class="px-5 py-3 text-right">
            <UiButton
              variant="ghost"
              size="sm"
              :disabled="session.current"
              @click="revoke(session)"
            >
              {{ session.current ? 'Current' : 'Revoke' }}
            </UiButton>
          </td>
        </tr>
      </UiDataTable>

      <UiDataTable
        title="Recent authentication events"
        description="Logins, denials, configuration changes, provider sign-outs, and revocations."
        :columns="auditColumns"
        :rows-count="audit.length"
        :loading="auditLoading && !audit.length"
        empty="No authentication events yet"
      >
        <tr
          v-for="event in audit"
          :key="`${event.at}-${event.kind}-${event.sub ?? ''}`"
          class="border-b border-border/40 last:border-0 hover:bg-muted/40"
        >
          <td class="px-5 py-3 whitespace-nowrap text-xs text-muted-foreground">
            {{ formatTime(event.at) }}
          </td>
          <td class="px-5 py-3">
            <UiStatusChip :tone="event.kind.includes('failure') || event.kind.includes('denied') ? 'danger' : event.kind.includes('success') ? 'success' : 'neutral'">
              {{ event.kind }}
            </UiStatusChip>
          </td>
          <td class="px-5 py-3 text-xs">
            {{ event.email || event.sub || '—' }}
          </td>
          <td class="px-5 py-3 text-xs text-muted-foreground">{{ event.ip || '—' }}</td>
          <td class="px-5 py-3 text-xs text-muted-foreground">{{ event.detail || '—' }}</td>
        </tr>
        <template #footer>
          <BasePagination
            :total="auditTotal"
            :page="auditPage"
            :page-size="auditPageSize"
            :disabled="auditLoading"
            @update:page="changeAuditPage"
            @update:page-size="changeAuditPageSize"
          />
        </template>
      </UiDataTable>
    </template>

    <UiDialog
      :open="showDisable"
      @update:open="(value) => (showDisable = value)"
      title="Disable SSO?"
    >
      <template #description>
        All SSO sessions are revoked immediately. Users sign in with
        <code class="font-mono">[server].auth_secret</code> until SSO is enabled again.
      </template>
      <div class="mt-2 flex items-center gap-3">
        <UiButton variant="secondary" @click="showDisable = false">Cancel</UiButton>
        <UiButton variant="destructive" :disabled="disabling" @click="disable">
          {{ disabling ? 'Disabling…' : 'Disable SSO' }}
        </UiButton>
      </div>
    </UiDialog>

  </div>
</template>
