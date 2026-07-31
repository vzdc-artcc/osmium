-- Human-readable summary for each audit entry, composed at write time and shown
-- as the "Message" column on the website. Nullable so existing rows and any
-- not-yet-updated call sites are simply blank rather than failing.
alter table access.audit_logs
    add column if not exists message text;
