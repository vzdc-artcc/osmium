-- FC = Financial Committee. A vZDC staff position (roster designation) that was
-- present in the legacy StaffPosition enum and must persist in Osmium. Additive
-- and idempotent so it applies cleanly to both fresh and existing databases.
insert into org.staff_positions (name, sort_order)
values ('FC', 130)
on conflict (name) do nothing;
