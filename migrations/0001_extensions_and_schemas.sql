create extension if not exists pgcrypto;
create extension if not exists citext;

create schema if not exists platform;
create schema if not exists identity;
create schema if not exists access;
create schema if not exists org;
create schema if not exists training;
create schema if not exists events;
create schema if not exists feedback;
create schema if not exists media;
create schema if not exists stats;
create schema if not exists integration;
create schema if not exists web;

create or replace function platform.touch_updated_at()
returns trigger
language plpgsql
as $$
begin
    new.updated_at = now();
    return new;
end;
$$;

create table if not exists platform.schema_version_notes (
    id text primary key default gen_random_uuid()::text,
    version_key text not null unique,
    title text not null,
    notes text,
    created_at timestamptz not null default now()
);

create table if not exists platform.job_runs (
    id text primary key default gen_random_uuid()::text,
    job_name text not null,
    started_at timestamptz not null,
    finished_at timestamptz,
    status text not null check (status in ('running', 'succeeded', 'failed', 'cancelled')),
    result_summary jsonb,
    error_text text,
    created_at timestamptz not null default now()
);
