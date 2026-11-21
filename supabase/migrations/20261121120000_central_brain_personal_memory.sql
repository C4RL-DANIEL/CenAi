-- Per-account Central Brain personal memory. All access is mediated by the authenticated control-plane function.
create table if not exists public.user_memory_settings (
  user_id uuid primary key references auth.users(id) on delete cascade,
  enabled boolean not null default false,
  updated_at timestamptz not null default now()
);
create table if not exists public.user_personal_memories (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  category text not null check (category in ('preference','profile','project','workflow','fact')),
  content text not null check (char_length(content) between 1 and 8000),
  source text not null default 'user' check (source in ('user','assistant_confirmed','imported')),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);
create index if not exists user_personal_memories_owner_updated_idx
  on public.user_personal_memories(user_id, updated_at desc);
alter table public.user_memory_settings enable row level security;
alter table public.user_personal_memories enable row level security;
revoke all on public.user_memory_settings from anon, authenticated;
revoke all on public.user_personal_memories from anon, authenticated;
grant all on public.user_memory_settings to service_role;
grant all on public.user_personal_memories to service_role;
