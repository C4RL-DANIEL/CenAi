-- Count every OTP verification attempt, including incorrect codes.
-- Lock the challenge after five attempts and consume it on a successful match.

create or replace function public.cenai_verify_email_otp(p_email text, p_otp_hash text)
returns boolean
language plpgsql
security definer
set search_path = ''
as $function$
declare
  matched boolean := false;
begin
  if auth.uid() is null then raise exception 'authentication required'; end if;
  if length(p_otp_hash) <> 64 then return false; end if;

  update public.email_otp_challenges
  set attempts = attempts + 1,
      consumed_at = case when otp_hash = p_otp_hash then now() else consumed_at end
  where user_id = auth.uid()
    and email = lower(p_email)
    and expires_at > now()
    and consumed_at is null
    and attempts < 5
  returning (otp_hash = p_otp_hash) into matched;

  if matched then
    insert into public.email_verifications(user_id,email,verified_at)
    values(auth.uid(),lower(p_email),now())
    on conflict(user_id) do update set email=excluded.email, verified_at=now();
  end if;

  return coalesce(matched,false);
end
$function$;

revoke all on function public.cenai_verify_email_otp(text,text) from public, anon;
grant execute on function public.cenai_verify_email_otp(text,text) to authenticated;
