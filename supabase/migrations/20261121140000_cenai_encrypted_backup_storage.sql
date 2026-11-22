-- Private encrypted workspace backups; Storage RLS scopes each object to its owner UUID.
insert into storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
values ('cenai-backups', 'cenai-backups', false, 104857600, array['application/json'])
on conflict (id) do update set public = false, file_size_limit = 104857600, allowed_mime_types = array['application/json'];

drop policy if exists "CenAI backup owner read" on storage.objects;
drop policy if exists "CenAI backup owner insert" on storage.objects;
drop policy if exists "CenAI backup owner update" on storage.objects;
drop policy if exists "CenAI backup owner delete" on storage.objects;

create policy "CenAI backup owner read" on storage.objects
for select to authenticated
using (bucket_id = 'cenai-backups' and (storage.foldername(name))[1] = auth.uid()::text);

create policy "CenAI backup owner insert" on storage.objects
for insert to authenticated
with check (bucket_id = 'cenai-backups' and (storage.foldername(name))[1] = auth.uid()::text);

create policy "CenAI backup owner update" on storage.objects
for update to authenticated
using (bucket_id = 'cenai-backups' and (storage.foldername(name))[1] = auth.uid()::text)
with check (bucket_id = 'cenai-backups' and (storage.foldername(name))[1] = auth.uid()::text);

create policy "CenAI backup owner delete" on storage.objects
for delete to authenticated
using (bucket_id = 'cenai-backups' and (storage.foldername(name))[1] = auth.uid()::text);
