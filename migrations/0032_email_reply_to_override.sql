alter table email.outbox
    add column if not exists reply_to_address text null;
