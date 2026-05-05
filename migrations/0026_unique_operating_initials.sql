create unique index if not exists idx_org_memberships_operating_initials_unique
    on org.memberships (operating_initials)
    where operating_initials is not null;
