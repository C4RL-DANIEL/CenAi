create or replace function public.cenai_user_has_verified_mfa(p_user_id uuid)
returns boolean
language sql
stable
security definer
set search_path = auth, public
as $$
  select exists (
    select 1 from auth.mfa_factors
    where user_id = p_user_id and status = 'verified'
  );
$$;

revoke all on function public.cenai_user_has_verified_mfa(uuid) from public, anon, authenticated;
grant execute on function public.cenai_user_has_verified_mfa(uuid) to service_role;
