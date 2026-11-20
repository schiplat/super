<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-vue-next'
import apiClient from '@/api/client'
import { useCapabilitiesStore } from '@/stores/capabilities'
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
  SsoAuditEvent,
  SsoConfig,
  SsoConfigView,
  SsoGate,
  SsoRole,
  SsoSession,
  RoleRule,
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
const sessionHours = ref('12')
const idleMinutes = ref('60')

const loading = ref(true)
const saving = ref(false)
const secretSaving = ref(false)
const testing = ref(false)
const disabling = ref(false)
const showDisable = ref(false)

const errors = ref<string[]>([])
const notice = ref<{ tone: 'info' | 'success' | 'warning' | 'danger'; text: string } | null>(
  null,
)
const testResult = ref<{ tone: 'success' | 'danger'; text: string } | null>(null)

const sessions = ref<SsoSession[]>([])
const audit = ref<SsoAuditEvent[]>([])

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
const redirectUri = computed(
  () =>
    view.value?.redirect_uri ??
    `${config.value.public_base_url.replace(/\/$/, '')}/api/v1/auth/sso/callback`,
)

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
      apiClient.get<{ events: SsoAuditEvent[] }>('/api/v1/auth/sso/audit', {
        params: { limit: 25 },
      }),
    ])
    view.value = configResponse.data
    config.value = configResponse.data.config ?? emptyConfig()
    rules.value = rulesFromConfig(config.value)
    sessionHours.value = String(Math.round(config.value.session_ttl_secs / 3600))
    idleMinutes.value = String(Math.round(config.value.idle_timeout_secs / 60))
    sessions.value = sessionsResponse.data.sessions ?? []
    audit.value = auditResponse.data.events ?? []
  } catch (error) {
    notice.value = { tone: 'danger', text: apiError(error) }
  } finally {
    loading.value = false
  }
}

async function save() {
  errors.value = []
  notice.value = null
  testResult.value = null

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

    notice.value = { tone: 'success', text: 'Saved. SSO is configured but disabled.' }
    await refreshLists()
  } catch (error: any) {
    const data = error?.response?.data
    errors.value = Array.isArray(data?.errors) ? data.errors : []
    if (!errors.value.length) notice.value = { tone: 'danger', text: apiError(error) }
  } finally {
    saving.value = false
  }
}

async function saveSecret() {
  if (!secret.value) return
  secretSaving.value = true
  notice.value = null
  try {
    await apiClient.post('/api/v1/auth/sso/secret', { secret: secret.value })
    secret.value = ''
    const response = await apiClient.get<SsoConfigView>('/api/v1/auth/sso/config')
    view.value = response.data
    notice.value = {
      tone: 'success',
      text: 'Client secret stored. The previous value stays valid for 15 minutes.',
    }
  } catch (error) {
    notice.value = { tone: 'danger', text: apiError(error) }
  } finally {
    secretSaving.value = false
  }
}

async function testConnection() {
  testing.value = true
  testResult.value = null
  try {
    const response = await apiClient.post<{ issuer: string }>('/api/v1/auth/sso/test')
    testResult.value = {
      tone: 'success',
      text: `Discovery OK — ${response.data.issuer}`,
    }
  } catch (error) {
    testResult.value = { tone: 'danger', text: apiError(error) }
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
    apiClient.get<{ events: SsoAuditEvent[] }>('/api/v1/auth/sso/audit', {
      params: { limit: 25 },
    }),
  ])
  sessions.value = sessionsResponse.data.sessions ?? []
  audit.value = auditResponse.data.events ?? []
}

async function revoke(session: SsoSession) {
  try {
    await apiClient.delete(`/api/v1/auth/sso/sessions/${session.id}`)
    await refreshLists()
  } catch (error) {
    notice.value = { tone: 'danger', text: apiError(error) }
  }
}

async function copyRedirect() {
  try {
    await navigator.clipboard.writeText(redirectUri.value)
    notice.value = { tone: 'info', text: 'Redirect URI copied.' }
  } catch {
    notice.value = { tone: 'warning', text: 'Could not copy — select the value manually.' }
  }
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

    <UiSection
      title="Current gate"
      description="Exactly one gate guards the API. Enabling SSO switches the static secret off."
    >
      <div class="flex flex-wrap items-center gap-2">
        <UiStatusChip :tone="gateTone">{{ gateLabel[gate] }}</UiStatusChip>
        <UiStatusChip v-if="view?.break_glass" tone="neutral">break-glass secret set</UiStatusChip>
        <UiStatusChip v-else tone="warning">no break-glass secret</UiStatusChip>
        <UiStatusChip v-if="view?.configured" tone="neutral">
          configured {{ formatTime(view?.updated_at) }}
        </UiStatusChip>
      </div>
      <p class="mt-3 text-sm text-muted-foreground">
        The static <code class="font-mono text-xs">[server].auth_secret</code> stays in
        <code class="font-mono text-xs">conf/super.toml</code> and becomes dormant under SSO.
        If the SSO store cannot be read at startup, superd falls back to it (and refuses to
        start when it is missing).
      </p>
    </UiSection>

    <form class="space-y-6" @submit.prevent="save">
      <UiSection
        title="OpenID Connect provider"
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
            hint="Origin only — the redirect URI is derived from it"
          >
            <UiInput v-model="config.public_base_url" placeholder="https://super.example.com" />
          </UiField>
          <UiField label="Scopes" hint="Must include openid">
            <UiInput v-model="config.scopes" placeholder="openid profile email" />
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
          <UiField label="Session lifetime (hours)">
            <UiInput v-model="sessionHours" type="number" />
          </UiField>
          <UiField label="Idle timeout (minutes)">
            <UiInput v-model="idleMinutes" type="number" />
          </UiField>
        </div>

        <div class="mt-4 flex items-center justify-between gap-4 rounded-xl bg-muted/40 px-3 py-2">
          <div class="min-w-0">
            <p class="text-sm font-medium">Enable SSO</p>
            <p class="text-xs text-muted-foreground">
              Requires a stored client secret. Takes effect immediately.
            </p>
          </div>
          <UiSwitch v-model="config.enabled" aria-label="Enable SSO" />
        </div>

        <div class="mt-3 flex items-center justify-between gap-4 rounded-xl bg-muted/40 px-3 py-2">
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

        <UiNotice v-if="testResult" :tone="testResult.tone" class="mt-4">
          {{ testResult.text }}
        </UiNotice>

        <template #footer>
          <div class="flex flex-wrap items-center gap-3">
            <UiButton type="submit" :disabled="saving">
              {{ saving ? 'Saving…' : 'Save configuration' }}
            </UiButton>
            <UiButton
              type="button"
              variant="secondary"
              :disabled="testing || !view?.configured"
              @click="testConnection"
            >
              {{ testing ? 'Testing…' : 'Test connection' }}
            </UiButton>
          </div>
        </template>
      </UiSection>
    </form>

    <UiSection
      title="Redirect URI"
      description="Register this exact value at the identity provider."
    >
      <div class="flex items-center gap-2 rounded-xl bg-muted/40 px-3 py-2">
        <code class="min-w-0 flex-1 truncate font-mono text-xs">{{ redirectUri }}</code>
        <UiButton type="button" variant="secondary" size="sm" @click="copyRedirect">
          Copy
        </UiButton>
      </div>
    </UiSection>

    <UiSection
      title="Client secret"
      description="Write-only: stored owner-only in data/auth.db and never returned by the API."
    >
      <div class="flex flex-col gap-3 sm:flex-row sm:items-end">
        <UiField label="New client secret" class="flex-1">
          <UiInput
            v-model="secret"
            type="password"
            autocomplete="new-password"
            placeholder="••••••••"
          />
        </UiField>
        <UiButton
          type="button"
          :disabled="secretSaving || !secret"
          @click="saveSecret"
        >
          {{ secretSaving ? 'Saving…' : 'Save secret' }}
        </UiButton>
      </div>
      <p class="mt-3 text-xs text-muted-foreground">
        {{
          view?.secret_updated_at
            ? `Updated ${formatTime(view.secret_updated_at)}. The previous value stays valid for 15 minutes.`
            : 'No secret stored yet — SSO cannot be enabled without one.'
        }}
      </p>
    </UiSection>

    <UiSection
      title="Role rules"
      description="First match wins. No match rejects the login; email selectors require a verified address."
    >
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
    </UiSection>

    <UiDataTable
      title="Sessions"
      description="Local sessions issued after an SSO login. Revocation is immediate."
      :columns="sessionColumns"
      :rows-count="sessions.length"
      :loading="loading"
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
      description="Logins, denials, configuration changes, and revocations."
      :columns="auditColumns"
      :rows-count="audit.length"
      :loading="loading"
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
    </UiDataTable>

    <UiSection
      title="Disable SSO"
      description="Revokes every SSO session and returns the static secret to the gate."
    >
      <p class="text-sm text-muted-foreground">
        Disabling without a configured
        <code class="font-mono text-xs">[server].auth_secret</code> would leave a non-loopback
        instance unauthenticated; the server refuses that with a 409.
      </p>
      <template #footer>
        <UiButton
          type="button"
          variant="destructive"
          :disabled="!view?.configured || !config.enabled"
          @click="showDisable = true"
        >
          Disable SSO
        </UiButton>
      </template>
    </UiSection>

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
