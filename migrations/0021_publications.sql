create table if not exists web.publication_categories (
    id text primary key default gen_random_uuid()::text,
    key text not null unique,
    name text not null,
    description text,
    sort_order integer not null default 0,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists web.publications (
    id text primary key default gen_random_uuid()::text,
    category_id text not null references web.publication_categories(id),
    title text not null,
    description text,
    effective_at timestamptz not null,
    updated_at timestamptz not null default now(),
    file_id text not null unique references media.file_assets(id),
    is_public boolean not null default false,
    sort_order integer not null default 0,
    status text not null default 'draft' check (status in ('draft', 'published', 'archived'))
);

create index if not exists idx_web_publications_category_sort_effective
    on web.publications(category_id, sort_order, effective_at desc);

create index if not exists idx_web_publications_visibility
    on web.publications(is_public, status, effective_at desc);

create trigger trg_web_publication_categories_updated_at
before update on web.publication_categories
for each row execute function platform.touch_updated_at();

create trigger trg_web_publications_updated_at
before update on web.publications
for each row execute function platform.touch_updated_at();

insert into web.publication_categories (key, name, description, sort_order)
values
    ('general-policy', 'General Policy & Facility Administration', null, 1),
    ('tower-sops', 'ATC Tower Standard Operating Procedures & Reference', null, 2),
    ('atct-tracon-sops', 'ATCT/TRACON Standard Operating Procedures & Reference', null, 3),
    ('tracon-sops', 'TRACON Standard Operating Procedures & Reference', null, 4),
    ('enroute-sops', 'Enroute Standard Operating Procedures & Reference', null, 5),
    ('letters-of-agreement', 'Letters of Agreement', null, 6),
    ('charts', 'Charts', null, 7),
    ('controller-bulletins', 'Controller Bulletins', null, 8),
    ('job-aids', 'Quick Reference Job Aids', null, 9),
    ('sfra', 'Washington Special Flight Rules Area (SFRA)', null, 10),
    ('vatis', 'vATIS', null, 11),
    ('vvscs', 'vVSCS', null, 12)
on conflict (key) do update
set
    name = excluded.name,
    description = excluded.description,
    sort_order = excluded.sort_order;
