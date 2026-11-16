-- CenAI website CMS, public API quotas, and PayPal webhook diagnostics.
-- This migration was applied to the production Supabase project on 2026-11-16.

create table if not exists public.site_pages (
  slug text primary key check (slug ~ '^[a-z0-9][a-z0-9-]{0,79}$'),
  title text not null,
  description text not null default '',
  content jsonb not null default '{}'::jsonb,
  published boolean not null default false,
  updated_by uuid references auth.users(id) on delete set null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

alter table public.site_pages enable row level security;
drop policy if exists "Public can read published site pages" on public.site_pages;
create policy "Public can read published site pages"
  on public.site_pages for select to anon, authenticated
  using (published = true);
grant select on public.site_pages to anon, authenticated;

insert into public.site_pages (slug, title, description, content, published)
values (
  'home',
  'CenAI — Make AI work like a team',
  'Bring models, knowledge, projects, and capable agents into one calm, connected workspace. Choose the tools. Keep control of the work.',
  '{"heroTitle":"Make AI work like a team.","heroDescription":"Bring models, knowledge, projects, and capable agents into one calm, connected workspace. Choose the tools. Keep control of the work.","announcement":"Early access is open"}'::jsonb,
  true
)
on conflict (slug) do nothing;

create table if not exists public.ai_quota_policies (
  owner_user_id uuid primary key references auth.users(id) on delete cascade,
  daily_request_limit integer not null default 100
    check (daily_request_limit between 1 and 1000000),
  monthly_token_limit bigint not null default 500000
    check (monthly_token_limit between 1000 and 100000000000),
  enabled boolean not null default true,
  updated_by uuid references auth.users(id) on delete set null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

alter table public.ai_quota_policies enable row level security;
create index if not exists usage_events_owner_created_idx
  on public.usage_events(owner_user_id, created_at desc);

alter table public.billing_webhook_events
  add column if not exists event_type text,
  add column if not exists resource_id text,
  add column if not exists processing_result text;
