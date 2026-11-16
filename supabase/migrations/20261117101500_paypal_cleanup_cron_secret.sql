create table if not exists public.cenai_internal_secrets (
  name text primary key,
  secret_value text not null,
  updated_at timestamptz not null default now()
);
alter table public.cenai_internal_secrets enable row level security;
revoke all on public.cenai_internal_secrets from anon, authenticated;
grant all on public.cenai_internal_secrets to service_role;
