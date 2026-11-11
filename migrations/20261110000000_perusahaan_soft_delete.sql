-- A hidden company stays in the table with the time it was hidden, so its positions and the
-- pendaftaran on them keep their history. Only companies without deleted_at are offered for new
-- positions and registrations.
ALTER TABLE perusahaan_mitra ADD COLUMN deleted_at TIMESTAMP;
