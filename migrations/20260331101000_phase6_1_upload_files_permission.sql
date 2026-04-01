-- Phase 6.1: explicit upload permission for owner-scoped file uploads

insert into permissions (name)
values ('upload_files')
on conflict (name) do nothing;

