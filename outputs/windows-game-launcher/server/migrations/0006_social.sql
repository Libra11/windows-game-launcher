CREATE TABLE social_profiles (
 user_id uuid PRIMARY KEY REFERENCES users, friend_code text UNIQUE NOT NULL,
 nickname text NOT NULL, bio text NOT NULL DEFAULT '', avatar text NOT NULL DEFAULT 'builtin:leaf',
 privacy jsonb NOT NULL DEFAULT '{"invisible":false,"library":true,"playtime":true,"achievements":true,"hiddenGames":[]}',
 updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE social_relations (
 owner uuid NOT NULL REFERENCES users, peer uuid NOT NULL REFERENCES users,
 state text NOT NULL CHECK(state IN ('outgoing','incoming','accepted','blocked')),
 muted boolean NOT NULL DEFAULT false, PRIMARY KEY(owner,peer), CHECK(owner<>peer)
);
CREATE TABLE social_presence (
 user_id uuid NOT NULL, device_id uuid NOT NULL, games jsonb NOT NULL DEFAULT '[]',
 updated_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY(user_id,device_id),
 FOREIGN KEY(user_id,device_id) REFERENCES devices(user_id,id)
);
CREATE TABLE social_seen_sessions (
 user_id uuid NOT NULL, device_id uuid NOT NULL, session_id uuid NOT NULL,
 seen_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY(user_id,device_id,session_id)
);
CREATE TABLE social_events (
 seq bigserial PRIMARY KEY, recipient uuid NOT NULL REFERENCES users, actor uuid NOT NULL REFERENCES users,
 game_key text NOT NULL, game_title text NOT NULL, created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX social_events_recipient ON social_events(recipient,seq);
CREATE INDEX social_events_time ON social_events(created_at);
CREATE TABLE social_rate_limits (
 user_id uuid NOT NULL REFERENCES users, kind text NOT NULL, minute bigint NOT NULL, count integer NOT NULL,
 PRIMARY KEY(user_id,kind)
);

ALTER TABLE social_profiles ADD COLUMN avatar_bytes bigint NOT NULL DEFAULT 0;

CREATE TABLE social_event_state (singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),expired_through bigint NOT NULL DEFAULT 0);
INSERT INTO social_event_state(singleton) VALUES(true);
