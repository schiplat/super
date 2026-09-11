<script setup lang="ts">
import { onUnmounted, provide, ref, type Ref } from 'vue';
import { RouterLink, RouterView, useRoute } from 'vue-router';
import { Plus } from 'lucide-vue-next';
import UiButton from '@/components/ui/UiButton.vue';

export type NotifyHeaderAction = {
  label: string;
  onClick: () => void;
  variant?: 'default' | 'outline' | 'secondary' | 'ghost' | 'destructive';
} | null;

const route = useRoute();
const headerAction = ref<NotifyHeaderAction>(null);

provide('notifyHeaderAction', headerAction as Ref<NotifyHeaderAction>);

onUnmounted(() => {
  headerAction.value = null;
});

const tabs = [
  { to: '/settings/notify/webhooks', label: 'Webhooks', match: '/settings/notify/webhooks' },
  { to: '/settings/notify/rules', label: 'Inhibition rules', match: '/settings/notify/rules' },
  { to: '/settings/notify/delivery', label: 'Delivery', match: '/settings/notify/delivery' },
] as const;

function isActive(match: string): boolean {
  return route.path === match || route.path.startsWith(`${match}/`);
}
</script>

<template>
  <div class="max-w-5xl mx-auto py-6 space-y-6">
    <div class="flex items-start justify-between gap-4">
      <div>
        <h1 class="text-xl font-bold tracking-tight">Notification Settings</h1>
        <p class="text-sm text-muted-foreground mt-1">Webhook alerts for process and system events.</p>
      </div>
      <UiButton
        v-if="headerAction"
        size="sm"
        class="gap-1.5 shrink-0"
        :variant="headerAction.variant || 'default'"
        @click="headerAction.onClick"
      >
        <Plus class="w-3.5 h-3.5" />
        {{ headerAction.label }}
      </UiButton>
    </div>

    <div class="inline-flex p-0.5 rounded-lg bg-muted/80" role="tablist" aria-label="Notification sections">
      <RouterLink
        v-for="tab in tabs"
        :key="tab.to"
        :to="tab.to"
        role="tab"
        :aria-selected="isActive(tab.match)"
        class="px-3 py-1.5 rounded-md text-sm font-medium transition-colors"
        :class="isActive(tab.match) ? 'bg-card text-foreground' : 'text-muted-foreground hover:text-foreground'"
      >
        {{ tab.label }}
      </RouterLink>
    </div>

    <RouterView />
  </div>
</template>
