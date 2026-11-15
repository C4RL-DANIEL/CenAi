-- Per-account explicit consent for optional aggregate telemetry and Guardian reports.
create table if not exists public.user_telemetry_consents (
  user_id uuid primary key references auth.users(id) on delete cascade,
  usage_analytics_enabled boolean not null default false,
  guardian_enabled boolean not null default false,
  updated_at timestamptz not null default now()
);
alter table public.user_telemetry_consents enable row level security;
revoke all on public.user_telemetry_consents from anon, authenticated;
grant all on public.user_telemetry_consents to service_role;
