-- Tim MB is registered by email before the first login, when the Google ID isn't known yet.
ALTER TABLE pengguna ALTER COLUMN google_sub DROP NOT NULL;
