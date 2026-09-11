<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';
import { Lock, Loader2, ArrowRight } from 'lucide-vue-next';
import apiClient from '@/api/client';
import { useAuthStore } from '@/stores/auth';

const router = useRouter();
const authStore = useAuthStore();

const token = ref('');
const loading = ref(false);
const error = ref('');

async function handleLogin() {
  if (!token.value) return;
  loading.value = true;
  error.value = '';
  try {
    await apiClient.post('/api/v1/auth/login', {}, { headers: { Authorization: `Bearer ${token.value}` } });
    localStorage.setItem('super_token', token.value);
    await authStore.fetchProfile();
    router.push('/');
  } catch (e: any) {
    console.error(e);
    error.value = e.response?.data?.message || e.response?.data?.error || 'Invalid Access Token';
    localStorage.removeItem('super_token');
  } finally {
    loading.value = false;
  }
}
</script>

<template>
  <div class="min-h-screen flex items-center justify-center bg-muted p-4">
    <div class="w-full max-w-sm bg-card rounded-2xl overflow-hidden border border-border shadow-sm">
      <div class="h-1.5 w-full bg-gradient-to-r from-stone-500 to-stone-700"></div>
      <div class="p-8 sm:p-10">
        <div class="flex justify-center mb-6">
          <div class="w-14 h-14 bg-muted rounded-2xl flex items-center justify-center text-muted-foreground shadow-sm ring-1 ring-border">
            <Lock class="w-7 h-7" />
          </div>
        </div>
        <div class="text-center mb-8">
          <p class="text-muted-foreground text-sm">Sign in to manage your services</p>
        </div>
        <div class="flex flex-col gap-5">
          <div class="flex flex-col gap-1.5">
            <label class="text-xs font-semibold uppercase tracking-wider text-muted-foreground/60 pl-1">Access Token</label>
            <input
              v-model="token" type="password" placeholder="sk-..."
              class="w-full px-4 py-3 rounded-xl bg-muted border border-border text-foreground font-mono text-sm focus:outline-none focus:ring-2 focus:ring-ring/30 focus:border-foreground/35 transition-all placeholder:text-muted-foreground/50"
              :class="{ 'bg-destructive/5 border-destructive focus:border-destructive focus:ring-destructive/20': error }"
              @keyup.enter="handleLogin" autofocus
            />
            <div class="min-h-4 pl-1">
              <p v-if="error" class="text-destructive text-xs font-medium leading-snug">{{ error }}</p>
            </div>
          </div>
          <button
            class="w-full py-3 px-4 bg-primary text-primary-foreground hover:bg-primary/90 font-medium rounded-xl transition-all flex items-center justify-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed"
            :disabled="loading || !token"
            @click="handleLogin"
          >
            <Loader2 v-if="loading" class="w-5 h-5 animate-spin" />
            <template v-else>
              <span>Sign In</span>
              <ArrowRight class="w-4 h-4 opacity-80" />
            </template>
          </button>
        </div>
      </div>
      <div class="bg-muted py-4 text-center border-t border-border">
        <p class="text-xs text-muted-foreground/70 font-medium">Secured by Super Premium</p>
      </div>
    </div>
  </div>
</template>
