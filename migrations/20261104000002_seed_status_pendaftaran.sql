-- The statuses a pendaftaran moves through. A new pendaftaran starts at Menunggu Verifikasi.
-- Ordered so that the ids follow that same flow.
INSERT INTO status_pendaftaran (status)
SELECT baru.status
FROM unnest(ARRAY[
    'Menunggu Verifikasi',
    'Lolos Seleksi Administrasi',
    'Diterima',
    'Aktif',
    'Selesai',
    'Tidak Lolos',
    'Batal/Mengundurkan Diri'
]) WITH ORDINALITY AS baru (status, urutan)
WHERE NOT EXISTS (
    SELECT 1 FROM status_pendaftaran s WHERE s.status = baru.status
)
ORDER BY baru.urutan;
