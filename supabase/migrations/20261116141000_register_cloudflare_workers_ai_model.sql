-- Register a current Workers AI model and keep cloud inference disabled until an administrator opts in.
insert into public.admin_feature_flags(key,enabled,description)
values('ai.cloud.enabled',false,'Enable the Cloudflare Workers AI gateway after reviewing usage and billing limits.')
on conflict(key) do update set description=excluded.description;

insert into public.model_registry(
  slug,provider,model_id,display_name,description,tasks,modalities,enabled,public,config
)
values(
  'cloudflare-gemma-4-26b-a4b-it',
  'custom',
  '@cf/google/gemma-4-26b-a4b-it',
  'Gemma 4 26B A4B (Cloudflare)',
  'Cloudflare Workers AI hosted instruction model. Cloud inference is controlled by ai.cloud.enabled and per-user quota policies.',
  array['chat','reasoning','coding'],
  array['text'],
  true,
  true,
  '{"runtime":"cloudflare-workers-ai","binding":"AI","default_max_tokens":1024,"requires_explicit_enable":true}'::jsonb
)
on conflict(slug) do nothing;
