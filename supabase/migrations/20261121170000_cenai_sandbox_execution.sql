-- Minimal metadata-only audit/quota table for ephemeral Vercel Sandbox executions.
create table if not exists public.cenai_sandbox_runs (
  run_id uuid primary key,
  user_id uuid not null references auth.users(id) on delete cascade,
  status text not null check (status in ('running','succeeded','failed','timed_out','cancelled')),
  started_at timestamptz not null default now(),
  finished_at timestamptz,
  duration_ms integer check (duration_ms is null or duration_ms between 0 and 120000),
  exit_code integer check (exit_code is null or exit_code between -255 and 255),
  created_at timestamptz not null default now()
);

create index if not exists cenai_sandbox_runs_user_created_idx
  on public.cenai_sandbox_runs (user_id, created_at desc);
create index if not exists cenai_sandbox_runs_active_idx
  on public.cenai_sandbox_runs (status, started_at) where status = 'running';

alter table public.cenai_sandbox_runs enable row level security;
revoke all on public.cenai_sandbox_runs from anon, authenticated;
grant all on public.cenai_sandbox_runs to service_role;

create or replace function public.cenai_registration_allowed()
returns boolean
language sql
stable
security definer
set search_path = auth, public
as $$
  select exists (
    select 1
    from auth.users u
    join public.registration_policy p on p.id = 1
    where u.id = auth.uid()
      and p.enforced_from is not null
      and (
        u.created_at < p.enforced_from
        or (
          u.raw_app_meta_data ->> 'cenai_registration_verified' = 'true'
          and u.raw_app_meta_data ->> 'cenai_registration_method' = 'discord'
        )
      )
  );
$$;

create or replace function public.cenai_claim_sandbox_run()
returns uuid
language plpgsql
security definer
set search_path = public, auth
as $$
declare
  v_user_id uuid := auth.uid();
  v_run_id uuid := gen_random_uuid();
begin
  if v_user_id is null then return null; end if;
  perform pg_advisory_xact_lock(0);
  perform pg_advisory_xact_lock(hashtextextended(v_user_id::text, 0));

  if (select count(*) from public.cenai_sandbox_runs
      where user_id = v_user_id and created_at > now() - interval '1 hour') >= 8 then
    return null;
  end if;

  if (select count(*) from public.cenai_sandbox_runs
      where status = 'running' and started_at > now() - interval '2 minutes') >= 8 then
    return null;
  end if;

  if exists (select 1 from public.cenai_sandbox_runs
      where user_id = v_user_id and status = 'running'
        and started_at > now() - interval '2 minutes') then
    return null;
  end if;

  insert into public.cenai_sandbox_runs(run_id, user_id, status)
  values (v_run_id, v_user_id, 'running');
  return v_run_id;
end;
$$;

create or replace function public.cenai_finish_sandbox_run(
  p_run_id uuid,
  p_status text,
  p_duration_ms integer,
  p_exit_code integer
)
returns boolean
language plpgsql
security definer
set search_path = public, auth
as $$
begin
  if auth.uid() is null or p_status not in ('succeeded','failed','timed_out','cancelled') then
    return false;
  end if;
  update public.cenai_sandbox_runs
  set status = p_status,
      finished_at = now(),
      duration_ms = greatest(0, least(coalesce(p_duration_ms, 0), 120000)),
      exit_code = case when p_exit_code between -255 and 255 then p_exit_code else null end
  where run_id = p_run_id and user_id = auth.uid() and status = 'running';
  return found;
end;
$$;

revoke all on function public.cenai_registration_allowed() from public, anon;
revoke all on function public.cenai_claim_sandbox_run() from public, anon;
revoke all on function public.cenai_finish_sandbox_run(uuid,text,integer,integer) from public, anon;
grant execute on function public.cenai_registration_allowed() to authenticated;
grant execute on function public.cenai_claim_sandbox_run() to authenticated;
grant execute on function public.cenai_finish_sandbox_run(uuid,text,integer,integer) to authenticated;
