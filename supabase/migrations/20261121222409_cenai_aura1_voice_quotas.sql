-- Aura-1 voice quota: reserve before inference and enforce a shared daily cap.
-- This allocation reserves 4,000 of the 10,000 free daily Workers AI Neurons for TTS,
-- leaving the remainder for chat/other Workers AI workloads and safety headroom.
create table if not exists public.cenai_voice_usage_events (
  id bigint generated always as identity primary key,
  owner_user_id uuid not null references auth.users(id) on delete cascade,
  characters integer not null check (characters between 1 and 1000),
  neurons numeric(14,2) not null check (neurons > 0),
  status text not null default 'reserved' check (status in ('reserved','success','error')),
  created_at timestamptz not null default now(),
  completed_at timestamptz
);
create index if not exists cenai_voice_usage_daily_idx
  on public.cenai_voice_usage_events (created_at);
create index if not exists cenai_voice_usage_user_daily_idx
  on public.cenai_voice_usage_events (owner_user_id, created_at);
alter table public.cenai_voice_usage_events enable row level security;
revoke all on public.cenai_voice_usage_events from public, anon, authenticated;
grant select, insert, update, delete on public.cenai_voice_usage_events to service_role;
grant usage, select on sequence public.cenai_voice_usage_events_id_seq to service_role;

create or replace function public.cenai_reserve_voice_usage(
  p_user_id uuid,
  p_characters integer,
  p_user_daily_character_limit integer default 1000,
  p_global_daily_neuron_limit numeric default 4000
)
returns table(allowed boolean,event_id bigint,reason text,user_characters_used integer,global_neurons_used numeric)
language plpgsql security definer set search_path = ''
as $function$
declare
  v_day_start timestamptz := pg_catalog.date_trunc('day',now() at time zone 'UTC') at time zone 'UTC';
  v_cost numeric(14,2);
  v_user_used integer := 0;
  v_global_used numeric(14,2) := 0;
  v_event_id bigint;
begin
  if p_user_id is null or p_characters is null or p_characters < 1 or p_characters > 1000
     or p_user_daily_character_limit < 1 or p_user_daily_character_limit > 1000
     or p_global_daily_neuron_limit < 100 or p_global_daily_neuron_limit > 10000 then
    return query select false,null::bigint,'invalid_request',0,0::numeric; return;
  end if;
  -- Serialize reservations globally first, then by user, so concurrent requests cannot overspend.
  perform pg_catalog.pg_advisory_xact_lock(76192401);
  perform pg_catalog.pg_advisory_xact_lock(pg_catalog.hashtextextended(p_user_id::text,76192401));
  select coalesce(sum(characters),0)::integer into v_user_used
    from public.cenai_voice_usage_events
    where owner_user_id=p_user_id and created_at>=v_day_start;
  select coalesce(sum(neurons),0)::numeric into v_global_used
    from public.cenai_voice_usage_events
    where created_at>=v_day_start;
  v_cost := round(p_characters * 1.36364, 2);
  if v_user_used + p_characters > p_user_daily_character_limit then
    return query select false,null::bigint,'user_daily_character_limit',v_user_used,v_global_used; return;
  end if;
  if v_global_used + v_cost > p_global_daily_neuron_limit then
    return query select false,null::bigint,'global_daily_neuron_limit',v_user_used,v_global_used; return;
  end if;
  insert into public.cenai_voice_usage_events(owner_user_id,characters,neurons,status)
    values(p_user_id,p_characters,v_cost,'reserved') returning id into v_event_id;
  return query select true,v_event_id,null::text,v_user_used+p_characters,v_global_used+v_cost;
end
$function$;

create or replace function public.cenai_record_voice_usage(
  p_user_id uuid,p_event_id bigint,p_status text
)
returns boolean
language plpgsql security definer set search_path = ''
as $function$
declare v_updated bigint;
begin
  if p_user_id is null or p_event_id is null or p_status not in ('success','error') then return false; end if;
  update public.cenai_voice_usage_events
    set status=p_status,completed_at=now()
    where id=p_event_id and owner_user_id=p_user_id and status='reserved';
  get diagnostics v_updated = row_count;
  return v_updated=1;
end
$function$;

revoke all on function public.cenai_reserve_voice_usage(uuid,integer,integer,numeric) from public,anon,authenticated;
revoke all on function public.cenai_record_voice_usage(uuid,bigint,text) from public,anon,authenticated;
grant execute on function public.cenai_reserve_voice_usage(uuid,integer,integer,numeric) to service_role;
grant execute on function public.cenai_record_voice_usage(uuid,bigint,text) to service_role;

insert into public.admin_feature_flags(key,enabled,description)
values('voice.cloud.enabled',false,'Enable Cloudflare Aura-1 speech synthesis after validating quotas, audio format, and the deployed Worker.')
on conflict(key) do update set description=excluded.description;
