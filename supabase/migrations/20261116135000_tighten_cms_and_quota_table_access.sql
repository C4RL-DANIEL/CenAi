-- Tighten public access for the CenAI quota and website CMS tables.
-- The control plane uses the service_role key for all administrative writes.
-- Public visitors may only read published CMS pages.

revoke all privileges on table public.ai_quota_policies from anon, authenticated;

revoke insert, update, delete, truncate, references, trigger
on table public.site_pages from anon, authenticated;
grant select on table public.site_pages to anon, authenticated;

drop policy if exists site_pages_public_read on public.site_pages;
create policy site_pages_public_read
on public.site_pages
for select
to anon, authenticated
using (published = true);
