-- Replace user-callable OTP SECURITY DEFINER RPCs with service-role-only RPCs.
-- The cenai-email-otp Edge Function authenticates the bearer token and supplies user_id.

create or replace function public.cenai_issue_email_otp_for_user(
  p_user_id uuid, p_email text, p_otp_hash text
)
returns void language plpgsql security definer set search_path = ''
as $function$
declare previous_created timestamptz;
begin
  if coalesce(auth.role(), '') <> 'service_role' then raise exception 'service role required'; end if;
  if p_user_id is null then raise exception 'user required'; end if;
  if length(p_email) > 254 or p_email !~* '^[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}$' then raise exception 'invalid email'; end if;
  if length(p_otp_hash) <> 64 or p_otp_hash !~ '^[a-f0-9]{64}$' then raise exception 'invalid challenge'; end if;
  select created_at into previous_created from public.email_otp_challenges where user_id = p_user_id for update;
  if previous_created is not null and previous_created > now() - interval '60 seconds' then raise exception 'please wait before requesting another code'; end if;
  delete from public.email_verifications where user_id = p_user_id and email <> lower(p_email);
  insert into public.email_otp_challenges(user_id,email,otp_hash,expires_at,attempts,consumed_at,created_at)
  values(p_user_id,lower(p_email),p_otp_hash,now()+interval '10 minutes',0,null,now())
  on conflict(user_id) do update set email=excluded.email,otp_hash=excluded.otp_hash,expires_at=excluded.expires_at,attempts=0,consumed_at=null,created_at=now();
end
$function$;

create or replace function public.cenai_verify_email_otp_for_user(
  p_user_id uuid, p_email text, p_otp_hash text
)
returns boolean language plpgsql security definer set search_path = ''
as $function$
declare matched boolean := false;
begin
  if coalesce(auth.role(), '') <> 'service_role' then raise exception 'service role required'; end if;
  if p_user_id is null or length(p_otp_hash) <> 64 or p_otp_hash !~ '^[a-f0-9]{64}$' then return false; end if;
  update public.email_otp_challenges
  set attempts=attempts+1,consumed_at=case when otp_hash=p_otp_hash then now() else consumed_at end
  where user_id=p_user_id and email=lower(p_email) and expires_at>now() and consumed_at is null and attempts<5
  returning (otp_hash=p_otp_hash) into matched;
  if matched then
    insert into public.email_verifications(user_id,email,verified_at) values(p_user_id,lower(p_email),now())
    on conflict(user_id) do update set email=excluded.email,verified_at=now();
  end if;
  return coalesce(matched,false);
end
$function$;

revoke all on function public.cenai_issue_email_otp(text,text) from public, anon, authenticated;
revoke all on function public.cenai_verify_email_otp(text,text) from public, anon, authenticated;
grant execute on function public.cenai_issue_email_otp(text,text) to service_role;
grant execute on function public.cenai_verify_email_otp(text,text) to service_role;
revoke all on function public.cenai_issue_email_otp_for_user(uuid,text,text) from public, anon, authenticated;
revoke all on function public.cenai_verify_email_otp_for_user(uuid,text,text) from public, anon, authenticated;
grant execute on function public.cenai_issue_email_otp_for_user(uuid,text,text) to service_role;
grant execute on function public.cenai_verify_email_otp_for_user(uuid,text,text) to service_role;
