import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { useWebSocket } from '@vueuse/core'
import apiClient from '@/api/client'
import { API_PATHS } from '@/api/paths'
import { alertDialog } from '@/lib/app-dialog'
import type { Program, WsMessage, ProgramDetail } from '@/types'

// Log callback type
type LogCallback = (line: string, source: 'stdout' | 'stderr' | 'superd') => void;

export const useProgramStore = defineStore('program', () => {
  // --- State ---
  const programs = ref<Program[]>([])
  const isLoading = ref(false)
  const error = ref<string | null>(null)

  // IDs currently undergoing an action (button loading state)
  const operatingIds = ref<Set<string>>(new Set())

  // Log subscription registry (key: program ID, value: callback)
  const logListeners = new Map<string, LogCallback>();

  // --- WebSocket Setup ---
  // Infer protocol (https -> wss, http -> ws)
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const host = window.location.host;

  // Append token from localStorage for Premium auth (empty in OSS)
  const token = localStorage.getItem('super_token');
  const query = token ? `?token=${token}` : '';

  // Production connects to host/ws; dev uses Vite /ws proxy
  const wsUrl = `${protocol}//${host}/ws${query}`;

  const { status: wsStatus } = useWebSocket(wsUrl, {
    autoReconnect: true,
    heartbeat: {
      message: 'ping',
      interval: 30000,
    },
    onMessage: (_, event) => {
      try {
        const msg: WsMessage = JSON.parse(event.data);
        handleWsMessage(msg);
      } catch (e) {
        // console.warn('WS parse error:', event.data);
      }
    }
  });

  const isConnected = computed(() => wsStatus.value === 'OPEN');

  // --- WebSocket message handling ---
  function handleWsMessage(msg: WsMessage) {
    // 1. Status change
    if (msg.type === 'StatusChange') {
      const { id, status } = msg.payload;
      const target = programs.value.find(p => p.id === id);
      if (target) {
        target.status = status;
        if (operatingIds.value.has(id)) {
          operatingIds.value.delete(id);
        }
        // WS payload has status only — refresh to sync health_error / last_error / pid.
        // Especially important for Healthy ↔ Running (health_check fail/recover).
        void refreshProgramSummary(id);
      }
    }
    // 2. Log line
    else if (msg.type === 'Log') {
      const { id, source, line } = msg.payload;
      const callback = logListeners.get(id);
      if (callback) {
        const src = source === 'superd' ? 'superd' : source === 'stderr' ? 'stderr' : 'stdout';
        callback(line, src);
      }
      if (source === 'superd') {
        void refreshProgramSummary(id);
      }
    }
  }

  // --- Actions ---

  // 1. Fetch list
  async function fetchPrograms() {
    isLoading.value = true
    error.value = null
    try {
      const res = await apiClient.get<Program[]>(API_PATHS.PROGRAMS.LIST)
      programs.value = res.data || []
    } catch (err: any) {
      console.error('Fetch failed:', err)
      error.value = err.response?.data || err.message || 'Failed to connect to backend'
      programs.value = []
    } finally {
      isLoading.value = false
    }
  }

  // 2. Generic action (start/stop/restart)
  async function performAction(action: 'start' | 'stop' | 'restart', id: string) {
    if (operatingIds.value.has(id)) return;
    operatingIds.value.add(id)
    try {
      let url = '';
      switch (action) {
        case 'start': url = API_PATHS.PROGRAMS.START(id); break;
        case 'stop': url = API_PATHS.PROGRAMS.STOP(id); break;
        case 'restart': url = API_PATHS.PROGRAMS.RESTART(id); break;
      }
      await apiClient.post(url)
      // Immediately refresh summary so UI updates without waiting for WebSocket
      await refreshProgramSummary(id);
      operatingIds.value.delete(id);
    } catch (err: any) {
      console.error(`Failed to ${action}:`, err)
      await alertDialog(`Operation failed: ${err.message}`, {
        title: 'Operation failed',
        variant: 'destructive',
      })
      operatingIds.value.delete(id)
    }
  }

  // 3. Remove process (caller should confirm via UI dialog first)
  async function removeProgram(id: string) {
    if (operatingIds.value.has(id)) return;

    operatingIds.value.add(id)
    try {
      await apiClient.delete(API_PATHS.PROGRAMS.REMOVE(id))
      // Delete does not emit status events; refetch the list
      await fetchPrograms()
    } catch (err: any) {
      await alertDialog(`Remove failed: ${err.message}`, {
        title: 'Remove failed',
        variant: 'destructive',
      })
    } finally {
      operatingIds.value.delete(id)
    }
  }

  // 4. Log subscription management
  function subscribeLog(id: string, callback: LogCallback) {
    logListeners.set(id, callback);
  }

  async function refreshProgramSummary(id: string) {
    try {
      const res = await apiClient.get<ProgramDetail>(API_PATHS.PROGRAMS.DETAIL(id));
      const detail = res.data;
      const target = programs.value.find(p => p.id === id);
      if (!target) return;
      target.status = detail.state;
      target.last_error = detail.last_error;
      target.health_error = detail.health_error;
      target.pid = detail.pid;
    } catch (err) {
      console.error('Failed to refresh program summary:', err);
    }
  }

  function unsubscribeLog(id: string) {
    logListeners.delete(id);
  }

  // 5. Reload system configuration
  async function reloadSystem() {
    try {
      isLoading.value = true;
      await apiClient.post(API_PATHS.SYSTEM.RELOAD);
      // Refetch after reload in case config changed
      await fetchPrograms();
      await alertDialog('System configuration reloaded successfully.', {
        title: 'Reload complete',
      });
    } catch (err: any) {
      console.error('Reload failed:', err);
      await alertDialog(`Reload failed: ${err.message}`, {
        title: 'Reload failed',
        variant: 'destructive',
      });
    } finally {
      isLoading.value = false;
    }
  }

  return {
    programs,
    isLoading,
    error,
    operatingIds,
    isConnected,
    reloadSystem,
    fetchPrograms,
    removeProgram,
    subscribeLog,
    unsubscribeLog,
    refreshProgramSummary,
    startProgram: (id: string) => performAction('start', id),
    stopProgram: (id: string) => performAction('stop', id),
    restartProgram: (id: string) => performAction('restart', id),
  }
})
