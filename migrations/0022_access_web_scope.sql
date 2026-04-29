alter table access.service_account_roles
    drop constraint if exists service_account_roles_scope_type_check;

alter table access.service_account_roles
    add constraint service_account_roles_scope_type_check
    check (
        scope_type in (
            'global',
            'event',
            'file',
            'training_progression',
            'training_session',
            'web',
            'web_page',
            'service_account'
        )
    );

alter table access.audit_logs
    drop constraint if exists audit_logs_scope_type_check;

alter table access.audit_logs
    add constraint audit_logs_scope_type_check
    check (
        scope_type in (
            'global',
            'event',
            'file',
            'training_progression',
            'training_session',
            'web',
            'web_page',
            'service_account'
        )
    );
