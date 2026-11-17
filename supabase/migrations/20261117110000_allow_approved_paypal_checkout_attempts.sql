alter table public.billing_checkout_attempts
  drop constraint if exists billing_checkout_attempts_status_check;

alter table public.billing_checkout_attempts
  add constraint billing_checkout_attempts_status_check
  check (status in (
    'AWAITING_APPROVAL',
    'APPROVED',
    'ACTIVE',
    'CANCELLED',
    'EXPIRED',
    'FAILED'
  ));
