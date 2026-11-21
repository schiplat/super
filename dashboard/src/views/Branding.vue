<script setup lang="ts">
/**
 * Branding (Settings → Branding): upload a custom sidebar logo for this
 * instance. The default stays the bundled super.png; an upload replaces it
 * for every operator (it is instance state, not per-browser). GET is public
 * (the sidebar renders it with a plain <img>), writes are Admin.
 */
import { computed, onMounted, ref } from 'vue'
import { RefreshCw, Upload } from 'lucide-vue-next'
import apiClient from '@/api/client'
import { useCapabilitiesStore } from '@/stores/capabilities'
import UiButton from '@/components/ui/UiButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiSection from '@/components/ui/UiSection.vue'

const caps = useCapabilitiesStore()

const loading = ref(false)
const uploading = ref(false)
const resetting = ref(false)
const errors = ref<string[]>([])
const notice = ref<{ tone: 'info' | 'success' | 'warning' | 'danger'; text: string } | null>(null)
/** Cache-buster: bumped after upload/reset so the <img> reloads. */
const logoVersion = ref(0)
const fileInput = ref<HTMLInputElement | null>(null)

/** What the sidebar will render right now (default or custom). */
const currentLogoSrc = computed(() =>
  caps.customLogo ? `/api/v1/branding/logo?v=${logoVersion.value}` : '/super.png',
)

function apiError(error: any): string {
  const data = error?.response?.data
  return data?.message || data?.error || error?.message || 'Request failed'
}

function pickFile() {
  fileInput.value?.click()
}

async function onFileChosen(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = '';
  if (!file) return;

  errors.value = [];
  if (file.size > 1024 * 1024) {
    errors.value = ['The file is larger than 1 MiB — pick a smaller image.'];
    return;
  }
  uploading.value = true;
  notice.value = null;
  try {
    const bytes = await file.arrayBuffer();
    await apiClient.put('/api/v1/branding/logo', bytes, {
      headers: { 'Content-Type': file.type || 'application/octet-stream' },
    });
    logoVersion.value += 1;
    await caps.refresh();
    notice.value = { tone: 'success', text: 'Logo updated — every operator sees it now.' };
  } catch (error) {
    errors.value = [apiError(error)];
  } finally {
    uploading.value = false;
  }
}

async function resetLogo() {
  resetting.value = true;
  notice.value = null;
  try {
    await apiClient.delete('/api/v1/branding/logo');
    logoVersion.value += 1;
    await caps.refresh();
    notice.value = { tone: 'success', text: 'Back to the bundled Super logo.' };
  } catch (error) {
    notice.value = { tone: 'danger', text: apiError(error) };
  } finally {
    resetting.value = false;
  }
}

onMounted(() => {
  // Prime the custom-logo fact (the sidebar probe may have raced the upload).
  void caps.refresh();
})
</script>

<template>
  <div class="page-stack">
    <UiNotice v-if="notice" :tone="notice.tone">{{ notice.text }}</UiNotice>

    <UiSection
      title="Sidebar logo"
      description="A custom logo replaces the bundled Super logo in the sidebar for every operator of this instance."
    >
      <div class="flex flex-col gap-6 sm:flex-row sm:items-start">
        <!-- Preview: the sidebar's brand row, roughly to scale -->
        <div class="w-full max-w-[248px] shrink-0 rounded-xl bg-sidebar p-3">
          <div class="flex h-16 items-center px-3">
            <img
              :src="currentLogoSrc"
              alt="Current sidebar logo"
              class="h-8 max-w-[150px] object-contain"
              @error="($event.target as HTMLImageElement).style.visibility = 'hidden'"
              @load="($event.target as HTMLImageElement).style.visibility = 'visible'"
            />
          </div>
        </div>

        <div class="min-w-0 flex-1">
          <div class="flex flex-wrap items-center gap-3">
            <UiButton type="button" :disabled="uploading" @click="pickFile">
              <Upload class="h-4 w-4" />
              {{ uploading ? 'Uploading…' : 'Upload logo' }}
            </UiButton>
            <UiButton
              type="button"
              variant="ghost"
              class="text-destructive hover:bg-destructive/10"
              :disabled="resetting || !caps.customLogo"
              @click="resetLogo"
            >
              <RefreshCw class="h-4 w-4" />
              {{ resetting ? 'Resetting…' : 'Reset to default' }}
            </UiButton>
            <input
              ref="fileInput"
              type="file"
              accept=".png,.jpg,.jpeg,.webp,.svg,image/png,image/jpeg,image/webp,image/svg+xml"
              class="hidden"
              @change="onFileChosen"
            />
          </div>

          <UiNotice v-if="errors.length" tone="danger" class="mt-4">
            <ul class="list-disc space-y-1 pl-4">
              <li v-for="error in errors" :key="error">{{ error }}</li>
            </ul>
          </UiNotice>

          <div class="mt-4 rounded-xl bg-muted/40 p-3 text-xs leading-relaxed text-muted-foreground">
            <p class="font-medium text-foreground">Recommended</p>
            <ul class="mt-1 list-disc space-y-1 pl-4">
              <li>
                <span class="text-foreground">Type:</span> SVG (vector, best) or PNG with a
                transparent background; JPEG and WebP are also accepted.
              </li>
              <li>
                <span class="text-foreground">Size:</span> the sidebar shows it 32&nbsp;px tall —
                supply at least 128&nbsp;px height (4× margin); keep the longest edge ≤&nbsp;512&nbsp;px.
              </li>
              <li><span class="text-foreground">File:</span> up to 1&nbsp;MiB.</li>
            </ul>
          </div>
        </div>
      </div>
    </UiSection>
  </div>
</template>
