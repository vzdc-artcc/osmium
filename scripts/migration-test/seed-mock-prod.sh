#!/usr/bin/env bash

set -euo pipefail

dump_file="/seed/prod.sql"

if [[ ! -f "${dump_file}" ]]; then
  echo "missing dump file at ${dump_file}" >&2
  exit 1
fi

echo "waiting for mock-prod-postgres to accept connections"
until pg_isready -h "${PGHOST}" -p "${PGPORT}" -U "${PGUSER}" -d "${PGDATABASE}" >/dev/null 2>&1; do
  sleep 1
done

echo "resetting public schema in ${PGDATABASE}"
psql -v ON_ERROR_STOP=1 <<'SQL'
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vzdc') THEN
        CREATE ROLE vzdc;
    END IF;
END
$$;

DROP SCHEMA IF EXISTS public CASCADE;
CREATE SCHEMA public;
GRANT ALL ON SCHEMA public TO postgres;
GRANT ALL ON SCHEMA public TO public;
SQL

echo "restoring ${dump_file}"
psql -v ON_ERROR_STOP=1 -f "${dump_file}"
