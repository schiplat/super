<script setup lang="ts">
import { Box, Terminal, Plus, ArrowRight } from 'lucide-vue-next';
import { RouterLink } from 'vue-router';
import UiButton from '@/components/ui/UiButton.vue';

defineProps<{
  premium?: boolean;
  canCreate?: boolean;
}>();
</script>

<template>
  <div class="rounded-xl border border-dashed border-border bg-card/80 px-6 py-14 sm:py-16 flex flex-col items-center text-center">
    <div class="mb-5 flex h-14 w-14 items-center justify-center rounded-2xl border border-border bg-muted text-muted-foreground/60">
      <Box class="h-7 w-7" stroke-width="1.5" />
    </div>

    <h3 class="text-lg font-semibold tracking-tight text-foreground">No programs yet</h3>
    <p class="mt-2 max-w-sm text-sm leading-relaxed text-muted-foreground">
      Super is running, but nothing is managed yet. Add a process to start monitoring uptime, logs, and health.
    </p>

    <div v-if="premium && canCreate" class="mt-8 flex flex-col items-center gap-3">
      <RouterLink to="/programs/new">
        <UiButton class="h-10 min-w-[11.5rem] gap-2">
          <Plus class="h-4 w-4" stroke-width="2.25" />
          Create first program
          <ArrowRight class="h-3.5 w-3.5 opacity-80" />
        </UiButton>
      </RouterLink>
      <p class="text-xs text-muted-foreground/50">
        Or use <code class="font-mono text-muted-foreground">super add</code> from the CLI
      </p>
    </div>

    <div v-else-if="premium && !canCreate" class="mt-8 max-w-sm text-sm text-muted-foreground/70">
      Ask an Admin or Operator to create programs, or use the CLI with an Operator/Admin Access Token.
    </div>

    <div v-else class="mt-8 w-full max-w-lg space-y-3 text-left">
      <div class="flex items-start gap-3 rounded-lg border border-border bg-muted px-4 py-3 font-mono text-xs text-muted-foreground">
        <Terminal class="mt-0.5 h-4 w-4 shrink-0 opacity-40" />
        <code>super add ./my-app --name my-app --autostart</code>
      </div>
      <p class="text-center text-xs text-muted-foreground/50">
        Or define programs in <code class="font-mono">super.toml</code> / <code class="font-mono">POST /api/v1/programs</code>
      </p>
    </div>
  </div>
</template>
