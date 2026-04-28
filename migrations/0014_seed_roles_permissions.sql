insert into access.roles (name, description)
values
    ('USER', 'Default member role'),
    ('STAFF', 'General staff role'),
    ('ATM', 'Air Traffic Manager'),
    ('DATM', 'Deputy Air Traffic Manager'),
    ('TA', 'Training Administrator'),
    ('EC', 'Events Coordinator'),
    ('FE', 'Facility Engineer'),
    ('WM', 'Webmaster'),
    ('ATA', 'Assistant Training Administrator'),
    ('AWM', 'Assistant Webmaster'),
    ('AEC', 'Assistant Events Coordinator'),
    ('AFE', 'Assistant Facility Engineer'),
    ('INS', 'Instructor'),
    ('MTR', 'Mentor'),
    ('EVENT_STAFF', 'Event staff'),
    ('WEB_TEAM', 'Web team'),
    ('BOT', 'Bot role'),
    ('SERVICE_APP', 'Service application role')
on conflict (name) do nothing;

insert into access.permissions (name, description)
values
    ('auth.read', 'Read own profile'),
    ('auth.delete', 'Logout current session'),
    ('auth.manage', 'Development login as CID'),
    ('system.read', 'Read readiness endpoints'),
    ('users.read', 'View all users'),
    ('users.update', 'Manage users'),
    ('training.update', 'Manage training'),
    ('feedback.update', 'Manage feedback'),
    ('files.create', 'Upload files'),
    ('files.read', 'Read files'),
    ('files.update', 'Manage files'),
    ('files.delete', 'Delete files'),
    ('events.read', 'Read events'),
    ('events.create', 'Create events'),
    ('events.update', 'Manage events'),
    ('events.delete', 'Delete events'),
    ('stats.manage', 'Manage stats sync'),
    ('integrations.manage', 'Manage integrations'),
    ('web.update', 'Manage website content')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('USER', 'auth.read'),
    ('USER', 'auth.delete'),
    ('USER', 'files.create'),
    ('STAFF', 'auth.read'),
    ('STAFF', 'auth.delete'),
    ('STAFF', 'system.read'),
    ('STAFF', 'users.read'),
    ('STAFF', 'users.update'),
    ('STAFF', 'training.update'),
    ('STAFF', 'feedback.update'),
    ('STAFF', 'files.create'),
    ('STAFF', 'files.update'),
    ('STAFF', 'auth.manage'),
    ('STAFF', 'events.update'),
    ('STAFF', 'stats.manage'),
    ('STAFF', 'integrations.manage'),
    ('STAFF', 'web.update'),
    ('BOT', 'integrations.manage'),
    ('SERVICE_APP', 'integrations.manage')
on conflict (role_name, permission_name) do nothing;

insert into access.service_accounts (key, name, description, owner_team)
values
    ('discord_bot', 'Discord Bot', 'First-party Discord bot', 'web'),
    ('teamspeak_bot', 'TeamSpeak Bot', 'First-party TeamSpeak bot', 'ops'),
    ('website_frontend', 'Website Frontend', 'Public website integration', 'web'),
    ('asx_sync', 'ASX Sync', 'ASX integration', 'ops'),
    ('rvm_sync', 'RVM Sync', 'RVM integration', 'ops')
on conflict (key) do nothing;
