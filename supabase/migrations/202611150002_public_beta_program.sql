-- CenAI public beta program
-- All access decisions are server-side; only service_role may read/write beta records.
create table if not exists public.beta_applications (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  email text not null,
  display_name text not null check (char_length(display_name) between 1 and 80),
  motivation text not null check (char_length(motivation) between 20 and 2000),
  experience text not null default 'new' check (experience in ('new','some','advanced')),
  status text not null default 'pending' check (status in ('pending','approved','rejected','revoked')),
  reviewer_id uuid references auth.users(id) on delete set null,
  reviewer_note text not null default '' check (char_length(reviewer_note) <= 1000),
  reviewed_at timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  unique(user_id)
);
create index if not exists beta_applications_status_created_idx on public.beta_applications(status,created_at desc);
create index if not exists beta_applications_email_idx on public.beta_applications(lower(email));
create table if not exists public.beta_feedback (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  category text not null check (category in ('bug','idea','ux','performance','other')),
  title text not null check (char_length(title) between 4 and 120),
  body text not null check (char_length(body) between 10 and 5000),
  app_version text not null default '',
  created_at timestamptz not null default now()
);
create index if not exists beta_feedback_created_idx on public.beta_feedback(created_at desc);
alter table public.beta_applications enable row level security;
alter table public.beta_feedback enable row level security;
revoke all on public.beta_applications from anon, authenticated;
revoke all on public.beta_feedback from anon, authenticated;
grant all on public.beta_applications to service_role;
grant all on public.beta_feedback to service_role;
create or replace function public.is_beta_tester()
returns boolean language sql stable security definer set search_path = ''
as $ select exists(select 1 from public.beta_applications b where b.user_id = auth.uid() and b.status = 'approved') $;
revoke all on function public.is_beta_tester() from public, anon;
grant execute on function public.is_beta_tester() to authenticated;
insert into public.admin_feature_flags(key,enabled,description) values
 ('beta.applications.open',true,'Allow authenticated users to apply for public beta access'),
 ('beta.feedback.enabled',true,'Allow approved beta testers to submit feedback'),
 ('beta.invite_only',false,'Require administrator invitation before beta application')
on conflict(key) do nothing;
