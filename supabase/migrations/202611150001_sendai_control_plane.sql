-- Send.ai control plane bootstrap
-- Run in the Supabase SQL Editor after creating the project.
-- Never expose the Supabase secret/service-role key to the browser or Android app.

create table if not exists public.admin_users (
  user_id uuid primary key references auth.users(id) on delete cascade,
  role text not null check (role in ('owner','admin','operator')),
  is_active boolean not null default true,
  created_at timestamptz not null default now()
);

create table if not exists public.admin_feature_flags (
  key text primary key check (key ~ '^[a-z0-9._-]{2,80}$'),
  enabled boolean not null default false,
  description text not null default '',
  updated_by uuid references auth.users(id),
  updated_at timestamptz not null default now()
);

create table if not exists public.admin_audit_logs (
  id bigint generated always as identity primary key,
  actor_user_id uuid references auth.users(id) on delete set null,
  actor_email text not null,
  action text not null,
  details jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now()
);

alter table public.admin_users enable row level security;
alter table public.admin_feature_flags enable row level security;
alter table public.admin_audit_logs enable row level security;

-- No browser/authenticated role can read or mutate control-plane tables.
revoke all on public.admin_users from anon, authenticated;
revoke all on public.admin_feature_flags from anon, authenticated;
revoke all on public.admin_audit_logs from anon, authenticated;
grant all on public.admin_users to service_role;
grant all on public.admin_feature_flags to service_role;
grant all on public.admin_audit_logs to service_role;
grant usage, select on sequence public.admin_audit_logs_id_seq to service_role;

create index if not exists admin_audit_logs_created_at_idx
  on public.admin_audit_logs (created_at desc);
create index if not exists admin_audit_logs_actor_idx
  on public.admin_audit_logs (actor_user_id, created_at desc);

insert into public.admin_feature_flags(key, enabled, description) values
 ('agents.autonomy.enabled', false, 'Allow approved agent orchestration features'),
 ('guardian.diagnostics.enabled', false, 'Enable opt-in Guardian diagnostics ingestion'),
 ('releases.beta.enabled', false, 'Enable beta release channel'),
 ('releases.experimental.enabled', false, 'Enable experimental release channel'),
 ('api.public.enabled', false, 'Enable public CenAI API access'),
 ('billing.enabled', false, 'Enable paid plans and billing')
on conflict (key) do nothing;

-- Bootstrap the first owner only after creating their account in Supabase Auth.
-- Replace the UUID with that account's auth.users.id, then run:
-- insert into public.admin_users(user_id, role) values ('YOUR-AUTH-USER-UUID', 'owner');
