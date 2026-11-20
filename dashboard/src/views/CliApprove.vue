<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { CheckCircle2, Loader2, TerminalSquare } from 'lucide-vue-next'
import apiClient from '@/api/client'
import { useAuthStore } from '@/stores/auth'
import UiButton from '@/components/ui/UiButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'

const route = useRoute()
const router = useRouter()
const auth = useAuthStore()

const code = computed(() => (typeof route.query.code === 'string' ? route.query.code : ''))
const approving = ref(false)
const done = ref(false)
const error = ref('')

async function approve() {
  if (!code.value) return
  approving.value = true
  error.value = ''
  try {
    await apiClient.post('/api/v1/auth/sso/cli/approve', { user_code: code.value })
    done.value = true
  } catch (e: any) {
    error.value =
      e.response?.data?.message || 'This sign-in code is unknown, expired, or already used.'
  } finally {
    approving.value = false
  }
}

onMounted(() => {
  void auth.ensureProfile()
})
</script>

<template>
  <div class="flex min-h-screen items-center justify-center bg-background p-4">
    <div
      class="w-full max-w-md overflow-hidden rounded-2xl bg-card ring-1 ring-border/60 shadow-[0_1px_2px_hsl(220_20%_20%/0.04),0_2px_8px_hsl(220_20%_20%/0.03)]"
    >
      <div class="p-8">
        <div class="mb-6 flex justify-center">
          <div
            class="flex h-14 w-14 items-center justify-center rounded-2xl bg-muted/40 text-muted-foreground"
          >
            <TerminalSquare class="h-7 w-7" />
          </div>
        </div>

        <h1 class="text-center text-lg font-semibold tracking-tight">Approve CLI sign-in</h1>
        <p class="mt-1 text-center text-sm text-muted-foreground">
          A terminal asked to sign in as you. Confirm the code it printed.
        </p>

        <div v-if="!code" class="mt-6">
          <UiNotice tone="danger">No sign-in code in the URL.</UiNotice>
        </div>

        <div v-else class="mt-6 space-y-4">
          <div class="rounded-xl bg-muted/40 px-3 py-3 text-center">
            <p class="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
              Code
            </p>
            <p class="mt-1 font-mono text-xl tracking-widest">{{ code }}</p>
          </div>

          <p v-if="auth.user" class="text-center text-xs text-muted-foreground">
            Signed in as {{ auth.user.name }} ({{ auth.user.role }})
          </p>

          <UiNotice v-if="done" tone="success" title="Approved">
            The terminal can finish signing in. You can close this page.
          </UiNotice>
          <UiNotice v-if="error" tone="danger">{{ error }}</UiNotice>

          <div v-if="!done" class="flex items-center justify-center gap-3">
            <UiButton variant="secondary" @click="router.push('/')">Cancel</UiButton>
            <UiButton :disabled="approving" @click="approve">
              <Loader2 v-if="approving" class="h-4 w-4 animate-spin" />
              <CheckCircle2 v-else class="h-4 w-4" />
              Approve
            </UiButton>
          </div>
        </div>
      </div>
      <div class="border-t border-border/40 py-4 text-center">
        <p class="text-xs text-muted-foreground">Secured by Super</p>
      </div>
    </div>
  </div>
</template>
