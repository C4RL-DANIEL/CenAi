create table if not exists public.cenai_sandbox_workspaces (
  user_id uuid primary key references auth.users(id) on delete cascade,
  snapshot_id text not null check (snapshot_id ~ '^snap_[A-Za-z0-9_-]{8,96}$'),
  updated_at timestamptz not null default now()
);

alter table public.cenai_sandbox_workspaces enable row level security;
revoke all on public.cenai_sandbox_workspaces from anon, authenticated;
grant all on public.cenai_sandbox_workspaces to service_role;

create or replace function public.cenai_get_sandbox_workspace()
returns text
language plpgsql
stable
security definer
set search_path = public, auth
as $$
declare
  v_user_id uuid := auth.uid();
  v_snapshot_id text;
begin
  if v_user_id is null then return null; end if;
  select snapshot_id into v_snapshot_id
  from public.cenai_sandbox_workspaces
  where user_id = v_user_id;
  return v_snapshot_id;
end;
$$;

create or replace function public.cenai_save_sandbox_workspace(p_snapshot_id text)
returns boolean
language plpgsql
security definer
set search_path = public, auth
as $$
declare
  v_user_id uuid := auth.uid();
begin
  if v_user_id is null
    or p_snapshot_id is null
    or length(p_snapshot_id) > 105
    or p_snapshot_id !~ '^snap_[A-Za-z0-9_-]{8,96}$'
  then
    return false;
  end if;

  insert into public.cenai_sandbox_workspaces(user_id, snapshot_id, updated_at)
  values (v_user_id, p_snapshot_id, now())
  on conflict (user_id) do update
    set snapshot_id = excluded.snapshot_id,
        updated_at = now();
  return true;
end;
$$;

revoke all on function public.cenai_get_sandbox_workspace() from public, anon;
revoke all on function public.cenai_save_sandbox_workspace(text) from public, anon;
grant execute on function public.cenai_get_sandbox_workspace() to authenticated;
grant execute on function public.cenai_save_sandbox_workspace(text) to authenticated;
