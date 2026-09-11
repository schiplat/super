<script setup lang="ts">
import { computed } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { ShieldAlert, ServerCrash, Ghost, Home, RefreshCw, PlugZap, TimerOff } from 'lucide-vue-next';
import UiButton from '@/components/ui/UiButton.vue';

const route = useRoute();
const router = useRouter();

const code = computed(() => route.params.code?.toString() || '404');

const config = computed(() => {
  switch (code.value) {
    case '403': return { title: 'Access Denied', msg: "You don't have the necessary permissions to view this resource.", icon: ShieldAlert, variant: 'secondary' as const };
    case '500': return { title: 'Server Error', msg: "Something went wrong on our end. We're working on it.", icon: ServerCrash, variant: 'destructive' as const };
    case '503': return { title: 'Service Unavailable', msg: "The service is temporarily unavailable. Please check the backend connection.", icon: PlugZap, variant: 'destructive' as const };
    case '504': return { title: 'Gateway Timeout', msg: 'A plugin call did not answer in time. The request was dropped (fail closed) — try again.', icon: TimerOff, variant: 'destructive' as const };
    default: return { title: 'Page Not Found', msg: "Sorry, we couldn't find the page you're looking for.", icon: Ghost, variant: 'default' as const };
  }
});

const isServiceError = computed(() => code.value === '500' || code.value === '503' || code.value === '504');

/** Leave /error/* — reloading this URL would keep the user stuck even after the API recovers. */
function goHome() {
  router.replace('/');
}

function handleRetry() {
  // Hard navigate so a recovered daemon is contacted from a clean dashboard mount.
  window.location.assign('/');
}
</script>

<template>
  <div class="min-h-screen bg-background flex items-center justify-center p-4">
    <div class="text-center space-y-6 max-w-md">
      <div class="relative inline-block">
        <div class="w-24 h-24 bg-card border border-border rounded-full flex items-center justify-center shadow-sm mx-auto">
          <component :is="config.icon" class="w-12 h-12 text-muted-foreground" />
        </div>
        <div class="absolute -top-2 -right-2 w-4 h-4 bg-foreground/20 rounded-full animate-bounce"></div>
      </div>
      <div class="space-y-2">
        <h1 class="text-6xl font-black text-foreground tracking-tighter">{{ code }}</h1>
        <h2 class="text-2xl font-bold text-foreground/80">{{ config.title }}</h2>
        <p class="text-muted-foreground">{{ config.msg }}</p>
      </div>
      <div class="flex flex-wrap items-center justify-center gap-3">
        <UiButton v-if="isServiceError" @click="handleRetry" class="shadow-sm">
          <RefreshCw class="w-4 h-4" />
          Try again
        </UiButton>
        <UiButton :variant="isServiceError ? 'outline' : undefined" @click="goHome" class="shadow-sm">
          <Home class="w-4 h-4" />
          Back to Dashboard
        </UiButton>
      </div>
    </div>
  </div>
</template>
