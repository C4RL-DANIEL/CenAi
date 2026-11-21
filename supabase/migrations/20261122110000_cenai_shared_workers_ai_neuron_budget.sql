-- Shared atomic Neuron reservations across every CenAI Workers AI feature.
-- The app reserves at most 8,000 Neurons per UTC day and leaves 2,000 for safety/other account activity.
create table if not exists public.cenai_ai_daily_budget (
  day_utc date primary key,
  reserved_neurons numeric(14,2) not null default 0 check (reserved_neurons >= 0),
  updated_at timestamptz not null default now()
);
alter table public.cenai_ai_daily_budget enable row level security;
revoke all on public.cenai_ai_daily_budget from public, anon, authenticated;
grant select, insert, update, delete on public.cenai_ai_daily_budget to service_role;

-- Replace the voice reservation so it also participates in the shared budget.
create or replace function public.cenai_reserve_voice_usage(
  p_user_id uuid, p_characters integer,
  p_user_daily_character_limit integer default 1000,
  p_global_daily_neuron_limit numeric default 4000
) returns table(allowed boolean,event_id bigint,reason text,user_characters_used integer,global_neurons_used numeric)
language plpgsql security definer set search_path = ''
as $function$
declare
  v_day_start timestamptz := pg_catalog.date_trunc('day',now() at time zone 'UTC') at time zone 'UTC';
  v_day date := (now() at time zone 'UTC')::date;
  v_cost numeric(14,2); v_user_used integer := 0; v_voice_used numeric(14,2) := 0;
  v_global_used numeric(14,2) := 0; v_event_id bigint;
begin
  if coalesce(auth.role(),'') <> 'service_role' then raise exception 'service role required'; end if;
  if p_user_id is null or p_characters is null or p_characters < 1 or p_characters > 1000
     or p_user_daily_character_limit < 1 or p_user_daily_character_limit > 1000
     or p_global_daily_neuron_limit < 100 or p_global_daily_neuron_limit > 10000 then
    return query select false,null::bigint,'invalid_request',0,0::numeric; return;
  end if;
  perform pg_catalog.pg_advisory_xact_lock(76192401);
  perform pg_catalog.pg_advisory_xact_lock(pg_catalog.hashtextextended(p_user_id::text,76192401));
  select coalesce(sum(characters),0)::integer into v_user_used from public.cenai_voice_usage_events
    where owner_user_id=p_user_id and created_at>=v_day_start;
  select coalesce(sum(neurons),0)::numeric into v_voice_used from public.cenai_voice_usage_events where created_at>=v_day_start;
  select coalesce(max(reserved_neurons),0)::numeric into v_global_used from public.cenai_ai_daily_budget where day_utc=v_day;
  v_cost := round(p_characters * 1.36364, 2);
  if v_user_used + p_characters > p_user_daily_character_limit then
    return query select false,null::bigint,'user_daily_character_limit',v_user_used,v_global_used; return;
  end if;
  if v_voice_used + v_cost > p_global_daily_neuron_limit then
    return query select false,null::bigint,'global_daily_neuron_limit',v_user_used,v_global_used; return;
  end if;
  if v_global_used + v_cost > 8000 then
    return query select false,null::bigint,'shared_global_daily_neuron_limit',v_user_used,v_global_used; return;
  end if;
  insert into public.cenai_voice_usage_events(owner_user_id,characters,neurons,status)
    values(p_user_id,p_characters,v_cost,'reserved') returning id into v_event_id;
  insert into public.cenai_ai_daily_budget as budget(day_utc,reserved_neurons,updated_at)
    values(v_day,v_cost,now())
    on conflict(day_utc) do update set reserved_neurons=budget.reserved_neurons+excluded.reserved_neurons,updated_at=now();
  return query select true,v_event_id,null::text,v_user_used+p_characters,v_global_used+v_cost;
end
$function$;

-- Chat reservations also reserve estimated Neurons before Workers AI inference.
create or replace function public.cenai_reserve_ai_request_v2(
  p_user_id uuid,p_model_id text,p_model_slug text,p_input_tokens integer default 0,
  p_max_output_tokens integer default 1024,p_estimated_neurons numeric default 1,
  p_default_daily_limit integer default 20,p_default_monthly_token_limit bigint default 20000,
  p_global_daily_neuron_limit numeric default 8000
) returns table(allowed boolean,event_id bigint,reason text,daily_used integer,monthly_tokens bigint,global_neurons_used numeric)
language plpgsql security definer set search_path = ''
as $function$
declare
  v_daily_limit integer := p_default_daily_limit; v_monthly_limit bigint := p_default_monthly_token_limit;
  v_enabled boolean := true; v_daily_used integer := 0; v_monthly_tokens bigint := 0; v_rate_used integer := 0;
  v_new_id bigint; v_day_start timestamptz := pg_catalog.date_trunc('day',now() at time zone 'UTC') at time zone 'UTC';
  v_month_start timestamptz := pg_catalog.date_trunc('month',now() at time zone 'UTC') at time zone 'UTC';
  v_day date := (now() at time zone 'UTC')::date; v_global_used numeric(14,2) := 0;
begin
  if coalesce(auth.role(),'') <> 'service_role' then raise exception 'service role required'; end if;
  if p_user_id is null or p_model_id is null or p_model_slug is null then return query select false,null::bigint,'invalid_request',0,0::bigint,0::numeric; return; end if;
  if p_input_tokens < 0 or p_input_tokens > 20000 or p_max_output_tokens < 1 or p_max_output_tokens > 2048
     or p_estimated_neurons is null or p_estimated_neurons <= 0 or p_estimated_neurons > 4000 or p_global_daily_neuron_limit < 100 or p_global_daily_neuron_limit > 10000 then
    return query select false,null::bigint,'invalid_token_budget',0,0::bigint,0::numeric; return;
  end if;
  perform pg_catalog.pg_advisory_xact_lock(76192401);
  perform pg_catalog.pg_advisory_xact_lock(pg_catalog.hashtextextended(p_user_id::text,76192401));
  select daily_request_limit,monthly_token_limit,enabled into v_daily_limit,v_monthly_limit,v_enabled
    from public.ai_quota_policies where owner_user_id=p_user_id;
  if not found then v_daily_limit:=p_default_daily_limit; v_monthly_limit:=p_default_monthly_token_limit; v_enabled:=true; end if;
  if not v_enabled then return query select false,null::bigint,'quota_disabled',0,0::bigint,0::numeric; return; end if;
  select count(*)::integer into v_daily_used from public.usage_events
    where owner_user_id=p_user_id and source='web' and event_type='chat.completion' and created_at>=v_day_start;
  select coalesce(sum(input_tokens+output_tokens),0)::bigint into v_monthly_tokens from public.usage_events
    where owner_user_id=p_user_id and source='web' and event_type='chat.completion' and created_at>=v_month_start;
  select count(*)::integer into v_rate_used from public.usage_events
    where owner_user_id=p_user_id and source='web' and event_type='chat.completion' and created_at>=now()-interval '1 minute';
  select coalesce(max(reserved_neurons),0)::numeric into v_global_used from public.cenai_ai_daily_budget where day_utc=v_day;
  if v_daily_used>=v_daily_limit then return query select false,null::bigint,'daily_request_limit',v_daily_used,v_monthly_tokens,v_global_used; return; end if;
  if v_rate_used>=20 then return query select false,null::bigint,'rate_limit',v_daily_used,v_monthly_tokens,v_global_used; return; end if;
  if v_monthly_tokens+p_input_tokens+p_max_output_tokens>v_monthly_limit then return query select false,null::bigint,'monthly_token_limit',v_daily_used,v_monthly_tokens,v_global_used; return; end if;
  if v_global_used+p_estimated_neurons>p_global_daily_neuron_limit then return query select false,null::bigint,'shared_global_daily_neuron_limit',v_daily_used,v_monthly_tokens,v_global_used; return; end if;
  insert into public.usage_events(owner_user_id,source,event_type,provider,model_id,input_tokens,output_tokens,latency_ms,status,metadata)
    values(p_user_id,'web','chat.completion','cloudflare',p_model_id,p_input_tokens,0,0,'success',
      pg_catalog.jsonb_build_object('model_slug',p_model_slug,'reservation',true,'reserved_output_tokens',p_max_output_tokens,'reserved_neurons',p_estimated_neurons))
    returning id into v_new_id;
  insert into public.cenai_ai_daily_budget as budget(day_utc,reserved_neurons,updated_at)
    values(v_day,p_estimated_neurons,now())
    on conflict(day_utc) do update set reserved_neurons=budget.reserved_neurons+excluded.reserved_neurons,updated_at=now();
  return query select true,v_new_id,null::text,v_daily_used+1,v_monthly_tokens+p_input_tokens,v_global_used+p_estimated_neurons;
end
$function$;

revoke all on function public.cenai_reserve_ai_request_v2(uuid,text,text,integer,integer,numeric,integer,bigint,numeric) from public,anon,authenticated;
grant execute on function public.cenai_reserve_ai_request_v2(uuid,text,text,integer,integer,numeric,integer,bigint,numeric) to service_role;