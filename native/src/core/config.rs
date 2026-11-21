//! Public client configuration matching the existing CenAI deployment.
pub const SUPABASE_URL: &str = "https://bqkzehozezvepbthglyj.supabase.co";
pub const SUPABASE_PUBLISHABLE_KEY: &str = "sb_publishable_l6YDudJWsFgUJ2YZc2Xq1Q_hNatLp3C";
pub const CONTROL_PLANE_URL: &str = "https://bqkzehozezvepbthglyj.supabase.co/functions/v1/cenai-control-plane";
pub const PERSONAL_MEMORY_URL: &str = "https://bqkzehozezvepbthglyj.supabase.co/functions/v1/cenai-memory";
pub const BILLING_URL: &str = "https://bqkzehozezvepbthglyj.supabase.co/functions/v1/cenai-billing";
pub const PAYPAL_BILLING_URL: &str = "https://bqkzehozezvepbthglyj.supabase.co/functions/v1/cenai-paypal-billing";
pub const TELEMETRY_URL: &str = "https://bqkzehozezvepbthglyj.supabase.co/functions/v1/cenai-telemetry";
pub const CLOUD_CHAT_URL: &str = "https://cenai-website.aglibotcarldaniel.workers.dev/api/ai/chat/completions";
pub const CLOUD_VOICE_URL: &str = "https://cenai-website.aglibotcarldaniel.workers.dev/api/voice/speech";
pub const CLOUD_VOICE_STATUS_URL: &str = "https://cenai-website.aglibotcarldaniel.workers.dev/api/voice/status";
pub const WEBSITE_OTP_URL: &str = "https://cenai-website.cenai-app.workers.dev/api/email-otp";
pub const REQUEST_TIMEOUT_SECONDS: u64 = 40;
pub const STREAM_TIMEOUT_SECONDS: u64 = 300;
pub const ECOSYSTEM_URL: &str = "https://bqkzehozezvepbthglyj.supabase.co/functions/v1/cenai-ecosystem";
pub const BACKUP_BUCKET: &str = "cenai-backups";
