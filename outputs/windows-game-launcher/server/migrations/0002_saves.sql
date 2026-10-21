CREATE TABLE save_profiles (
 user_id uuid NOT NULL REFERENCES users, id uuid NOT NULL, game_id uuid NOT NULL,
 name text NOT NULL CHECK(length(name) BETWEEN 1 AND 80), revision bigint NOT NULL DEFAULT 0,
 selected_snapshot_id uuid, PRIMARY KEY(user_id,id)
);
CREATE TABLE save_uploads (
 user_id uuid NOT NULL, id uuid NOT NULL, profile_id uuid NOT NULL, device_id uuid NOT NULL,
 snapshot_id uuid NOT NULL, request jsonb NOT NULL, bytes bigint NOT NULL CHECK(bytes>0 AND bytes<=2147483648),
 staging_key text NOT NULL UNIQUE, object_key text NOT NULL UNIQUE, completed boolean NOT NULL DEFAULT false,
 expires_at timestamptz NOT NULL DEFAULT now()+interval '24 hours',
 PRIMARY KEY(user_id,id), UNIQUE(user_id,snapshot_id),
 FOREIGN KEY(user_id,profile_id) REFERENCES save_profiles(user_id,id),
 FOREIGN KEY(user_id,device_id) REFERENCES devices(user_id,id)
);
CREATE TABLE save_objects (
 user_id uuid NOT NULL REFERENCES users, id uuid NOT NULL, object_key text NOT NULL UNIQUE,
 archive_hash text NOT NULL, manifest_hash text NOT NULL, bytes bigint NOT NULL,
 manifest jsonb NOT NULL, PRIMARY KEY(user_id,id)
);
CREATE TABLE save_snapshots (
 user_id uuid NOT NULL, id uuid NOT NULL, profile_id uuid NOT NULL, device_id uuid NOT NULL,
 object_id uuid NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(user_id,id), UNIQUE(user_id,profile_id,id),
 FOREIGN KEY(user_id,profile_id) REFERENCES save_profiles(user_id,id),
 FOREIGN KEY(user_id,device_id) REFERENCES devices(user_id,id),
 FOREIGN KEY(user_id,object_id) REFERENCES save_objects(user_id,id)
);
CREATE TABLE save_snapshot_parents (
 user_id uuid NOT NULL, profile_id uuid NOT NULL, snapshot_id uuid NOT NULL, parent_id uuid NOT NULL,
 PRIMARY KEY(user_id,snapshot_id,parent_id),
 FOREIGN KEY(user_id,profile_id,snapshot_id) REFERENCES save_snapshots(user_id,profile_id,id),
 FOREIGN KEY(user_id,profile_id,parent_id) REFERENCES save_snapshots(user_id,profile_id,id)
);
ALTER TABLE save_profiles ADD FOREIGN KEY(user_id,id,selected_snapshot_id)
 REFERENCES save_snapshots(user_id,profile_id,id);
CREATE TABLE save_object_gc (object_key text PRIMARY KEY, not_before timestamptz NOT NULL DEFAULT now());
CREATE INDEX save_upload_expiry ON save_uploads(expires_at) WHERE NOT completed;
