CREATE TABLE IF NOT EXISTS servers (
    id TEXT PRIMARY KEY, -- ID assigned by panel backend, don't auto increment
    container_id TEXT UNIQUE,
    state TEXT NOT NULL DEFAULT 'stopped',
    install_status TEXT NOT NULL,
    restart_policy TEXT NOT NULL DEFAULT 'no'
        CHECK (restart_policy IN ('no', 'on-failure', 'always', 'unless-stopped')),
    network_mode TEXT NOT NULL CHECK (network_mode IN ('none', 'private', 'public'))
    -- allocations in allocations table with FK

    -- resources in resources table with FK
);