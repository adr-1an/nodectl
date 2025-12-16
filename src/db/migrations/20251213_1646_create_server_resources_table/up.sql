CREATE TABLE IF NOT EXISTS server_resources (
    server_id TEXT PRIMARY KEY REFERENCES servers(id) ON DELETE CASCADE,
    cpu INT NOT NULL, -- in millicores
    ram INT NOT NULL, -- in MiB
    swap INT NOT NULL, -- in MiB
    disk INT NOT NULL -- in MiB
);