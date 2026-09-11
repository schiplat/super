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

    // OSS / no security plugin: full local control, no login wall.
    if (!caps.security) {
      applyLocalAdmin();
      return;
    }

    const token = localStorage.getItem('super_token');
    if (!token) {
      user.value = null;
      return;
    }

    try {
      // 1. Fetch all tokens
      // RBAC allows Operator/Viewer GET on this endpoint
      const res = await apiClient.get<any[]>('/api/v1/auth/tokens');
      const tokens = res.data || [];

      // 2. Match local token prefix (sk-xxxxxx) to a record
      // AuthRecord includes token_prefix (first 6 chars)
      const localPrefix = token.substring(0, 6);
      const match = tokens.find((t) => t.token_prefix === localPrefix);

      if (match) {
        // Matched: regular access token
        // Capitalize role for display consistency (e.g. "operator" -> "Operator")
        const displayRole = match.role.charAt(0).toUpperCase() + match.role.slice(1).toLowerCase();

        user.value = {
          id: match.id,
          name: match.name, // Token display name from API
          role: displayRole as UserRole,
          avatar: match.name.charAt(0).toUpperCase(),
        };
      } else {
        // No match: root secret (not in DB) or incomplete token list
        // Default to root administrator
        user.value = {
          id: 'root',
          name: 'Administrator',
          role: 'Admin',
          avatar: 'A',
        };
      }
    } catch (e) {
      console.error('Failed to fetch profile details:', e);
      // Fallback: keep session but profile may be inaccurate
      user.value = {
        id: 'unknown',
        name: 'Unknown User',
        role: 'Viewer',
        avatar: '?',
      };
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
    if (token && caps.security) {
      try {
        await apiClient.post('/api/v1/auth/logout');
      } catch {
        // Best-effort: still clear local session.
      }
    }
    localStorage.removeItem('super_token');
    user.value = null;

    if (!caps.security) {
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
