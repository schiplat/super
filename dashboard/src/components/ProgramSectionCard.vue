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
  <div class="surface-card border border-border shadow-sm">
    <!-- Collapsed height is identical across optional sections -->
    <div class="flex items-center justify-between gap-3 px-5 min-h-[3.25rem]">
      <div class="flex items-center gap-2 min-w-0">
        <span class="text-muted-foreground shrink-0 [&_svg]:w-3.5 [&_svg]:h-3.5">
          <slot name="icon" />
        </span>
        <div class="min-w-0">
          <h3 class="text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground flex items-center gap-2">
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
    <div v-if="enabled" class="px-5 pb-5 border-t border-border/60 pt-4 space-y-4">
      <slot />
    </div>
  </div>
</template>
