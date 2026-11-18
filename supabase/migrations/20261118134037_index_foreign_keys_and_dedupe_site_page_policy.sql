-- Add indexes for foreign keys flagged by the Supabase performance advisor.
create index if not exists ai_quota_policies_updated_by_idx on public.ai_quota_policies(updated_by);
create index if not exists billing_checkout_attempts_plan_key_idx on public.billing_checkout_attempts(plan_key);
create index if not exists billing_one_time_payments_plan_key_idx on public.billing_one_time_payments(plan_key);
create index if not exists billing_plan_config_updated_by_idx on public.billing_plan_config(updated_by);
create index if not exists model_registry_created_by_idx on public.model_registry(created_by);
create index if not exists model_registry_updated_by_idx on public.model_registry(updated_by);
create index if not exists site_pages_updated_by_idx on public.site_pages(updated_by);
create index if not exists usage_events_api_key_id_idx on public.usage_events(api_key_id);
drop policy if exists "Public can read published site pages" on public.site_pages;
