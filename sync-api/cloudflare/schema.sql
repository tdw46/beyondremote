CREATE TABLE IF NOT EXISTS br_users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            provider TEXT NOT NULL DEFAULT 'password',
            provider_user_id TEXT NOT NULL,
            name TEXT NOT NULL UNIQUE,
            display_name TEXT NOT NULL DEFAULT '',
            email TEXT NOT NULL DEFAULT '',
            avatar TEXT NOT NULL DEFAULT '',
            password_hash TEXT,
            status INTEGER NOT NULL DEFAULT 1,
            is_admin INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(provider, provider_user_id)
        );
        CREATE TABLE IF NOT EXISTS br_sessions (
            token_hash TEXT PRIMARY KEY,
            user_id INTEGER NOT NULL REFERENCES br_users(id) ON DELETE CASCADE,
            expires_at TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS br_auth_requests (
            code TEXT PRIMARY KEY,
            op TEXT NOT NULL,
            remote_id TEXT NOT NULL DEFAULT '',
            uuid TEXT NOT NULL DEFAULT '',
            device_info TEXT NOT NULL DEFAULT '{}',
            auth_body TEXT,
            expires_at TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS br_legacy_address_books (
            user_id INTEGER PRIMARY KEY REFERENCES br_users(id) ON DELETE CASCADE,
            data TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS br_personal_peers (
            user_id INTEGER NOT NULL REFERENCES br_users(id) ON DELETE CASCADE,
            peer_id TEXT NOT NULL,
            data TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(user_id, peer_id)
        );
        CREATE TABLE IF NOT EXISTS br_personal_tags (
            user_id INTEGER NOT NULL REFERENCES br_users(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            color INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(user_id, name)
        );
        CREATE TABLE IF NOT EXISTS br_devices (
            user_id INTEGER NOT NULL REFERENCES br_users(id) ON DELETE CASCADE,
            remote_id TEXT NOT NULL,
            uuid TEXT NOT NULL DEFAULT '',
            device_info TEXT NOT NULL DEFAULT '{}',
            last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(user_id, remote_id)
        );
CREATE TABLE br_presence (
 user_id INTEGER NOT NULL REFERENCES br_users(id) ON DELETE CASCADE,
 remote_id TEXT NOT NULL, generation TEXT NOT NULL, token_hash TEXT NOT NULL,
 sequence INTEGER NOT NULL DEFAULT 0, state TEXT NOT NULL CHECK(state IN ('online-reported','offline-reported','unknown')),
 updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, payload TEXT NOT NULL DEFAULT '{}',
 PRIMARY KEY(user_id,remote_id), FOREIGN KEY(user_id,remote_id) REFERENCES br_devices(user_id,remote_id) ON DELETE CASCADE
);
CREATE INDEX br_devices_recent ON br_devices(user_id,last_seen_at DESC);
CREATE INDEX br_sessions_owner ON br_sessions(user_id);
CREATE INDEX br_sessions_expiry ON br_sessions(expires_at);
CREATE TABLE br_auth_limits(key TEXT PRIMARY KEY, window INTEGER NOT NULL, attempts INTEGER NOT NULL);
ALTER TABLE br_auth_requests ADD COLUMN state_hash TEXT;
ALTER TABLE br_auth_requests ADD COLUMN claimed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE br_auth_requests ADD COLUMN delivered INTEGER NOT NULL DEFAULT 0;

ALTER TABLE br_sessions ADD COLUMN native INTEGER NOT NULL DEFAULT 0;

ALTER TABLE br_auth_requests ADD COLUMN native INTEGER NOT NULL DEFAULT 0;
CREATE INDEX br_auth_requests_expiry ON br_auth_requests(native,expires_at);
