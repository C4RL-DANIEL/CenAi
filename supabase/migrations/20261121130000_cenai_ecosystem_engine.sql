-- Shared app catalog with per-user, revocable capability grants.
create table if not exists public.ecosystem_trusted_publishers (
  publisher_id text primary key check (publisher_id ~ '^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$'),
  owner_user_id uuid references auth.users(id) on delete set null,
  enabled boolean not null default false,
  created_at timestamptz not null default now()
);
create table if not exists public.ecosystem_apps (
  app_id text primary key check (app_id ~ '^[A-Za-z0-9][A-Za-z0-9._-]{0,159}$'),
  publisher_id text not null references public.ecosystem_trusted_publishers(publisher_id),
  version text not null check (char_length(version) between 1 and 64),
  protocol_version integer not null check (protocol_version = 1),
  description text not null default '' check (char_length(description) <= 1000),
  capabilities text[] not null check (cardinality(capabilities) between 1 and 64),
  requested_scopes text[] not null check (cardinality(requested_scopes) between 1 and 64),
  enabled boolean not null default true,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);
create table if not exists public.ecosystem_user_grants (
  user_id uuid not null references auth.users(id) on delete cascade,
  app_id text not null references public.ecosystem_apps(app_id) on delete cascade,
  granted_scopes text[] not null default '{}',
  revoked boolean not null default false,
  updated_at timestamptz not null default now(),
  primary key (user_id, app_id)
);
create table if not exists public.ecosystem_audit_logs (
  id bigint generated always as identity primary key,
  actor_user_id uuid references auth.users(id) on delete set null,
  app_id text,
  action text not null,
  details jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now()
);
create index if not exists ecosystem_grants_user_idx on public.ecosystem_user_grants(user_id, updated_at desc);
create index if not exists ecosystem_apps_publisher_idx on public.ecosystem_apps(publisher_id, enabled);
alter table public.ecosystem_trusted_publishers enable row level security;
alter table public.ecosystem_apps enable row level security;
alter table public.ecosystem_user_grants enable row level security;
alter table public.ecosystem_audit_logs enable row level security;
revoke all on public.ecosystem_trusted_publishers from anon, authenticated;
revoke all on public.ecosystem_apps from anon, authenticated;
revoke all on public.ecosystem_user_grants from anon, authenticated;
revoke all on public.ecosystem_audit_logs from anon, authenticated;
grant all on public.ecosystem_trusted_publishers to service_role;
grant all on public.ecosystem_apps to service_role;
grant all on public.ecosystem_user_grants to service_role;
grant all on public.ecosystem_audit_logs to service_role;
grant usage, select on sequence public.ecosystem_audit_logs_id_seq to service_role;
