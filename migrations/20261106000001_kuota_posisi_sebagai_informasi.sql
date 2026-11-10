-- Kuota posisi adalah kuota penerimaan dan hanya sebagai informasi: jumlah pendaftar tidak dibatasi
-- dan tidak ada lagi sisa kuota yang berkurang saat mendaftar.
ALTER TABLE posisi_magang DROP COLUMN sisa_kuota;
ALTER TABLE posisi_magang RENAME COLUMN kuota_total TO kuota;
