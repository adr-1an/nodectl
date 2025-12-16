CREATE TABLE IF NOT EXISTS allocations (
    id SERIAL PRIMARY KEY,
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    port INT NOT NULL,
    protocol TEXT NOT NULL CHECK (protocol IN ('tcp', 'udp')),
    UNIQUE (port, protocol)
);