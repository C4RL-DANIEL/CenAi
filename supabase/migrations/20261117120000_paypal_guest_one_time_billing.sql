create table if not exists public.billing_one_time_payments (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  provider_order_id text not null unique,
  plan_key text not null references public.billing_plan_config(plan_key),
  amount numeric(12,2) not null check (amount > 0),
  currency text not null check (currency ~ '^[A-Z]{3}$'),
  status text not null default 'CREATED'
    check (status in ('CREATED','COMPLETED','FAILED','REFUNDED','REVERSED','EXPIRED')),
  paid_at timestamptz,
  access_expires_at timestamptz,
  provider_capture_id text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);
create index if not exists billing_one_time_payments_user_status_idx
  on public.billing_one_time_payments(user_id,status,access_expires_at);
create index if not exists billing_one_time_payments_expiry_idx
  on public.billing_one_time_payments(status,access_expires_at);
alter table public.billing_one_time_payments enable row level security;
revoke all on public.billing_one_time_payments from anon,authenticated;
grant all on public.billing_one_time_payments to service_role;
