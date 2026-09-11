export type ProcessStatus =
  | 'Stopped' | 'Starting' | 'Running' | 'Backoff'
  | 'Fatal' | 'Stopping' | 'Waiting' | 'Healthy';

export interface Program {
  id: string;
  name: string;
  group?: string;
  status: ProcessStatus;

  // Matches the Rust backend schema
  pid?: number;
  created_at?: number;
  uptime_sec?: number;
  updated_at: number;
  last_error?: string;
  health_error?: string;

  cpu_usage?: number;
  mem_usage?: number;
  depends_on?: string[];
  resource_limits?: {
    cpu_quota?: number;
    memory_limit?: number;
    memory_warn_percent?: number;
    memory_warn_headroom?: number;
    memory_high?: number;
  };
}

// WebSocket message types
export type WsMessageType = 'StatusChange' | 'Log';

export interface StatusChangePayload {
  id: string;
  status: ProcessStatus; // Reuses ProcessStatus above
  name: string;
}

export interface LogPayload {
  id: string;
  source: 'stdout' | 'stderr' | 'superd';
  line: string;
}

// Union type matching the Rust enum
export type WsMessage =
  | { type: 'StatusChange'; payload: StatusChangePayload }
  | { type: 'Log'; payload: LogPayload };

export interface ProgramLogFile {
  source: string;
  content: string;
}

export interface ProgramLogsResponse {
  id: string;
  logs: ProgramLogFile[];
}

// Nested health check configuration (Rust HealthCheck + tuning knobs)
export interface HealthCheckConfig {
  type: 'tcp' | 'http' | 'exec';
  host?: string;
  port?: number;
  url?: string;
  method?: string;
  command?: string;
  // Tuning (0 = default; max_failures 0 disables auto-restart)
  interval_secs?: number;
  timeout_secs?: number;
  start_period_secs?: number;
  max_failures?: number;
}

// Persisted lifecycle/exception event history (Rust ProgramEventRecord)
export interface ProgramEventRecord {
  ts: number;
  ts_ms?: number;
  program_id?: string | null;
  program_name?: string | null;
  event: string;
  exit_code?: number | null;
  signal?: number | null;
  retry_count?: number | null;
  duration_secs?: number | null;
  msg: string;
}

export interface EventStats {
  total: number;
  by_type: { event: string; count: number }[];
  first_ts?: number | null;
  last_ts?: number | null;
}

export interface ProgramEventsQuery {
  from?: number;
  to?: number;
  event_type?: string;
  exit_code?: number;
  q?: string;
  limit?: number;
  offset?: number;
  sort_by?: 'time' | 'event' | 'exit_code' | 'signal' | 'retry_count' | 'duration_secs' | 'msg';
  order?: 'asc' | 'desc';
}

// Full config shape (Rust ProgramConfig)
export interface ProgramConfig {
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  cwd?: string;
  user?: string;
  group?: string;
  autostart: boolean;
  retry_limit: number;
  autorestart?: 'unexpected' | 'true' | 'false';
  exitcodes?: number[];
  startsecs?: number;
  stopsecs?: number;
  priority?: number;
  stdout_logfile?: string;
  stderr_logfile?: string;
  depends_on: string[];

  // Nested objects
  hooks?: {
    pre_start?: string;
    post_start?: string;
    pre_stop?: string;
    post_stop?: string;
  };

  // Timestamps
  created_at: number;
  updated_at: number;
  health_check?: HealthCheckConfig;

  cron?: string;
  on_overlap?: 'skip' | 'queue' | 'kill';
  catchup?: 'skip' | 'latest' | 'all';
  jitter_sec?: number;
  max_concurrent?: number;
  max_queued?: number;
  resource_limits?: {
    cpu_quota?: number;
    memory_limit?: number;
    memory_warn_percent?: number;
    memory_warn_headroom?: number;
    memory_high?: number;
  };
  artifact?: {
    source: string;
    checksum: string;
    extract: boolean;
    destination: string;
    restart_policy: string;
    download_timeout?: number;
    verify_timeout?: number;
  };
  /** Set while an OTA swap is verifying / pending rollback (WAL). */
  restore_path?: string | null;
}

// Detail API response (Rust ProgramInfo)
export interface ProgramDetail {
  id: string;
  state: ProcessStatus;
  pid?: number;
  config: ProgramConfig; // All static configuration
  last_error?: string;
  health_error?: string;
}

// Create request body (CreateProgramRequest)
export interface CreateProgramRequest {
  name?: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  cwd?: string;
  user?: string;
  group?: string;
  autostart: boolean;
  retry_limit: number;
  depends_on: string[];
  autorestart?: 'unexpected' | 'true' | 'false';
  exitcodes?: number[];
  startsecs?: number;
  stopsecs?: number;
  priority?: number;
  stdout_logfile?: string;
  stderr_logfile?: string;

  // Batch creation
  numprocs: number;
  process_name?: string; // e.g. "worker-{num}"

  // Nested objects
  hooks: {
    pre_start?: string;
    post_start?: string;
    pre_stop?: string;
    post_stop?: string;
  };
  health_check?: HealthCheckConfig;

  // Premium
  cron?: string;
  on_overlap?: 'skip' | 'queue' | 'kill';
  catchup?: 'skip' | 'latest' | 'all';
  jitter_sec?: number;
  max_concurrent?: number;
  max_queued?: number;
  resource_limits?: {
    cpu_quota?: number;
    memory_limit?: number;
    memory_warn_percent?: number;
    memory_warn_headroom?: number;
    memory_high?: number;
  };

  // OTA upgrade
  artifact?: {
    source: string;
    checksum: string;
    extract: boolean;
    destination: string;
    restart_policy: string;
    download_timeout?: number;
    verify_timeout?: number;
  };
}

// Update request body (UpdateProgramRequest)
// All fields optional (PATCH / partial update semantics)
export interface UpdateProgramRequest {
  name?: string;
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
  user?: string;
  group?: string;
  autostart?: boolean;
  retry_limit?: number;
  depends_on?: string[]; // UI does not edit dependencies yet; type kept for API
  autorestart?: 'unexpected' | 'true' | 'false';
  exitcodes?: number[];
  startsecs?: number;
  stopsecs?: number;
  priority?: number;
  stdout_logfile?: string;
  stderr_logfile?: string;

  hooks?: {
    pre_start?: string;
    post_start?: string;
    pre_stop?: string;
    post_stop?: string;
  };

  health_check?: {
    type: 'tcp' | 'http' | 'exec' | 'disabled';
    host?: string;
    port?: number;
    url?: string;
    method?: string;
    command?: string;
    interval_secs?: number;
    timeout_secs?: number;
    start_period_secs?: number;
    max_failures?: number;
  };

  cron?: string;

  on_overlap?: 'skip' | 'queue' | 'kill';
  catchup?: 'skip' | 'latest' | 'all';
  jitter_sec?: number;
  max_concurrent?: number;
  max_queued?: number;

  // Artifact is usually set by OTA; type kept for API completeness
  artifact?: {
    source: string;
    checksum: string;
    extract: boolean;
    destination: string;
    restart_policy: string;
    download_timeout?: number;
    verify_timeout?: number;
  };

  resource_limits?: {
    cpu_quota?: number;
    memory_limit?: number;
    memory_warn_percent?: number;
    memory_warn_headroom?: number;
    memory_high?: number;
  };
}
