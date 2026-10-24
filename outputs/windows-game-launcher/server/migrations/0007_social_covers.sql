CREATE TABLE social_covers (
 user_id uuid NOT NULL REFERENCES users,
 game_key text NOT NULL,
 source_hash text NOT NULL,
 bytes bigint NOT NULL CHECK(bytes > 0 AND bytes <= 1000000),
 PRIMARY KEY(user_id,game_key)
);
