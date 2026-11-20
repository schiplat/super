<script setup lang="ts">
import { ExternalLink } from 'lucide-vue-next';
import UiSwitch from '@/components/ui/UiSwitch.vue';

defineProps<{
  title: string;
  hint?: string;
  docHref?: string;
  docTitle?: string;
}>();

const enabled = defineModel<boolean>({ required: true });
</script>

<template>
  <div class="surface-card ring-1 ring-border/60 shadow-[0_1px_2px_hsl(220_20%_20%/0.04),0_2px_8px_hsl(220_20%_20%/0.03)]">
    <!-- Collapsed height is identical across optional sections -->
    <div class="flex items-center justify-between gap-3 px-6 py-4">
      <div class="flex items-center gap-2 min-w-0">
        <span class="text-muted-foreground shrink-0 [&_svg]:w-3.5 [&_svg]:h-3.5">
          <slot name="icon" />
        </span>
        <div class="min-w-0">
          <h3 class="type-eyebrow text-muted-foreground flex items-center gap-2">
            {{ title }}
            <a
              v-if="docHref"
              :href="docHref"
              target="_blank"
              class="inline-flex text-foreground/25 hover:text-primary transition-colors"
              :title="docTitle || title"
              @click.stop
            ><ExternalLink class="w-3 h-3" /></a>
          </h3>
          <p v-if="hint && !enabled" class="text-[11px] text-muted-foreground/75 truncate mt-0.5">{{ hint }}</p>
        </div>
      </div>
      <UiSwitch v-model="enabled" />
    </div>
    <div v-if="enabled" class="space-y-4 border-t border-border/40 px-6 pb-6 pt-4">
      <slot />
    </div>
  </div>
</template>
