ALTER TABLE save_uploads
 ADD COLUMN status text NOT NULL DEFAULT 'uploading' CHECK(status IN ('uploading','queued','validating','failed','completed')),
 ADD COLUMN attempts integer NOT NULL DEFAULT 0 CHECK(attempts>=0),
 ADD COLUMN next_attempt_at timestamptz NOT NULL DEFAULT now(),
 ADD COLUMN lease_token uuid,
 ADD COLUMN lease_until timestamptz,
 ADD COLUMN last_error text NOT NULL DEFAULT '';
UPDATE save_uploads SET status='completed' WHERE completed;
CREATE INDEX save_jobs_ready ON save_uploads(status,next_attempt_at,lease_until) WHERE NOT completed;
