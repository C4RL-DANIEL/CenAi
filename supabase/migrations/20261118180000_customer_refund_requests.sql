-- Customer-initiated refund requests for verified one-time PayPal card payments.
create table if not exists public.billing_refund_requests (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  customer_email text not null default '',
  provider_order_id text not null unique references public.billing_one_time_payments(provider_order_id) on delete cascade,
  amount numeric(12,2) not null check (amount > 0),
  currency text not null check (currency ~ '^[A-Z]{3}$'),
  reason text not null check (char_length(reason) between 3 and 80),
  details text not null default '' check (char_length(details) <= 2000),
  status text not null default 'PENDING' check (status in ('PENDING','APPROVED','REJECTED','PROCESSING','COMPLETED','FAILED')),
  decision_note text not null default '' check (char_length(decision_note) <= 1000),
  reviewed_by uuid references auth.users(id) on delete set null,
  reviewed_at timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);
create index if not exists billing_refund_requests_user_created_idx on public.billing_refund_requests(user_id, created_at desc);
create index if not exists billing_refund_requests_status_created_idx on public.billing_refund_requests(status, created_at);
alter table public.billing_refund_requests enable row level security;
revoke all on public.billing_refund_requests from anon, authenticated;
grant all on public.billing_refund_requests to service_role;
