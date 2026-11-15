use sqlx::{PgConnection, PgPool};

use super::model::{
    AcceptResult, Berkas, BerkasBaru, FilterPendaftaran, Halaman, JadwalSeleksi, JumlahPendaftaran,
    LogStatus, LokasiBerkas, Pendaftaran, PendaftaranRingkas, PlottingDpm, PosisiTujuan,
    SimpanJadwal, SimpanPlotting, SimpanTahapResult, SuratKeterangan, UpdateStatusResult,
    panitia_magang_can_change,
};

/// Mengunci baris mahasiswa (`FOR UPDATE`) sampai transaksi selesai.
///
/// Request lain dari mahasiswa yang sama (mendaftar, konfirmasi penerimaan) tertahan selama kunci
/// dipegang, sehingga dua request tidak bisa sama-sama lolos pemeriksaan jumlah pendaftaran yang
/// berjalan.
pub async fn lock_mahasiswa(conn: &mut PgConnection, id_pengguna: i32) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1 FROM mahasiswa WHERE id_pengguna = $1 FOR UPDATE")
        .bind(id_pengguna)
        .execute(conn)
        .await?;
    Ok(())
}

/// Mengunci baris-baris pendaftaran ini sampai transaksi selesai, lalu membaca status terbarunya,
/// urut berdasarkan id.
///
/// Urutan penguncian yang tetap mencegah dua request saling menunggu (deadlock).
///
/// Penguncian dan pembacaan status sengaja dipisah menjadi dua statement. `FOR UPDATE` pada join
/// dengan `status_pendaftaran` bisa kehilangan baris yang statusnya berubah saat menunggu kunci:
/// PostgreSQL mengecek ulang join memakai `id_status` baru terhadap baris status lama, sehingga
/// barisnya tampak seperti tidak ada.
async fn lock_statuses(
    conn: &mut PgConnection,
    id_pendaftaran: &[i32],
) -> Result<Vec<(i32, String)>, sqlx::Error> {
    sqlx::query(
        "SELECT 1
         FROM pendaftaran
         WHERE id_pendaftaran = ANY($1)
         ORDER BY id_pendaftaran
         FOR UPDATE",
    )
    .bind(id_pendaftaran)
    .execute(&mut *conn)
    .await?;

    sqlx::query_as(
        "SELECT p.id_pendaftaran, s.status
         FROM pendaftaran p
         JOIN status_pendaftaran s ON s.id_status = p.id_status
         WHERE p.id_pendaftaran = ANY($1)
         ORDER BY p.id_pendaftaran",
    )
    .bind(id_pendaftaran)
    .fetch_all(conn)
    .await
}

/// Memeriksa posisi tujuan pendaftaran.
///
/// Mengembalikan `None` bila posisi tidak ada, disembunyikan, atau perusahaannya disembunyikan.
/// Bila
/// ada, hasilnya menyatakan apakah periodenya sedang berlangsung menurut waktu WIB. Kuota tidak
/// diperiksa, karena kuota hanya informasi dan jumlah pendaftar tidak dibatasi.
pub async fn find_posisi(
    conn: &mut PgConnection,
    id_posisi: i32,
) -> Result<Option<PosisiTujuan>, sqlx::Error> {
    sqlx::query_as(
        "SELECT now() AT TIME ZONE 'Asia/Jakarta' BETWEEN pp.waktu_mulai AND pp.waktu_tutup
                    AS periode_berlangsung
         FROM posisi_magang pm
         JOIN periode_pendaftaran pp ON pp.id_periode = pm.id_periode
         JOIN perusahaan_mitra pr ON pr.id_perusahaan = pm.id_perusahaan
         WHERE pm.id_posisi = $1 AND pm.deleted_at IS NULL AND pr.deleted_at IS NULL",
    )
    .bind(id_posisi)
    .fetch_optional(conn)
    .await
}

/// Menghitung pendaftaran mahasiswa ini untuk pemeriksaan sebelum mendaftar:
/// - `di_periode`: semua pendaftaran di periode posisi tujuan, dalam status apa pun.
/// - `berjalan_di_posisi`: pendaftaran di posisi tujuan yang belum berstatus akhir.
/// - `aktif`: pendaftaran berstatus `Aktif`.
///
/// Pendaftaran pada posisi yang sudah disembunyikan tetap dihitung.
pub async fn count_pendaftaran(
    conn: &mut PgConnection,
    id_pengguna: i32,
    id_posisi: i32,
) -> Result<JumlahPendaftaran, sqlx::Error> {
    sqlx::query_as(
        "SELECT count(*) FILTER (
                    WHERE pm.id_periode =
                        (SELECT id_periode FROM posisi_magang WHERE id_posisi = $2)
                ) AS di_periode,
                count(*) FILTER (
                    WHERE p.id_posisi = $2
                      AND s.status NOT IN ('Selesai', 'Tidak Lolos', 'Batal/Mengundurkan Diri')
                ) AS berjalan_di_posisi,
                count(*) FILTER (WHERE s.status = 'Aktif') AS aktif
         FROM pendaftaran p
         JOIN status_pendaftaran s ON s.id_status = p.id_status
         JOIN posisi_magang pm ON pm.id_posisi = p.id_posisi
         WHERE p.id_pengguna = $1",
    )
    .bind(id_pengguna)
    .bind(id_posisi)
    .fetch_one(conn)
    .await
}

/// Menyimpan pendaftaran baru berstatus `Menunggu Verifikasi`, baris log status pertamanya, dan
/// baris setiap berkasnya.
///
/// Kolom waktu tidak punya zona waktu, sehingga diisi waktu WIB secara eksplisit, tidak bergantung
/// pada zona waktu server database. Berkas di disk ditulis terpisah oleh pemanggil.
pub async fn insert(
    conn: &mut PgConnection,
    id_pengguna: i32,
    id_posisi: i32,
    berkas: &[BerkasBaru],
) -> Result<Pendaftaran, sqlx::Error> {
    let mut pendaftaran: Pendaftaran = sqlx::query_as(
        "WITH baru AS (
             INSERT INTO pendaftaran (id_posisi, id_status, id_pengguna, created_at, updated_at)
             SELECT $1, id_status, $2, now() AT TIME ZONE 'Asia/Jakarta',
                    now() AT TIME ZONE 'Asia/Jakarta'
             FROM status_pendaftaran
             WHERE status = 'Menunggu Verifikasi'
             RETURNING *
         ), log AS (
             INSERT INTO log_status (id_pendaftaran, status_baru, created_at)
             SELECT id_pendaftaran, id_status, created_at
             FROM baru
         )
         SELECT baru.id_pendaftaran, baru.id_posisi, s.status,
                to_char(baru.created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS created_at,
                to_char(baru.updated_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS updated_at
         FROM baru
         JOIN status_pendaftaran s ON s.id_status = baru.id_status",
    )
    .bind(id_posisi)
    .bind(id_pengguna)
    .fetch_one(&mut *conn)
    .await?;

    for file in berkas {
        let saved: Berkas = sqlx::query_as(
            "INSERT INTO berkas_pendaftaran
                 (id_pendaftaran, jenis_berkas, nama_berkas, ukuran_berkas)
             VALUES ($1, $2, $3, $4)
             RETURNING id_berkas, jenis_berkas, nama_berkas, ukuran_berkas",
        )
        .bind(pendaftaran.id_pendaftaran)
        .bind(file.jenis)
        .bind(&file.nama)
        .bind(file.isi.len() as i32)
        .fetch_one(&mut *conn)
        .await?;
        pendaftaran.berkas.push(saved);
    }

    Ok(pendaftaran)
}

/// Ekspresi SQL tenggat konfirmasi penerimaan untuk pendaftaran beralias `p`: 12 jam setelah
/// statusnya menjadi `Diterima`, dalam WIB seperti kolom waktu lainnya.
///
/// Waktu mulainya diambil dari baris `log_status` saat status berubah ke `Diterima`. Bila baris itu
/// tidak ada, `updated_at` yang dipakai, karena tidak ada proses lain yang mengubahnya selama
/// status masih `Diterima`. Makro ini satu-satunya sumber aturan tenggat, dipakai oleh daftar
/// pendaftaran, konfirmasi penerimaan, dan pembatalan otomatis.
macro_rules! batas_konfirmasi {
    () => {
        "(COALESCE(
             (SELECT max(l.created_at)
              FROM log_status l
              JOIN status_pendaftaran ls ON ls.id_status = l.status_baru
              WHERE l.id_pendaftaran = p.id_pendaftaran AND ls.status = 'Diterima'),
             p.updated_at
         ) + interval '12 hours')"
    };
}

/// Bagian `FROM ... WHERE` yang dipakai bersama oleh [`list`] dan [`count`], sehingga total selalu
/// mengikuti filter yang sama. Menerima field [`FilterPendaftaran`] sebagai `$1` sampai `$7`.
///
/// Pencarian nama memakai `strpos`, bukan `LIKE`, agar karakter `%` dan `_` dianggap karakter
/// biasa.
macro_rules! pendaftaran_terfilter {
    () => {
        " FROM pendaftaran p
         JOIN status_pendaftaran s ON s.id_status = p.id_status
         JOIN posisi_magang pm ON pm.id_posisi = p.id_posisi
         JOIN perusahaan_mitra pr ON pr.id_perusahaan = pm.id_perusahaan
         JOIN mahasiswa m ON m.id_pengguna = p.id_pengguna
         WHERE ($1::int IS NULL OR p.id_pengguna = $1)
           AND ($2::int IS NULL OR p.id_pendaftaran = $2)
           AND ($3::int IS NULL OR pm.id_periode = $3)
           AND ($4::int IS NULL OR p.id_status = $4)
           AND ($5::int IS NULL OR pm.id_perusahaan = $5)
           AND ($6::text IS NULL OR strpos(lower(m.nama), lower($6)) > 0)
           AND ($7::int IS NULL OR p.id_posisi = $7)"
    };
}

/// Pendaftaran yang cocok dengan filter, dari yang paling baru; semuanya bila `halaman` bernilai
/// `None`.
///
/// Kolom waktu disimpan dalam WIB, sehingga dikirim dengan offset `+07:00`. `batas_konfirmasi`
/// hanya
/// terisi untuk status `Diterima`. NIM di-cast ke teks agar spasi pengisi `CHAR` hilang.
pub async fn list(
    db: &PgPool,
    filter: &FilterPendaftaran,
    halaman: Option<Halaman>,
) -> Result<Vec<PendaftaranRingkas>, sqlx::Error> {
    sqlx::query_as(concat!(
        "SELECT p.id_pendaftaran, s.status,
                to_char(p.created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS created_at,
                to_char(p.updated_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS updated_at,
                pm.id_posisi, pm.posisi, pm.id_periode,
                pr.id_perusahaan, pr.nama_perusahaan,
                CASE WHEN s.status = 'Diterima' THEN to_char(",
        batas_konfirmasi!(),
        ", 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') END AS batas_konfirmasi,
                EXISTS (
                    SELECT 1 FROM jadwal_seleksi_substansi j
                    WHERE j.id_pendaftaran = p.id_pendaftaran
                ) AS ada_jadwal_seleksi,
                EXISTS (
                    SELECT 1 FROM plotting_dpm d
                    WHERE d.id_pendaftaran = p.id_pendaftaran
                ) AS ada_plotting_dpm,
                m.id_pengguna, m.nim::text AS nim, m.nama",
        pendaftaran_terfilter!(),
        " ORDER BY p.created_at DESC, p.id_pendaftaran DESC
         LIMIT $8 OFFSET $9",
    ))
    .bind(filter.id_pengguna)
    .bind(filter.id_pendaftaran)
    .bind(filter.id_periode)
    .bind(filter.id_status)
    .bind(filter.id_perusahaan)
    .bind(filter.nama.as_deref())
    .bind(filter.id_posisi)
    .bind(halaman.map(Halaman::limit))
    .bind(halaman.map_or(0, Halaman::offset))
    .fetch_all(db)
    .await
}

/// Jumlah seluruh baris [`list`] untuk filter ini, dari semua halaman.
pub async fn count(db: &PgPool, filter: &FilterPendaftaran) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(concat!("SELECT count(*)", pendaftaran_terfilter!()))
        .bind(filter.id_pengguna)
        .bind(filter.id_pendaftaran)
        .bind(filter.id_periode)
        .bind(filter.id_status)
        .bind(filter.id_perusahaan)
        .bind(filter.nama.as_deref())
        .bind(filter.id_posisi)
        .fetch_one(db)
        .await
}

/// Berkas-berkas sebuah pendaftaran, sesuai urutan disimpan: berkas wajib dulu, lalu berkas
/// tambahan.
pub async fn list_berkas(db: &PgPool, id_pendaftaran: i32) -> Result<Vec<Berkas>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id_berkas, jenis_berkas, nama_berkas, ukuran_berkas
         FROM berkas_pendaftaran
         WHERE id_pendaftaran = $1
         ORDER BY id_berkas",
    )
    .bind(id_pendaftaran)
    .fetch_all(db)
    .await
}

/// Riwayat perubahan status sebuah pendaftaran, dari yang paling lama, dengan waktu dalam WIB
/// (`+07:00`).
pub async fn list_log(db: &PgPool, id_pendaftaran: i32) -> Result<Vec<LogStatus>, sqlx::Error> {
    sqlx::query_as(
        "SELECT l.id_log, lama.status AS status_lama, baru.status AS status_baru,
                to_char(l.created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS created_at
         FROM log_status l
         LEFT JOIN status_pendaftaran lama ON lama.id_status = l.status_lama
         JOIN status_pendaftaran baru ON baru.id_status = l.status_baru
         WHERE l.id_pendaftaran = $1
         ORDER BY l.created_at, l.id_log",
    )
    .bind(id_pendaftaran)
    .fetch_all(db)
    .await
}

/// Jadwal seleksi substansi sebuah pendaftaran (tanggal `YYYY-MM-DD`, jam `HH:MM`, dan lokasi),
/// atau
/// `None` bila belum diatur.
pub async fn find_jadwal(
    db: &PgPool,
    id_pendaftaran: i32,
) -> Result<Option<JadwalSeleksi>, sqlx::Error> {
    sqlx::query_as(
        "SELECT to_char(tanggal, 'YYYY-MM-DD') AS tanggal, to_char(jam, 'HH24:MI') AS jam, lokasi
         FROM jadwal_seleksi_substansi
         WHERE id_pendaftaran = $1",
    )
    .bind(id_pendaftaran)
    .fetch_optional(db)
    .await
}

/// Ringkasan surat keterangan sebuah pendaftaran (nama berkas, ukuran, dan nomor surat), atau
/// `None`
/// bila belum diterbitkan.
pub async fn find_surat(
    db: &PgPool,
    id_pendaftaran: i32,
) -> Result<Option<SuratKeterangan>, sqlx::Error> {
    sqlx::query_as(
        "SELECT nama_berkas, ukuran_berkas, nomor_surat
         FROM surat_keterangan
         WHERE id_pendaftaran = $1",
    )
    .bind(id_pendaftaran)
    .fetch_optional(db)
    .await
}

/// Plotting DPM sebuah pendaftaran (nama dan kontak DPM), atau `None` bila belum diatur.
pub async fn find_plotting(
    db: &PgPool,
    id_pendaftaran: i32,
) -> Result<Option<PlottingDpm>, sqlx::Error> {
    sqlx::query_as("SELECT nama_dpm, kontak_dpm FROM plotting_dpm WHERE id_pendaftaran = $1")
        .bind(id_pendaftaran)
        .fetch_optional(db)
        .await
}

/// Mengubah status pendaftaran dan mencatat siapa yang mengubahnya.
///
/// Dalam satu transaksi, pendaftaran dikunci dan status terbarunya dibaca, lalu perubahan dicek
/// dengan [`panitia_magang_can_change`]. Bila diizinkan, status dan `updated_at` diubah, dan baris
/// `log_status` ditambahkan berisi status lama, status baru, dan id Panitia Magang. Kuota posisi
/// tidak berubah, apa pun status barunya.
///
/// Waktu diambil dengan `clock_timestamp()`, bukan `now()`. `now()` tetap bernilai waktu awal
/// transaksi, sehingga perubahan yang sempat menunggu kunci bisa tercatat lebih awal dari perubahan
/// sebelumnya.
///
/// Bila berhasil, status lama dikembalikan untuk keperluan email pemberitahuan.
pub async fn update_status(
    db: &PgPool,
    id_pendaftaran: i32,
    id_pengubah: i32,
    status_baru: &str,
) -> Result<UpdateStatusResult, sqlx::Error> {
    let mut tx = db.begin().await?;

    // Each change checks against the latest status.
    let statuses = lock_statuses(&mut tx, &[id_pendaftaran]).await?;
    let Some((_, status_lama)) = statuses.into_iter().next() else {
        return Ok(UpdateStatusResult::NotFound);
    };
    if !panitia_magang_can_change(&status_lama, status_baru) {
        return Ok(UpdateStatusResult::NotAllowed);
    }

    // clock_timestamp instead of now(), which stays at the time the transaction began, so that a
    // change that had to wait for the one before it is still logged after it.
    sqlx::query(
        "WITH sekarang AS (
             SELECT clock_timestamp() AT TIME ZONE 'Asia/Jakarta' AS waktu
         ), baru AS (
             SELECT id_status FROM status_pendaftaran WHERE status = $3
         ), ubah AS (
             UPDATE pendaftaran p
             SET id_status = baru.id_status, updated_at = sekarang.waktu
             FROM baru, sekarang
             WHERE p.id_pendaftaran = $1
         )
         INSERT INTO log_status
             (id_pendaftaran, id_pengubah, status_lama, status_baru, created_at)
         SELECT $1, $2, lama.id_status, baru.id_status, sekarang.waktu
         FROM baru, sekarang, status_pendaftaran lama
         WHERE lama.status = $4",
    )
    .bind(id_pendaftaran)
    .bind(id_pengubah)
    .bind(status_baru)
    .bind(&status_lama)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(UpdateStatusResult::Updated { status_lama })
}

/// Mengunci pendaftaran-pendaftaran ini dan memeriksa apakah semuanya bisa menerima data sebuah
/// tahap.
///
/// Semua harus ada dan berstatus `status_wajib`, dan kondisi itu tetap berlaku sampai transaksi
/// selesai karena barisnya terkunci. Mengembalikan alasan penolakan (`NotFound` atau `NotAllowed`),
/// atau `None` bila semuanya memenuhi.
async fn cek_tahap(
    conn: &mut PgConnection,
    id_pendaftaran: &[i32],
    status_wajib: &str,
) -> Result<Option<SimpanTahapResult>, sqlx::Error> {
    let statuses = lock_statuses(conn, id_pendaftaran).await?;

    if statuses.len() != id_pendaftaran.len() {
        return Ok(Some(SimpanTahapResult::NotFound));
    }
    if statuses.iter().any(|(_, status)| status != status_wajib) {
        return Ok(Some(SimpanTahapResult::NotAllowed));
    }
    Ok(None)
}

/// Mengisi `updated_at` pendaftaran-pendaftaran ini dengan waktu sekarang (WIB).
///
/// Waktu ini tampil di dasbor mahasiswa sebagai perubahan terakhir. Statusnya tidak berubah,
/// sehingga tidak ada baris yang ditambahkan ke `log_status`.
async fn perbarui_updated_at(
    conn: &mut PgConnection,
    id_pendaftaran: &[i32],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE pendaftaran
         SET updated_at = clock_timestamp() AT TIME ZONE 'Asia/Jakarta'
         WHERE id_pendaftaran = ANY($1)",
    )
    .bind(id_pendaftaran)
    .execute(conn)
    .await?;
    Ok(())
}

/// Menyimpan jadwal seleksi yang sama ke semua pendaftaran yang disebut, atau tidak ke satu pun
/// bila
/// ada yang tidak memenuhi syarat.
///
/// Semua pendaftaran harus berstatus `Lolos Seleksi Administrasi`. Jadwal yang sudah ada diganti
/// (upsert), dan `updated_at` setiap pendaftaran diperbarui.
pub async fn simpan_jadwal(
    db: &PgPool,
    data: &SimpanJadwal,
) -> Result<SimpanTahapResult, sqlx::Error> {
    let mut tx = db.begin().await?;

    if let Some(ditolak) =
        cek_tahap(&mut tx, &data.id_pendaftaran, "Lolos Seleksi Administrasi").await?
    {
        return Ok(ditolak);
    }

    sqlx::query(
        "INSERT INTO jadwal_seleksi_substansi (id_pendaftaran, tanggal, jam, lokasi)
         SELECT id, $2::date, $3::time, $4
         FROM unnest($1::int[]) AS id
         ON CONFLICT (id_pendaftaran) DO UPDATE
         SET tanggal = EXCLUDED.tanggal, jam = EXCLUDED.jam, lokasi = EXCLUDED.lokasi",
    )
    .bind(&data.id_pendaftaran)
    .bind(&data.tanggal)
    .bind(&data.jam)
    .bind(&data.lokasi)
    .execute(&mut *tx)
    .await?;

    perbarui_updated_at(&mut tx, &data.id_pendaftaran).await?;
    tx.commit().await?;

    Ok(SimpanTahapResult::Saved)
}

/// Menyimpan DPM yang sama ke semua pendaftaran yang disebut, atau tidak ke satu pun bila ada yang
/// tidak memenuhi syarat.
///
/// Semua pendaftaran harus berstatus `Aktif`. Plotting yang sudah ada diganti (upsert), dan
/// `updated_at` setiap pendaftaran diperbarui.
pub async fn simpan_plotting(
    db: &PgPool,
    data: &SimpanPlotting,
) -> Result<SimpanTahapResult, sqlx::Error> {
    let mut tx = db.begin().await?;

    if let Some(ditolak) = cek_tahap(&mut tx, &data.id_pendaftaran, "Aktif").await? {
        return Ok(ditolak);
    }

    sqlx::query(
        "INSERT INTO plotting_dpm (id_pendaftaran, nama_dpm, kontak_dpm)
         SELECT id, $2, $3
         FROM unnest($1::int[]) AS id
         ON CONFLICT (id_pendaftaran) DO UPDATE
         SET nama_dpm = EXCLUDED.nama_dpm, kontak_dpm = EXCLUDED.kontak_dpm",
    )
    .bind(&data.id_pendaftaran)
    .bind(&data.nama_dpm)
    .bind(&data.kontak_dpm)
    .execute(&mut *tx)
    .await?;

    perbarui_updated_at(&mut tx, &data.id_pendaftaran).await?;
    tx.commit().await?;

    Ok(SimpanTahapResult::Saved)
}

/// Mengubah pendaftaran ini menjadi `Aktif` dan membatalkan pendaftaran lain milik mahasiswa yang
/// masih berjalan.
///
/// Dalam satu transaksi:
/// 1. Pendaftaran harus ada dan milik `id_pengguna`.
/// 2. Mahasiswa dikunci, sehingga pendaftaran baru tidak bisa menyelip di samping yang `Aktif`.
/// 3. Semua pendaftaran mahasiswa dikunci, sehingga perubahan status oleh Panitia Magang tertahan.
/// 4. Pendaftaran harus `Diterima`, mahasiswa belum punya yang `Aktif`, dan tenggat konfirmasi 12
///    jam
///    belum lewat.
/// 5. Pendaftaran ini menjadi `Aktif`, dan pendaftaran lain yang belum berstatus akhir menjadi
///    `Batal/Mengundurkan Diri`, masing-masing dengan baris log.
///
/// `id_pengubah` di log dibiarkan kosong, karena kolom itu hanya merujuk ke Panitia Magang, sama
/// seperti log pertama pendaftaran baru. Kuota tidak berubah.
pub async fn accept(
    db: &PgPool,
    id_pendaftaran: i32,
    id_pengguna: i32,
) -> Result<AcceptResult, sqlx::Error> {
    let mut tx = db.begin().await?;

    let pemilik: Option<i32> =
        sqlx::query_scalar("SELECT id_pengguna FROM pendaftaran WHERE id_pendaftaran = $1")
            .bind(id_pendaftaran)
            .fetch_optional(&mut *tx)
            .await?;

    match pemilik {
        None => return Ok(AcceptResult::NotFound),
        Some(pemilik) if pemilik != id_pengguna => return Ok(AcceptResult::NotOwner),
        Some(_) => {}
    }

    // Holds back this mahasiswa's new registrations and other accepts, so a registration cannot
    // slip in beside the Aktif one.
    lock_mahasiswa(&mut tx, id_pengguna).await?;

    // The mahasiswa's lock keeps this list from growing, and the statuses from here on are held
    // back from Panitia Magang's changes, so the checks and the changes below all see the latest ones.
    let semua: Vec<i32> =
        sqlx::query_scalar("SELECT id_pendaftaran FROM pendaftaran WHERE id_pengguna = $1")
            .bind(id_pengguna)
            .fetch_all(&mut *tx)
            .await?;
    let statuses = lock_statuses(&mut tx, &semua).await?;

    if !statuses
        .iter()
        .any(|(id, status)| *id == id_pendaftaran && status == "Diterima")
    {
        return Ok(AcceptResult::NotDiterima);
    }
    if statuses.iter().any(|(_, status)| status == "Aktif") {
        return Ok(AcceptResult::HasAktif);
    }

    let kedaluwarsa: bool = sqlx::query_scalar(concat!(
        "SELECT ",
        batas_konfirmasi!(),
        " <= clock_timestamp() AT TIME ZONE 'Asia/Jakarta'
         FROM pendaftaran p
         WHERE p.id_pendaftaran = $1",
    ))
    .bind(id_pendaftaran)
    .fetch_one(&mut *tx)
    .await?;
    if kedaluwarsa {
        return Ok(AcceptResult::Kedaluwarsa);
    }

    // clock_timestamp instead of now(), which stays at the time the transaction began, so that a
    // change that had to wait for the locks is still logged after the one before it.
    sqlx::query(
        "WITH sekarang AS (
             SELECT clock_timestamp() AT TIME ZONE 'Asia/Jakarta' AS waktu
         ), perubahan AS (
             SELECT p.id_pendaftaran, p.id_status AS status_lama, baru.id_status AS status_baru
             FROM pendaftaran p
             JOIN status_pendaftaran lama ON lama.id_status = p.id_status
             JOIN status_pendaftaran baru ON baru.status = CASE
                 WHEN p.id_pendaftaran = $1 THEN 'Aktif'
                 ELSE 'Batal/Mengundurkan Diri'
             END
             WHERE p.id_pengguna = $2
               AND lama.status NOT IN ('Selesai', 'Tidak Lolos', 'Batal/Mengundurkan Diri')
         ), ubah AS (
             UPDATE pendaftaran p
             SET id_status = perubahan.status_baru, updated_at = sekarang.waktu
             FROM perubahan, sekarang
             WHERE p.id_pendaftaran = perubahan.id_pendaftaran
         )
         INSERT INTO log_status (id_pendaftaran, status_lama, status_baru, created_at)
         SELECT perubahan.id_pendaftaran, perubahan.status_lama, perubahan.status_baru,
                sekarang.waktu
         FROM perubahan, sekarang",
    )
    .bind(id_pendaftaran)
    .bind(id_pengguna)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(AcceptResult::Accepted)
}

/// Membatalkan semua pendaftaran `Diterima` yang tenggat konfirmasinya sudah lewat, lalu
/// mengembalikan id pendaftaran yang dibatalkan.
///
/// Kandidat dicari dulu, lalu dikunci berurutan dan dicek ulang statusnya. Dengan begitu,
/// mahasiswa yang berhasil mengonfirmasi tepat sebelum tenggat (saat proses ini menunggu kunci)
/// tidak ikut dibatalkan. Bila ada beberapa server yang menjalankan proses ini bersamaan, tidak
/// ada pembatalan ganda.
///
/// Status diubah menjadi `Batal/Mengundurkan Diri` dengan baris log berstatus lama `Diterima`.
/// `id_pengubah` dibiarkan kosong karena tidak ada orang yang melakukan perubahan ini.
pub async fn batalkan_yang_kedaluwarsa(db: &PgPool) -> Result<Vec<i32>, sqlx::Error> {
    let mut tx = db.begin().await?;

    let calon: Vec<i32> = sqlx::query_scalar(concat!(
        "SELECT p.id_pendaftaran
         FROM pendaftaran p
         JOIN status_pendaftaran s ON s.id_status = p.id_status
         WHERE s.status = 'Diterima'
           AND ",
        batas_konfirmasi!(),
        " <= clock_timestamp() AT TIME ZONE 'Asia/Jakarta'",
    ))
    .fetch_all(&mut *tx)
    .await?;
    if calon.is_empty() {
        return Ok(calon);
    }

    // A mahasiswa may have confirmed while waiting for the lock, so only those still Diterima go.
    let batal: Vec<i32> = lock_statuses(&mut tx, &calon)
        .await?
        .into_iter()
        .filter(|(_, status)| status == "Diterima")
        .map(|(id, _)| id)
        .collect();

    sqlx::query(
        "WITH sekarang AS (
             SELECT clock_timestamp() AT TIME ZONE 'Asia/Jakarta' AS waktu
         ), lama AS (
             SELECT id_status FROM status_pendaftaran WHERE status = 'Diterima'
         ), baru AS (
             SELECT id_status FROM status_pendaftaran WHERE status = 'Batal/Mengundurkan Diri'
         ), ubah AS (
             UPDATE pendaftaran p
             SET id_status = baru.id_status, updated_at = sekarang.waktu
             FROM baru, sekarang
             WHERE p.id_pendaftaran = ANY($1)
         )
         INSERT INTO log_status (id_pendaftaran, status_lama, status_baru, created_at)
         SELECT id, lama.id_status, baru.id_status, sekarang.waktu
         FROM unnest($1::int[]) AS id, lama, baru, sekarang",
    )
    .bind(&batal)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(batal)
}

/// Lokasi dan pemilik sebuah berkas pendaftaran: nama berkas di server, jenis berkas, id pengguna
/// pemilik, dan NIM-nya. Mengembalikan `None` bila berkas tidak ada.
pub async fn find_berkas(db: &PgPool, id_berkas: i32) -> Result<Option<LokasiBerkas>, sqlx::Error> {
    sqlx::query_as(
        "SELECT b.nama_berkas, b.jenis_berkas, p.id_pengguna, m.nim::text AS nim
         FROM berkas_pendaftaran b
         JOIN pendaftaran p ON p.id_pendaftaran = b.id_pendaftaran
         JOIN mahasiswa m ON m.id_pengguna = p.id_pengguna
         WHERE b.id_berkas = $1",
    )
    .bind(id_berkas)
    .fetch_optional(db)
    .await
}
