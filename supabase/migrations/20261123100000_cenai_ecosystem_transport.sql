-- Durable replay protection and private per-app context for Ecosystem Protocol v1.
create table if not exists public.ecosystem_request_nonces (
  request_id text primary key check (request_id ~ '^[A-Za-z0-9._:-]{32,128}$'),
  user_id uuid not null references auth.users(id) on delete cascade,
  app_id text not null references public.ecosystem_apps(app_id) on delete cascade,
  created_at timestamptz not null
);
create index if not exists ecosystem_request_nonces_created_idx
  on public.ecosystem_request_nonces(created_at);

create table if not exists public.ecosystem_context_store (
  user_id uuid not null references auth.users(id) on delete cascade,
  app_id text not null references public.ecosystem_apps(app_id) on delete cascade,
  context_key text not null check (context_key ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'),
  value jsonb not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (user_id, app_id, context_key)
);
create index if not exists ecosystem_context_store_user_idx
  on public.ecosystem_context_store(user_id, app_id, updated_at desc);

alter table public.ecosystem_request_nonces enable row level security;
alter table public.ecosystem_context_store enable row level security;
revoke all on public.ecosystem_request_nonces from anon, authenticated;
revoke all on public.ecosystem_context_store from anon, authenticated;
grant all on public.ecosystem_request_nonces to service_role;
grant all on public.ecosystem_context_store to service_role;

-- Lock app/grant rows while consuming the nonce so concurrent revocation cannot
-- race an accepted request. The function is callable only by service_role.
create or replace function public.consume_ecosystem_request(
  p_user_id uuid,
  p_app_id text,
  p_capability text,
  p_request_id text,
  p_created_at timestamptz
) returns text
language plpgsql
security definer
set search_path = public
as $$
begin
  delete from public.ecosystem_request_nonces
    where created_at < now() - interval '10 minutes';

  perform 1
  from public.ecosystem_apps a
  join public.ecosystem_user_grants g on g.app_id = a.app_id
  where a.app_id = p_app_id
    and a.enabled = true
    and p_capability = any(a.capabilities)
    and p_capability = any(a.requested_scopes)
    and g.user_id = p_user_id
    and g.revoked = false
    and p_capability = any(g.granted_scopes)
  for share of a, g;

  if not found then return 'unauthorized'; end if;

  insert into public.ecosystem_request_nonces(request_id, user_id, app_id, created_at)
  values (p_request_id, p_user_id, p_app_id, p_created_at)
  on conflict (request_id) do nothing;

  if not found then return 'replay'; end if;
  return 'accepted';
end;
$$;
revoke all on function public.consume_ecosystem_request(uuid, text, text, text, timestamptz) from public, anon, authenticated;
grant execute on function public.consume_ecosystem_request(uuid, text, text, text, timestamptz) to service_role;
