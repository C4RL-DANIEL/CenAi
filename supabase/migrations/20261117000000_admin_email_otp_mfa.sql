create table if not exists public.admin_email_mfa (
  user_id uuid primary key references auth.users(id) on delete cascade,
  email text not null,
  otp_hash text,
  expires_at timestamptz,
  attempts integer not null default 0,
  verified_until timestamptz,
  updated_at timestamptz not null default now()
);
alter table public.admin_email_mfa enable row level security;
revoke all on public.admin_email_mfa from anon, authenticated;
grant all on public.admin_email_mfa to service_role;
