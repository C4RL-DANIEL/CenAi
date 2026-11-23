-- Cover remaining foreign keys so parent-row updates/deletes do not require
-- sequential scans of the referencing tables.
create index if not exists ecosystem_audit_logs_actor_user_id_idx
  on public.ecosystem_audit_logs (actor_user_id);
create index if not exists ecosystem_trusted_publishers_owner_user_id_idx
  on public.ecosystem_trusted_publishers (owner_user_id);
create index if not exists ecosystem_user_grants_app_id_idx
  on public.ecosystem_user_grants (app_id);
create index if not exists registration_invites_created_by_idx
  on public.registration_invites (created_by);
create index if not exists registration_invites_redeemed_by_idx
  on public.registration_invites (redeemed_by);
