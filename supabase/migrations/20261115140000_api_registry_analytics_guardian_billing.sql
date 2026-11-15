-- CenAI API, model registry, usage analytics, Guardian and billing foundations.
-- All records are server-only; public/mobile clients never receive service-role access.

create table if not exists public.model_registry (
  id uuid primary key default gen_random_uuid(),
  slug text not null unique check (slug ~ '^[a-z0-9][a-z0-9._-]{1,100}$'),
  provider text not null check (provider in ('openai','anthropic','google','deepseek','custom','local')),
  model_id text not null check (char_length(model_id) between 1 and 200),
  display_name text not null check (char_length(display_name) between 1 and 120),
  description text not null default '' check (char_length(description) <= 1000),
  tasks text[] not null default array['chat']::text[],
  modalities text[] not null default array['text']::text[],
  enabled boolean not null default false,
  public boolean not null default false,
  config jsonb not null default '{}'::jsonb,
  created_by uuid references auth.users(id) on delete set null,
  updated_by uuid references auth.users(id) on delete set null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);
create index if not exists model_registry_enabled_provider_idx on public.model_registry(enabled,provider,slug);

create table if not exists public.api_keys (
  id uuid primary key default gen_random_uuid(),
  owner_user_id uuid not null references auth.users(id) on delete cascade,
  name text not null check (char_length(name) between 1 and 80),
  key_prefix text not null,
  key_hash text not null unique check (key_hash ~ '^[a-f0-9]{64}$'),
  scopes text[] not null default array['models:read','usage:read']::text[],
  enabled boolean not null default true,
  created_at timestamptz not null default now(),
  last_used_at timestamptz,
  revoked_at timestamptz
);
create index if not exists api_keys_owner_created_idx on public.api_keys(owner_user_id,created_at desc);

create table if not exists public.usage_events (
  id bigint generated always as identity primary key,
  owner_user_id uuid references auth.users(id) on delete set null,
  api_key_id uuid references public.api_keys(id) on delete set null,
  source text not null default 'android' check (source in ('android','public_api','web','agent','system')),
  event_type text not null check (char_length(event_type) between 1 and 80),
  provider text not null default '',
  model_id text not null default '',
  input_tokens integer not null default 0 check (input_tokens >= 0),
  output_tokens integer not null default 0 check (output_tokens >= 0),
  latency_ms integer not null default 0 check (latency_ms >= 0),
  status text not null default 'success' check (status in ('success','error','cancelled','blocked')),
  error_code text not null default '',
  metadata jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now()
);
create index if not exists usage_events_created_idx on public.usage_events(created_at desc);
create index if not exists usage_events_owner_created_idx on public.usage_events(owner_user_id,created_at desc);

create table if not exists public.guardian_reports (
  id bigint generated always as identity primary key,
  user_id uuid not null references auth.users(id) on delete cascade,
  app_version text not null default '',
  android_version text not null default '',
  battery_percent smallint check (battery_percent between 0 and 100),
  available_ram_mb integer check (available_ram_mb >= 0),
  free_storage_mb bigint check (free_storage_mb >= 0),
  network_type text not null default '',
  crash_category text not null default '',
  created_at timestamptz not null default now()
);
create index if not exists guardian_reports_user_created_idx on public.guardian_reports(user_id,created_at desc);

create table if not exists public.billing_customers (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null unique references auth.users(id) on delete cascade,
  provider text not null check (provider in ('stripe','paddle')),
  provider_customer_id text not null unique,
  email text not null default '',
  created_at timestamptz not null default now()
);
create table if not exists public.billing_subscriptions (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  provider text not null check (provider in ('stripe','paddle')),
  provider_subscription_id text not null unique,
  plan_key text not null,
  status text not null,
  current_period_end timestamptz,
  cancel_at_period_end boolean not null default false,
  updated_at timestamptz not null default now()
);
create index if not exists billing_subscriptions_user_idx on public.billing_subscriptions(user_id,status);
create table if not exists public.billing_webhook_events (
  provider text not null check (provider in ('stripe','paddle')),
  event_id text not null,
  processed_at timestamptz not null default now(),
  primary key(provider,event_id)
);

do $$
declare t text;
begin
  foreach t in array array['model_registry','api_keys','usage_events','guardian_reports','billing_customers','billing_subscriptions','billing_webhook_events'] loop
    execute format('alter table public.%I enable row level security', t);
    execute format('revoke all on public.%I from anon, authenticated', t);
    execute format('grant all on public.%I to service_role', t);
  end loop;
end $$;

grant usage, select on sequence public.usage_events_id_seq to service_role;
grant usage, select on sequence public.guardian_reports_id_seq to service_role;

insert into public.admin_feature_flags(key,enabled,description) values
 ('models.registry.enabled',true,'Enable the server-managed CenAI model registry'),
 ('analytics.usage.enabled',true,'Enable privacy-minimized request usage analytics'),
 ('guardian.ingestion.enabled',false,'Allow explicitly opted-in device health reports'),
 ('api.public.enabled',false,'Enable public CenAI API keys and model listing'),
 ('billing.enabled',false,'Enable paid plans only after provider credentials and verified webhooks are configured')
on conflict(key) do nothing;
