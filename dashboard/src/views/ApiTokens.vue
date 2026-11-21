<script setup lang="ts">
/**
 * Named API tokens (Settings → API Tokens): create with an optional expiry
 * date, one-time plaintext reveal, immediate revocation. The same paths are
 * served by the security plugin in licensed mode, which hides this page.
 */
import { onMounted, ref } from 'vue'
import { Check, Copy, Plus } from 'lucide-vue-next'
import apiClient from '@/api/client'
import UiButton from '@/components/ui/UiButton.vue'
import UiDataTable from '@/components/ui/UiDataTable.vue'
import type { DataColumn } from '@/types/ui'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiField from '@/components/ui/UiField.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiStatusChip from '@/components/ui/UiStatusChip.vue'
import type { ApiTokenCreateResult, ApiTokenInfo, SsoRole } from '@/types/sso'

const tokens = ref<ApiTokenInfo[]>([])
const loading = ref(true)

const showCreateToken = ref(false)
const newTokenName = ref('')
const newTokenRole = ref<SsoRole>('viewer')
const /** 'none' | 'date' */ expiryMode = ref<'none' | 'date'>('none')
const newTokenExpiryDate = ref('')
const creatingToken = ref(false)
const createTokenError = ref('')
const revealedToken = ref<ApiTokenCreateResult | null>(null)
const copiedKey = ref('')

const notice = ref<{ tone: 'info' | 'success' | 'warning' | 'danger'; text: string } | null>(null)

const columns: DataColumn[] = [
  { key: 'name', label: 'Name' },
  { key: 'prefix', label: 'Token' },
  { key: 'role', label: 'Role' },
  { key: 'expires', label: 'Expires' },
  { key: 'created', label: 'Created' },
  { key: 'used', label: 'Last used' },
  { key: 'actions', label: '', align: 'right' },
]

function apiError(error: any): string {
  const data = error?.response?.data
  if (Array.isArray(data?.errors) && data.errors.length) return data.errors.join('; ')
  return data?.message || data?.error || error?.message || 'Request failed'
}

function formatTime(seconds: number | null | undefined): string {
  if (!seconds) return '—'
  return new Date(seconds * 1000).toLocaleString()
}

function isExpired(token: ApiTokenInfo): boolean {
  return token.expires_at !== null && token.expires_at * 1000 < Date.now()
}

async function loadTokens() {
  loading.value = true
  try {
    const response = await apiClient.get<ApiTokenInfo[]>('/api/v1/auth/tokens')
    tokens.value = Array.isArray(response.data) ? response.data : []
  } catch (error) {
    notice.value = { tone: 'danger', text: apiError(error) }
  } finally {
    loading.value = false
  }
}

function openCreateToken() {
  newTokenName.value = ''
  newTokenRole.value = 'viewer'
  expiryMode.value = 'none'
  newTokenExpiryDate.value = ''
  createTokenError.value = ''
  showCreateToken.value = true
}

async function createToken() {
  const name = newTokenName.value.trim()
  if (!name) {
    createTokenError.value = 'A name is required.'
    return
  }
  let expiresAt: number | undefined
  if (expiryMode.value === 'date') {
    if (!newTokenExpiryDate.value) {
      createTokenError.value = 'Pick an expiry date, or switch back to “No expiry”.'
      return
    }
    // The token stays valid through the end of the picked day (local time).
    expiresAt = Math.floor(new Date(`${newTokenExpiryDate.value}T23:59:59`).getTime() / 1000)
    if (expiresAt * 1000 <= Date.now()) {
      createTokenError.value = 'The expiry date must be in the future.'
      return
    }
  }

  creatingToken.value = true
  createTokenError.value = ''
  try {
    const response = await apiClient.post<ApiTokenCreateResult>('/api/v1/auth/tokens', {
      name,
      role: newTokenRole.value,
      ...(expiresAt ? { expires_at: expiresAt } : {}),
    })
    showCreateToken.value = false
    // The plaintext is shown exactly once, then never again.
    revealedToken.value = response.data
    await loadTokens()
  } catch (error) {
    createTokenError.value = apiError(error)
  } finally {
    creatingToken.value = false
  }
}

async function revokeToken(token: ApiTokenInfo) {
  try {
    await apiClient.delete(`/api/v1/auth/tokens/${token.id}`)
    await loadTokens()
  } catch (error) {
    notice.value = { tone: 'danger', text: apiError(error) }
  }
}

async function copyRevealedToken() {
  if (!revealedToken.value) return
  try {
    await navigator.clipboard.writeText(revealedToken.value.token)
    copiedKey.value = 'reveal'
    window.setTimeout(() => {
      if (copiedKey.value === 'reveal') copiedKey.value = ''
    }, 1500)
  } catch {
    // clipboard unavailable — the field stays selectable on screen
  }
}

/** Earliest pickable expiry date: today (the token would expire tonight). */
function todayIso(): string {
  const now = new Date()
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
}

onMounted(loadTokens)
</script>

<template>
  <div class="page-stack">
    <UiNotice v-if="notice" :tone="notice.tone">{{ notice.text }}</UiNotice>

    <UiDataTable
      title="API tokens"
      description="Named credentials for the CLI and automation. Each token authenticates with its own role; revocation and expiry are immediate. The secret is shown once at creation."
      :columns="columns"
      :rows-count="tokens.length"
      :loading="loading"
      empty="No API tokens"
    >
      <template #toolbar>
        <div class="flex flex-1 items-center justify-end gap-2">
          <UiButton type="button" size="sm" @click="openCreateToken">
            <Plus class="h-3.5 w-3.5" />
            New token
          </UiButton>
        </div>
      </template>
      <tr
        v-for="token in tokens"
        :key="token.id"
        class="border-b border-border/40 last:border-0 hover:bg-muted/40"
      >
        <td class="px-5 py-3">
          <p class="font-medium">{{ token.name }}</p>
        </td>
        <td class="px-5 py-3">
          <code class="font-mono text-xs text-muted-foreground">{{ token.token_prefix }}</code>
        </td>
        <td class="px-5 py-3">
          <UiStatusChip :tone="token.role === 'admin' ? 'success' : token.role === 'operator' ? 'info' : 'neutral'">
            {{ token.role }}
          </UiStatusChip>
        </td>
        <td class="px-5 py-3">
          <div class="flex items-center gap-2">
            <span class="text-xs text-muted-foreground">
              {{ token.expires_at ? formatTime(token.expires_at) : 'Never' }}
            </span>
            <UiStatusChip v-if="isExpired(token)" tone="danger">expired</UiStatusChip>
          </div>
        </td>
        <td class="px-5 py-3 text-xs text-muted-foreground">{{ formatTime(token.created_at) }}</td>
        <td class="px-5 py-3 text-xs text-muted-foreground">
          {{ token.last_used_at ? formatTime(token.last_used_at) : 'never' }}
        </td>
        <td class="px-5 py-3 text-right">
          <UiButton
            variant="ghost"
            size="sm"
            class="text-destructive hover:bg-destructive/10"
            @click="revokeToken(token)"
          >
            Revoke
          </UiButton>
        </td>
      </tr>
    </UiDataTable>

    <UiDialog
      :open="showCreateToken"
      @update:open="(value) => (showCreateToken = value)"
      title="New API token"
    >
      <template #description>
        A named credential for the CLI or automation. It authenticates with the role you pick
        here — grant the least that works.
      </template>
      <div class="mt-2 space-y-3">
        <UiField label="Name" hint="Letters, digits, '.', '_' or '-'">
          <UiInput v-model="newTokenName" placeholder="ci-deploy" @keyup.enter="createToken" />
        </UiField>
        <UiField label="Role">
          <select v-model="newTokenRole" class="field-control">
            <option value="viewer">Viewer — read-only</option>
            <option value="operator">Operator — read and manage programs</option>
            <option value="admin">Admin — everything, including tokens</option>
          </select>
        </UiField>
        <UiField label="Expiry">
          <div class="space-y-2">
            <label class="flex items-center gap-2 text-sm">
              <input v-model="expiryMode" type="radio" value="none" class="accent-foreground" />
              No expiry
            </label>
            <label class="flex items-center gap-2 text-sm">
              <input v-model="expiryMode" type="radio" value="date" class="accent-foreground" />
              Valid until
            </label>
            <input
              v-if="expiryMode === 'date'"
              v-model="newTokenExpiryDate"
              type="date"
              :min="todayIso()"
              class="field-control"
            />
          </div>
        </UiField>
        <p v-if="createTokenError" class="text-xs font-medium text-destructive">
          {{ createTokenError }}
        </p>
        <div class="flex items-center justify-end gap-2">
          <UiButton variant="secondary" @click="showCreateToken = false">Cancel</UiButton>
          <UiButton :disabled="creatingToken || !newTokenName.trim()" @click="createToken">
            {{ creatingToken ? 'Creating…' : 'Create token' }}
          </UiButton>
        </div>
      </div>
    </UiDialog>

    <!-- one-time reveal: this plaintext is never retrievable again -->
    <UiDialog
      :open="revealedToken !== null"
      @update:open="(value) => { if (!value) revealedToken = null }"
      :title="`Token “${revealedToken?.record.name ?? ''}”`"
    >
      <template #description>
        Copy it now — it will not be shown again. Only the prefix identifies the token after
        this dialog closes.
      </template>
      <div class="mt-2 space-y-3">
        <div class="flex items-center gap-2 rounded-xl bg-muted/40 px-3 py-2">
          <code class="min-w-0 flex-1 truncate font-mono text-xs">{{ revealedToken?.token }}</code>
          <UiButton
            type="button"
            variant="secondary"
            size="sm"
            :class="copiedKey === 'reveal' ? 'text-success' : 'text-muted-foreground'"
            @click="copyRevealedToken"
          >
            <Check v-if="copiedKey === 'reveal'" class="h-3.5 w-3.5" />
            <Copy v-else class="h-3.5 w-3.5" />
            {{ copiedKey === 'reveal' ? 'Copied' : 'Copy' }}
          </UiButton>
        </div>
        <p class="text-xs text-muted-foreground">
          Use it as <code class="font-mono">Authorization: Bearer su_…</code>, or save it with
          <code class="font-mono">super login su_…</code>.
        </p>
        <div class="flex justify-end">
          <UiButton @click="revealedToken = null">Done</UiButton>
        </div>
      </div>
    </UiDialog>
  </div>
</template>
