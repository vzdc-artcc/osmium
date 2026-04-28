create table if not exists training.performance_indicator_templates (
    id text primary key default gen_random_uuid()::text,
    name text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists training.performance_indicator_template_categories (
    id text primary key default gen_random_uuid()::text,
    template_id text not null references training.performance_indicator_templates(id) on delete cascade,
    name text not null,
    sort_order integer not null default 0
);

create table if not exists training.performance_indicator_template_criteria (
    id text primary key default gen_random_uuid()::text,
    category_id text not null references training.performance_indicator_template_categories(id) on delete cascade,
    name text not null,
    sort_order integer not null default 0
);

create table if not exists training.lesson_rubrics (
    id text primary key default gen_random_uuid()::text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists training.lessons (
    id text primary key default gen_random_uuid()::text,
    identifier text not null,
    location integer not null,
    name text not null,
    description text not null,
    position text not null,
    facility text not null,
    rubric_id text unique references training.lesson_rubrics(id) on delete set null,
    updated_at timestamptz not null default now(),
    instructor_only boolean not null default false,
    notify_instructor_on_pass boolean not null default false,
    release_request_on_pass boolean not null default false,
    duration integer not null default 60,
    trainee_preparation text,
    performance_indicator_template_id text references training.performance_indicator_templates(id) on delete set null,
    created_at timestamptz not null default now(),
    unique (identifier, location)
);

create table if not exists training.lesson_rubric_criteria (
    id text primary key default gen_random_uuid()::text,
    rubric_id text not null references training.lesson_rubrics(id) on delete cascade,
    criteria text not null,
    description text not null,
    passing integer not null,
    max_points integer not null,
    sort_order integer not null default 0,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists training.lesson_rubric_cells (
    id text primary key default gen_random_uuid()::text,
    criteria_id text not null references training.lesson_rubric_criteria(id) on delete cascade,
    points integer not null,
    description text not null,
    sort_order integer not null default 0,
    created_at timestamptz not null default now()
);

create table if not exists training.common_mistakes (
    id text primary key default gen_random_uuid()::text,
    name text not null,
    description text not null,
    facility text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists training.training_sessions (
    id text primary key default gen_random_uuid()::text,
    student_id text not null references identity.users(id) on delete cascade,
    instructor_id text not null references identity.users(id) on delete cascade,
    start timestamptz not null,
    "end" timestamptz not null,
    additional_comments text,
    trainer_comments text,
    vatusa_id text,
    enable_markdown boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists training.training_tickets (
    id text primary key default gen_random_uuid()::text,
    session_id text not null references training.training_sessions(id) on delete cascade,
    lesson_id text not null references training.lessons(id) on delete cascade,
    passed boolean not null,
    created_at timestamptz not null default now()
);

create table if not exists training.training_ticket_common_mistakes (
    training_ticket_id text not null references training.training_tickets(id) on delete cascade,
    common_mistake_id text not null references training.common_mistakes(id) on delete cascade,
    primary key (training_ticket_id, common_mistake_id)
);

create table if not exists training.rubric_scores (
    id text primary key default gen_random_uuid()::text,
    training_ticket_id text references training.training_tickets(id) on delete cascade,
    criteria_id text not null references training.lesson_rubric_criteria(id) on delete cascade,
    cell_id text not null references training.lesson_rubric_cells(id) on delete cascade,
    passed boolean not null
);

create table if not exists training.training_progressions (
    id text primary key default gen_random_uuid()::text,
    name text not null unique,
    next_progression_id text references training.training_progressions(id) on delete set null,
    auto_assign_new_home_obs boolean not null default false,
    auto_assign_new_visitor boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists training.training_progression_steps (
    id text primary key default gen_random_uuid()::text,
    progression_id text not null references training.training_progressions(id) on delete cascade,
    lesson_id text not null references training.lessons(id) on delete cascade,
    sort_order integer not null default 0,
    optional boolean not null default false,
    created_at timestamptz not null default now(),
    unique (progression_id, lesson_id),
    unique (progression_id, sort_order)
);

create table if not exists training.user_progressions (
    user_id text primary key references identity.users(id) on delete cascade,
    progression_id text not null references training.training_progressions(id) on delete cascade,
    assigned_at timestamptz not null default now(),
    assigned_by_actor_id text references access.actors(id) on delete set null
);

create table if not exists training.session_performance_indicators (
    id text primary key default gen_random_uuid()::text,
    training_session_id text not null unique references training.training_sessions(id) on delete cascade,
    created_at timestamptz not null default now()
);

create table if not exists training.session_performance_indicator_categories (
    id text primary key default gen_random_uuid()::text,
    session_performance_indicator_id text not null references training.session_performance_indicators(id) on delete cascade,
    name text not null,
    sort_order integer not null default 0
);

create table if not exists training.session_performance_indicator_criteria (
    id text primary key default gen_random_uuid()::text,
    category_id text not null references training.session_performance_indicator_categories(id) on delete cascade,
    name text not null,
    sort_order integer not null default 0,
    marker text check (marker in ('OBSERVED', 'NOT_OBSERVED', 'SATISFACTORY', 'NEEDS_IMPROVEMENT', 'UNSATISFACTORY')),
    comments text
);

create table if not exists training.training_appointment_lessons (
    appointment_id text not null references training.training_appointments(id) on delete cascade,
    lesson_id text not null references training.lessons(id) on delete cascade,
    primary key (appointment_id, lesson_id)
);

create table if not exists training.lesson_roster_changes (
    id text primary key default gen_random_uuid()::text,
    lesson_id text not null references training.lessons(id) on delete cascade,
    certification_type_id text not null references org.certification_types(id) on delete cascade,
    certification_option text not null check (certification_option in ('NONE', 'UNRESTRICTED', 'DEL', 'GND', 'TWR', 'APP', 'CTR', 'TIER_1', 'CERTIFIED', 'SOLO')),
    dossier_text text not null,
    created_at timestamptz not null default now()
);

create trigger trg_training_templates_updated_at
before update on training.performance_indicator_templates
for each row execute function platform.touch_updated_at();

create trigger trg_lesson_rubrics_updated_at
before update on training.lesson_rubrics
for each row execute function platform.touch_updated_at();

create trigger trg_lessons_updated_at
before update on training.lessons
for each row execute function platform.touch_updated_at();

create trigger trg_lesson_rubric_criteria_updated_at
before update on training.lesson_rubric_criteria
for each row execute function platform.touch_updated_at();

create trigger trg_common_mistakes_updated_at
before update on training.common_mistakes
for each row execute function platform.touch_updated_at();

create trigger trg_training_sessions_updated_at
before update on training.training_sessions
for each row execute function platform.touch_updated_at();

create trigger trg_training_progressions_updated_at
before update on training.training_progressions
for each row execute function platform.touch_updated_at();
