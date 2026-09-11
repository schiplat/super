export interface WebhookConfig {
  url: string;
  secret?: string;
  headers?: Record<string, string>;
}

export interface ThrottleStrategy {
  mode: 'immediate' | 'cooldown' | 'batch';
  cooldown_secs?: number;   // Cooldown mode
  window_secs?: number;     // Batch mode
  max_events?: number;      // Batch mode
}

export interface ChannelConfig {
  id: string;
  name: string;
  type: 'webhook' | 'dingtalk' | 'lark' | 'feishu' | 'slack' | 'wecom' | 'wechat_work' | 'teams';
  triggers: string[];
  include_log_tail: boolean;
  config: WebhookConfig;

  /** Deprecated — use strategy instead */
  cooldown_secs: number;
  /** Notification delivery strategy */
  strategy: ThrottleStrategy;
}

export const NOTIFICATION_TYPES = [
  { value: 'webhook' as const, label: 'Generic Webhook', urlHint: 'https://api.example.com/webhook', showSecret: true, secretLabel: 'HMAC-SHA256 signing secret' },
  { value: 'dingtalk' as const, label: 'DingTalk', urlHint: 'https://oapi.dingtalk.com/robot/send?access_token=...', showSecret: true, secretLabel: 'DingTalk signing secret' },
  { value: 'feishu' as const, label: 'Feishu / Lark', urlHint: 'https://open.feishu.cn/open-apis/bot/v2/hook/...', showSecret: true, secretLabel: 'Feishu signing secret' },
  { value: 'slack' as const, label: 'Slack', urlHint: 'https://hooks.slack.com/services/...', showSecret: true, secretLabel: 'Slack signing secret' },
  { value: 'wecom' as const, label: 'WeCom (WeChat Work)', urlHint: 'https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=...', showSecret: false, secretLabel: '' },
  { value: 'teams' as const, label: 'Microsoft Teams', urlHint: 'https://...webhook.office.com/webhookb2/...', showSecret: true, secretLabel: 'HMAC-SHA256 signing secret' },
] as const;

export interface InhibitionRule {
  id: string;
  sources: string[];
  targets: string[];
  match_on: string[];
  ttl_secs: number;
}

export interface NotificationConfig {
  channels: ChannelConfig[];
  inhibition_rules: InhibitionRule[];
  delivery_keep_days?: number;
  delivery_db?: string;
}

export interface StatsSnapshot {
  ts: number;       // unix timestamp seconds
  success: number;
  failed: number;
}

export interface ChannelStat {
  success: number;
  failed: number;
  suppressed: number;
  aggregated: number;
}

export interface NotifyStats {
  success: number;
  failed: number;
  suppressed: number;
  snapshots: StatsSnapshot[];
  channels: Record<string, ChannelStat>;
}

export type DeliveryOutcome = 'ok' | 'fail' | 'cooldown' | 'inhibited';

export interface DeliveryRecord {
  id: number;
  ts_ms: number;
  outcome: DeliveryOutcome;
  channel_id: string | null;
  channel_name: string | null;
  channel_type: string | null;
  event_type: string;
  event_id: string | null;
  program_id: string | null;
  program_name: string | null;
  http_status: number | null;
  error: string | null;
  latency_ms: number | null;
  detail: string | null;
}

export interface DeliveryStats {
  total: number;
  ok: number;
  fail: number;
  cooldown: number;
  inhibited: number;
  keep_days: number;
}

export const TRIGGER_OPTIONS = [
  { value: '*', label: 'All Events' },
  { value: 'process_fatal', label: 'Process Fatal / Crash' },
  { value: 'process_backoff', label: 'Process Restarting (Backoff)' },
  { value: 'process_recovered', label: 'Process Recovered' },
  { value: 'health_restart', label: 'Health Check Restart' },
  { value: 'system_startup', label: 'System Startup' },
  { value: 'system_shutdown', label: 'System Shutdown' },
];
