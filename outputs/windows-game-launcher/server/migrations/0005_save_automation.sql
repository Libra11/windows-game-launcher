ALTER TABLE save_profiles ADD COLUMN directory_rule jsonb;
ALTER TABLE save_profiles ADD COLUMN auto_retention boolean NOT NULL DEFAULT false;
ALTER TABLE save_snapshots ADD COLUMN pinned boolean NOT NULL DEFAULT false;
ALTER TABLE save_objects ADD COLUMN available boolean NOT NULL DEFAULT true;
