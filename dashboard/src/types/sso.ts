/** SSO (OIDC) API shapes — mirror of `common::auth_sso` and the routes. */

export type SsoRole = 'viewer' | 'operator' | 'admin'
export type SsoGate = 'none' | 'secret' | 'plugin' | 'sso'

export interface RoleRuleMatch {
  sub?: string
  email?: string
  groups_any?: string[]
}

export interface RoleRule {
  match: RoleRuleMatch
  role: SsoRole
}

export interface SsoConfig {
  enabled: boolean
  issuer: string
  client_id: string
  public_base_url: string
  scopes: string
  groups_claim: string
  token_auth_method: 'client_secret_basic' | 'client_secret_post'
  role_rules: RoleRule[]
  session_ttl_secs: number
  idle_timeout_secs: number
  allow_insecure_issuer: boolean
}

export interface SsoConfigView {
  configured: boolean
  config: SsoConfig | null
  redirect_uri: string | null
  secret_updated_at: number | null
  updated_at: number | null
  updated_by: string | null
  gate: SsoGate
  break_glass: boolean
}

export interface SsoSession {
  id: string
  sub: string
  email: string | null
  name: string | null
  role: SsoRole | null
  created_at: number
  last_seen_at: number
  expires_at: number
  ip: string | null
  user_agent: string | null
  current: boolean
}

export interface SsoAuditEvent {
  at: number
  kind: string
  sub: string | null
  email: string | null
  ip: string | null
  user_agent: string | null
  detail: string | null
}

export interface AuthStatusLite {
  mode: SsoGate
  auth_secret_login_allowed?: boolean
  sso?: {
    enabled: boolean
    available: boolean
    login_path: string
  } | null
}
