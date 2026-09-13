// src/stores/auth.ts
import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import router from '@/router';
import apiClient from '@/api/client';
import { useCapabilitiesStore } from '@/stores/capabilities';

export type UserRole = 'Viewer' | 'Operator' | 'Admin';

export interface UserProfile {
  id: string;
  name: string;
  role: UserRole;
  avatar: string;
}

const LOCAL_ADMIN: UserProfile = {
  id: 'local',
  name: 'Local',
  role: 'Admin',
  avatar: 'L',
};

export const useAuthStore = defineStore('auth', () => {
  const user = ref<UserProfile | null>(null);
  /** In-flight profile load so refresh races don't redirect before role is known. */
  let profileLoad: Promise<void> | null = null;

  const isAuthenticated = computed(() => !!user.value);
  const isAdmin = computed(() => user.value?.role === 'Admin');
  // Operator and Admin: start / stop / restart / signal
  const canOperate = computed(() => ['Admin', 'Operator'].includes(user.value?.role || ''));
  // Admin only: program edit/delete, stack apply, system reload
  const canManage = computed(() => user.value?.role === 'Admin');

  function applyLocalAdmin() {
    user.value = { ...LOCAL_ADMIN };
  }

  async function fetchProfile() {
    const caps = useCapabilitiesStore();
    await caps.ensureDiscovered();

    // OSS / no auth gate: full local control, no login wall.
    if (!caps.auth) {
      applyLocalAdmin();
      return;
    }

    const token = localStorage.getItem('super_token');
    if (!token) {
      user.value = null;
      return;
    }

    // Core auth only (no token CRUD): verify Bearer against the server first.
    if (!caps.security) {
      try {
        await apiClient.post('/api/v1/auth/login', {});
        user.value = {
          id: 'root',
          name: 'Administrator',
          role: 'Admin',
          avatar: 'A',
        };
      } catch {
        await logout();
      }
      return;
    }

    try {
      // 1. Fetch tokens visible to this Bearer (Admin: all; others: self)
      const res = await apiClient.get<any[]>('/api/v1/auth/tokens');
      const tokens = res.data || [];

      // 2. Match local Access Token prefix (sk-… → first 6 chars)
      const localPrefix = token.substring(0, 6);
      const match = tokens.find((t) => t.token_prefix === localPrefix);

      if (match) {
        const displayRole =
          match.role.charAt(0).toUpperCase() + match.role.slice(1).toLowerCase();

        user.value = {
          id: match.id,
          name: match.name,
          role: displayRole as UserRole,
          avatar: match.name.charAt(0).toUpperCase(),
        };
        return;
      }

      // Bootstrap auth_secret is not in the token store (no sk- prefix).
      if (!token.startsWith('sk-')) {
        user.value = {
          id: 'root',
          name: 'Administrator',
          role: 'Admin',
          avatar: 'A',
        };
        return;
      }

      // sk-… that did not resolve — drop the session; do not render as Admin/Viewer.
      await logout();
    } catch {
      await logout();
    }
  }

  /** Resolve once; safe to call from layout + page mounts on hard refresh. */
  async function ensureProfile() {
    if (user.value) return;
    if (!profileLoad) {
      profileLoad = fetchProfile().finally(() => {
        profileLoad = null;
      });
    }
    await profileLoad;
  }

  async function logout() {
    const caps = useCapabilitiesStore();
    const token = localStorage.getItem('super_token');
    if (token && caps.auth) {
      try {
        await apiClient.post('/api/v1/auth/logout');
      } catch {
        // Best-effort: still clear local session.
      }
    }
    localStorage.removeItem('super_token');
    user.value = null;

    if (!caps.auth) {
      applyLocalAdmin();
      return;
    }
    router.push('/login');
  }

  return {
    user,
    isAuthenticated,
    isAdmin,
    canOperate,
    canManage,
    fetchProfile,
    ensureProfile,
    logout,
  };
});
