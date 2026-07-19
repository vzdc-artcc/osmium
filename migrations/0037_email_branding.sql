create table if not exists email.branding (
    id text primary key,
    brand_name text not null,
    tagline text not null,
    footer_text text not null,
    logo_file_id text references media.file_assets(id) on delete set null,
    header_background_color text not null,
    header_text_color text not null,
    page_background_color text not null,
    panel_background_color text not null,
    text_color text not null,
    heading_color text not null,
    link_color text not null,
    accent_color text not null,
    button_background_color text not null,
    button_text_color text not null,
    heading_font_family text not null,
    body_font_family text not null,
    font_size_scale text not null default 'medium' check (font_size_scale in ('small', 'medium', 'large')),
    corner_style text not null default 'soft' check (corner_style in ('sharp', 'rounded', 'soft')),
    updated_by_user_id text references identity.users(id) on delete set null,
    updated_at timestamptz not null default now()
);

create trigger trg_email_branding_updated_at
before update on email.branding
for each row execute function platform.touch_updated_at();

insert into email.branding (
    id,
    brand_name,
    tagline,
    footer_text,
    logo_file_id,
    header_background_color,
    header_text_color,
    page_background_color,
    panel_background_color,
    text_color,
    heading_color,
    link_color,
    accent_color,
    button_background_color,
    button_text_color,
    heading_font_family,
    body_font_family,
    font_size_scale,
    corner_style
)
values (
    'default',
    'vZDC',
    'Washington ARTCC',
    'Sent by vZDC.',
    null,
    '#500e0e',
    '#ededf5',
    '#f1f0f6',
    '#ffffff',
    '#1f2430',
    '#500e0e',
    '#500e0e',
    '#500e0e',
    '#500e0e',
    '#ededf5',
    'roboto_sans',
    'roboto_sans',
    'medium',
    'soft'
)
on conflict (id) do nothing;

insert into access.permissions (name, description)
values
    ('emails.branding.read', 'Read email branding configuration'),
    ('emails.branding.update', 'Update email branding configuration')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'emails.branding.read'),
    ('STAFF', 'emails.branding.update')
on conflict (role_name, permission_name) do nothing;
