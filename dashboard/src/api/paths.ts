import type { ProgramEventsQuery } from '@/types';

const BASE_URL = '/api/v1';

export const API_PATHS = {
  SYSTEM: {
    RELOAD: `${BASE_URL}/system/reload`,
    STATS: `${BASE_URL}/system/stats`,
    LICENSE: `${BASE_URL}/system/license`,
  },

  // Programs
  PROGRAMS: {
    // List GET /api/v1/programs
    LIST: `${BASE_URL}/programs`,

    // Create POST /api/v1/programs
    CREATE: `${BASE_URL}/programs`,

    // Detail GET /api/v1/programs/:id
    DETAIL: (id: string) => `${BASE_URL}/programs/${id}`,

    // Actions POST /api/v1/programs/:id/start|stop|restart
    START: (id: string) => `${BASE_URL}/programs/${id}/start`,
    STOP: (id: string) => `${BASE_URL}/programs/${id}/stop`,
    RESTART: (id: string) => `${BASE_URL}/programs/${id}/restart`,

    // Delete DELETE /api/v1/programs/:id
    REMOVE: (id: string) => `${BASE_URL}/programs/${id}`,

    // GET /api/v1/programs/:id/logs?tail=N
    LOGS: (id: string, tail = 200, source?: string) => {
      const params = new URLSearchParams({ tail: String(tail) });
      if (source) params.set('source', source);
      return `${BASE_URL}/programs/${id}/logs?${params}`;
    },

    // GET /api/v1/programs/:id/events
    EVENTS: (id: string, query?: ProgramEventsQuery) => {
      const params = new URLSearchParams();
      if (query?.from != null) params.set('from', String(query.from));
      if (query?.to != null) params.set('to', String(query.to));
      if (query?.event_type) params.set('event_type', query.event_type);
      if (query?.exit_code != null) params.set('exit_code', String(query.exit_code));
      if (query?.q) params.set('q', query.q);
      if (query?.limit != null) params.set('limit', String(query.limit));
      if (query?.offset != null) params.set('offset', String(query.offset));
      if (query?.sort_by) params.set('sort_by', query.sort_by);
      if (query?.order) params.set('order', query.order);
      const qs = params.toString();
      return qs
        ? `${BASE_URL}/programs/${id}/events?${qs}`
        : `${BASE_URL}/programs/${id}/events`;
    },
  },

  EVENTS: {
    STATS: (programId?: string) => {
      const params = new URLSearchParams();
      if (programId) params.set('program_id', programId);
      const qs = params.toString();
      return qs ? `${BASE_URL}/events/stats?${qs}` : `${BASE_URL}/events/stats`;
    },
  },
};
