alter table feedback.dossier_entries
    add column if not exists is_confidential boolean not null default false;

insert into access.permissions (name, description)
values
    ('training.dossier.create', 'Create dossier entries for a user'),
    ('training.dossier_confidential.read', 'View confidential dossier entries')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'training.dossier.create'),
    ('ATM', 'training.dossier_confidential.read'),
    ('DATM', 'training.dossier_confidential.read'),
    ('TA', 'training.dossier_confidential.read')
on conflict (role_name, permission_name) do nothing;
