<script setup lang="ts">
import { computed } from 'vue';
import { ShieldCheck, FileText } from 'lucide-vue-next';
import { getSuperConfig } from '@/lib/superConfig';

const cfg = getSuperConfig();
const appVersion = cfg.version || 'dev';

const isLicensed = computed(() => {
  const edition = cfg.edition.toLowerCase();
  return edition === 'licensed' || edition.includes('premium') || edition.includes('pro');
});
</script>

<template>
  <footer class="mt-10 pt-6 pb-2 border-t border-border/70 flex flex-wrap justify-center items-center gap-x-3 gap-y-2 text-xs text-muted-foreground/60">
    <span class="cursor-default font-medium">&copy; 2026 g1/DDL</span>
    <span class="w-1 h-1 rounded-full bg-border"></span>
    <div
      v-if="isLicensed"
      class="flex items-center gap-1 text-success/80 cursor-help"
      title="Licensed deployment"
    >
      <ShieldCheck class="w-3 h-3" />
      <span>Licensed</span>
    </div>
    <div
      v-else
      class="flex items-center gap-1 cursor-default"
      title="Community edition"
    >
      <span>Community</span>
    </div>
    <span class="w-1 h-1 rounded-full bg-border"></span>
    <a href="https://super.docs.sconts.com/" target="_blank" rel="noopener noreferrer" class="flex items-center gap-1 hover:text-foreground transition-colors">
      <FileText class="w-3.5 h-3.5" />
      <span>Documentation</span>
    </a>
    <span class="w-1 h-1 rounded-full bg-border"></span>
    <span class="font-mono opacity-80 cursor-help" title="superd version">
      v{{ appVersion }}
    </span>
  </footer>
</template>
