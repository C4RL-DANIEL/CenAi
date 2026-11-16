-- Keep PayPal supported by all billing persistence tables.
-- Stripe and Paddle remain supported for existing installations.
alter table public.billing_customers drop constraint if exists billing_customers_provider_check;
alter table public.billing_customers add constraint billing_customers_provider_check check (provider = any (array['stripe'::text, 'paddle'::text, 'paypal'::text]));
alter table public.billing_subscriptions drop constraint if exists billing_subscriptions_provider_check;
alter table public.billing_subscriptions add constraint billing_subscriptions_provider_check check (provider = any (array['stripe'::text, 'paddle'::text, 'paypal'::text]));
alter table public.billing_webhook_events drop constraint if exists billing_webhook_events_provider_check;
alter table public.billing_webhook_events add constraint billing_webhook_events_provider_check check (provider = any (array['stripe'::text, 'paddle'::text, 'paypal'::text]));
