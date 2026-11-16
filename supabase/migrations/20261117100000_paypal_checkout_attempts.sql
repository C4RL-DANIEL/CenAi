create table if not exists public.billing_checkout_attempts (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  provider_subscription_id text not null unique,
  plan_key text not null references public.billing_plan_config(plan_key),
  status text not null default 'AWAITING_APPROVAL'
    check (status in ('AWAITING_APPROVAL','ACTIVE','CANCELLED','EXPIRED','FAILED')),
  expires_at timestamptz not null default (now() + interval '20 minutes'),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create index if not exists billing_checkout_attempts_user_status_idx
  on public.billing_checkout_attempts(user_id, status, expires_at);

alter table public.billing_checkout_attempts enable row level security;
revoke all on public.billing_checkout_attempts from anon, authenticated;
grant all on public.billing_checkout_attempts to service_role;
