-- Sensitive backup objects require AAL2 for accounts that enrolled a verified MFA factor.
create or replace function public.cenai_mfa_step_up_satisfied()
returns boolean
language sql
stable
security definer
set search_path = auth, public
as $$
  select
    not exists (
      select 1 from auth.mfa_factors
      where user_id = auth.uid() and status = 'verified'
    )
    or coalesce(auth.jwt() ->> 'aal', 'aal1') = 'aal2';
$$;

revoke all on function public.cenai_mfa_step_up_satisfied() from public, anon;
grant execute on function public.cenai_mfa_step_up_satisfied() to authenticated, service_role;

drop policy if exists "CenAI backup owner read" on storage.objects;
drop policy if exists "CenAI backup owner insert" on storage.objects;
drop policy if exists "CenAI backup owner update" on storage.objects;
drop policy if exists "CenAI backup owner delete" on storage.objects;

create policy "CenAI backup owner read" on storage.objects
for select to authenticated
using (bucket_id = 'cenai-backups' and (storage.foldername(name))[1] = auth.uid()::text and public.cenai_mfa_step_up_satisfied());

create policy "CenAI backup owner insert" on storage.objects
for insert to authenticated
with check (bucket_id = 'cenai-backups' and (storage.foldername(name))[1] = auth.uid()::text and public.cenai_mfa_step_up_satisfied());

create policy "CenAI backup owner update" on storage.objects
for update to authenticated
using (bucket_id = 'cenai-backups' and (storage.foldername(name))[1] = auth.uid()::text and public.cenai_mfa_step_up_satisfied())
with check (bucket_id = 'cenai-backups' and (storage.foldername(name))[1] = auth.uid()::text and public.cenai_mfa_step_up_satisfied());

create policy "CenAI backup owner delete" on storage.objects
for delete to authenticated
using (bucket_id = 'cenai-backups' and (storage.foldername(name))[1] = auth.uid()::text and public.cenai_mfa_step_up_satisfied());
