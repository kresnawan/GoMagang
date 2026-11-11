-- A hidden mata kuliah stays in the table with the time it was hidden, so the penyetaraan that
-- already chose it keep their history. Only mata kuliah without deleted_at can be chosen again.
ALTER TABLE mata_kuliah_konversi ADD COLUMN deleted_at TIMESTAMP;

-- A pendaftaran may choose each mata kuliah once. Doubles left from before are dropped first,
-- keeping the earliest one.
DELETE FROM penyetaraan_mk lama
USING penyetaraan_mk awal
WHERE lama.id_pendaftaran = awal.id_pendaftaran
  AND lama.id_mk_konversi = awal.id_mk_konversi
  AND lama.id_penyetaraan > awal.id_penyetaraan;

ALTER TABLE penyetaraan_mk
    ADD CONSTRAINT penyetaraan_mk_pendaftaran_mk_key UNIQUE (id_pendaftaran, id_mk_konversi);
