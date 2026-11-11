-- A hidden position stays in the table with the time it was hidden, so the pendaftaran on it keep
-- their history. Only positions without deleted_at are shown in the catalog and open to register.
ALTER TABLE posisi_magang ADD COLUMN deleted_at TIMESTAMP;
