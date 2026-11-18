insert into public.admin_feature_flags (key, enabled, description)
values (
  'billing.recurring_enabled',
  false,
  'Temporarily gate recurring PayPal subscriptions until activation, renewal, and cancellation lifecycle tests pass. One-time card payments remain controlled by billing.enabled.'
)
on conflict (key) do update
set enabled = false,
    description = excluded.description,
    updated_at = now();
