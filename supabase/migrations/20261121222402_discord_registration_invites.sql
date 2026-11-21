-- Snapshot the Discord-only registration cutover. Existing accounts created
-- before this timestamp remain valid; all later accounts must redeem an invite.
create table if not exists public.registration_policy (
  id smallint primary key check (id = 1),
  enforced_from timestamptz not null default now(),
  updated_at timestamptz not null default now()
);
insert into public.registration_policy(id, enforced_from)
values (1, now())
on conflict (id) do nothing;
alter table public.registration_policy enable row level security;
revoke all on public.registration_policy from public, anon, authenticated;
grant all on public.registration_policy to service_role;

-- Owner-issued Discord registration invitations.
-- Raw invitation tokens are returned once; only SHA-256 hashes are stored.
create table if not exists public.registration_invites (
  id uuid primary key default gen_random_uuid(),
  token_hash text not null unique check (token_hash ~ '^[0-9a-f]{64}$'),
  created_by uuid not null references auth.users(id) on delete restrict,
  label text not null default '' check (char_length(label) <= 120),
  expires_at timestamptz not null,
  redeemed_by uuid references auth.users(id) on delete set null,
  redeemed_at timestamptz,
  revoked_at timestamptz,
  created_at timestamptz not null default now(),
  constraint registration_invite_redemption_pair check (
    (redeemed_by is null and redeemed_at is null) or
    (redeemed_by is not null and redeemed_at is not null)
  )
);
create index if not exists registration_invites_created_idx
  on public.registration_invites(created_at desc);
create index if not exists registration_invites_expiry_idx
  on public.registration_invites(expires_at)
  where redeemed_at is null and revoked_at is null;

alter table public.registration_invites enable row level security;
revoke all on public.registration_invites from public, anon, authenticated;
grant all on public.registration_invites to service_role;

-- Atomic, race-safe, single-use redemption. A retry by the same Discord user
-- is idempotent so a transient Auth metadata write can be safely retried.
create or replace function public.cenai_redeem_registration_invite(
  p_token_hash text,
  p_user_id uuid
) returns boolean
language plpgsql
security invoker
set search_path = ''
as $function$
declare
  invite_row public.registration_invites%rowtype;
begin
  if coalesce(auth.jwt() ->> 'role', '') <> 'service_role' then
    raise exception 'service role required';
  end if;
  if p_token_hash !~ '^[0-9a-f]{64}$' or p_user_id is null then
    return false;
  end if;

  select * into invite_row
  from public.registration_invites
  where token_hash = p_token_hash
  for update;

  if not found or invite_row.revoked_at is not null then
    return false;
  end if;
  if invite_row.redeemed_at is not null then
    return invite_row.redeemed_by = p_user_id;
  end if;
  if invite_row.expires_at <= now() then
    return false;
  end if;

  update public.registration_invites
  set redeemed_by = p_user_id, redeemed_at = now()
  where id = invite_row.id and redeemed_at is null and revoked_at is null;

  return found;
end
$function$;

revoke all on function public.cenai_redeem_registration_invite(text,uuid) from public, anon, authenticated;
grant execute on function public.cenai_redeem_registration_invite(text,uuid) to service_role;
