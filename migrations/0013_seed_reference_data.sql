insert into platform.schema_version_notes (version_key, title, notes)
values ('v1', 'Fresh-start multi-schema foundation', 'Initial fresh-start schema for Osmium.')
on conflict (version_key) do nothing;

-- Staff positions are intentionally NOT seeded here. The live staff-position
-- system is the STAFF_POSITIONS constant + identity.staff_positions (VATUSA-synced,
-- source auto/manual). The legacy org.staff_positions / org.user_staff_positions
-- tables are dropped in migration 0063 as unused.

-- Certification types are intentionally NOT seeded here. A fresh database starts
-- with no certification types; the real position-based types come from either the
-- legacy data migration (db-migrator reference domain) or manual admin configuration.

insert into stats.sync_times (id)
values ('default')
on conflict (id) do nothing;

insert into stats.statistics_prefixes (id, prefixes)
values ('default', array['ZDC'])
on conflict (id) do nothing;

insert into media.file_categories (name, order_num, key)
values
    ('General', 10, 'general'),
    ('Events', 20, 'events'),
    ('Training', 30, 'training'),
    ('Documents', 40, 'documents')
on conflict (key) do nothing;

insert into web.site_settings (key, value)
values
    ('welcome_messages', '{"homeText":"","visitorText":""}'::jsonb),
    ('public_nav', '[]'::jsonb),
    ('footer_links', '[]'::jsonb),
    ('event_defaults', '{}'::jsonb)
on conflict (key) do nothing;
