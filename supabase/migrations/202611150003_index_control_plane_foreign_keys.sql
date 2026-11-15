-- Index foreign keys used by the admin and beta workflows.
create index if not exists admin_feature_flags_updated_by_idx on public.admin_feature_flags(updated_by);
create index if not exists beta_applications_reviewer_id_idx on public.beta_applications(reviewer_id);
create index if not exists beta_feedback_user_id_idx on public.beta_feedback(user_id);
