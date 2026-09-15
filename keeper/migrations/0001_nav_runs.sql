create table if not exists nav_runs (
  vault            text        not null,
  epoch            bigint      not null,
  status           text        not null,
  attempts         integer     not null default 1,
  total_assets     numeric(39,0),
  idle_balance     numeric(39,0),
  breakdown        jsonb,
  nav_before       numeric(39,0),
  nav_after        numeric(39,0),
  signature        text,
  last_error       text,
  first_attempt_at timestamptz not null default now(),
  last_attempt_at  timestamptz not null default now(),
  primary key (vault, epoch)
);

create index if not exists nav_runs_status_idx on nav_runs (status, last_attempt_at);
