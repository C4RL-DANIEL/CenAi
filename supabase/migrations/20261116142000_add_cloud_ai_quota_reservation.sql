-- Atomically reserve Cloudflare AI requests against per-user quotas.
create or replace function public.cenai_reserve_ai_request(
  p_user_id uuid,p_model_id text,p_model_slug text,p_input_tokens integer default 0,
  p_max_output_tokens integer default 1024,p_default_daily_limit integer default 20,
  p_default_monthly_token_limit bigint default 20000
)
returns table(allowed boolean,event_id bigint,reason text,daily_used integer,monthly_tokens bigint)
language plpgsql security definer set search_path = ''
as $function$
declare
  v_daily_limit integer := p_default_daily_limit;
  v_monthly_limit bigint := p_default_monthly_token_limit;
  v_enabled boolean := true;
  v_daily_used integer := 0;
  v_monthly_tokens bigint := 0;
  v_rate_used integer := 0;
  v_new_id bigint;
  v_day_start timestamptz := pg_catalog.date_trunc('day',now() at time zone 'UTC') at time zone 'UTC';
  v_month_start timestamptz := pg_catalog.date_trunc('month',now() at time zone 'UTC') at time zone 'UTC';
begin
  if coalesce(auth.role(),'') <> 'service_role' then raise exception 'service role required'; end if;
  if p_user_id is null or p_model_id is null or p_model_slug is null then return query select false,null::bigint,'invalid_request',0,0::bigint; return; end if;
  if p_input_tokens < 0 or p_input_tokens > 20000 or p_max_output_tokens < 1 or p_max_output_tokens > 2048 then return query select false,null::bigint,'invalid_token_budget',0,0::bigint; return; end if;
  perform pg_catalog.pg_advisory_xact_lock(pg_catalog.hashtextextended(p_user_id::text,0));
  select daily_request_limit,monthly_token_limit,enabled into v_daily_limit,v_monthly_limit,v_enabled
  from public.ai_quota_policies where owner_user_id=p_user_id;
  if not found then v_daily_limit:=p_default_daily_limit; v_monthly_limit:=p_default_monthly_token_limit; v_enabled:=true; end if;
  if not v_enabled then return query select false,null::bigint,'quota_disabled',0,0::bigint; return; end if;
  select count(*)::integer into v_daily_used from public.usage_events
  where owner_user_id=p_user_id and source='web' and event_type='chat.completion' and created_at>=v_day_start;
  select coalesce(sum(input_tokens+output_tokens),0)::bigint into v_monthly_tokens from public.usage_events
  where owner_user_id=p_user_id and source='web' and event_type='chat.completion' and created_at>=v_month_start;
  select count(*)::integer into v_rate_used from public.usage_events
  where owner_user_id=p_user_id and source='web' and event_type='chat.completion' and created_at>=now()-interval '1 minute';
  if v_daily_used>=v_daily_limit then return query select false,null::bigint,'daily_request_limit',v_daily_used,v_monthly_tokens; return; end if;
  if v_rate_used>=20 then return query select false,null::bigint,'rate_limit',v_daily_used,v_monthly_tokens; return; end if;
  if v_monthly_tokens+p_input_tokens+p_max_output_tokens>v_monthly_limit then return query select false,null::bigint,'monthly_token_limit',v_daily_used,v_monthly_tokens; return; end if;
  insert into public.usage_events(owner_user_id,source,event_type,provider,model_id,input_tokens,output_tokens,latency_ms,status,metadata)
  values(p_user_id,'web','chat.completion','cloudflare',p_model_id,p_input_tokens,0,0,'success',
    pg_catalog.jsonb_build_object('model_slug',p_model_slug,'reservation',true,'reserved_output_tokens',p_max_output_tokens))
  returning id into v_new_id;
  return query select true,v_new_id,null::text,v_daily_used+1,v_monthly_tokens+p_input_tokens;
end
$function$;
revoke all on function public.cenai_reserve_ai_request(uuid,text,text,integer,integer,integer,bigint) from public,anon,authenticated;
grant execute on function public.cenai_reserve_ai_request(uuid,text,text,integer,integer,integer,bigint) to service_role;
