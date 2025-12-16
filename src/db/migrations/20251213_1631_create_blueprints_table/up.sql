CREATE TABLE IF NOT EXISTS blueprints (
    id SERIAL PRIMARY KEY,
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    image_url TEXT NOT NULL,
    install_script JSONB,
    start_command JSONB,
    stop_command TEXT, -- Optional command that will be sent to stdin to stop the server, if specified. If null, send SIGKILL
    use_shell BOOL NOT NULL DEFAULT true
)