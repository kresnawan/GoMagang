use sqlx::{PgConnection, PgPool};

use super::model::{
    AcceptResult, Berkas, BerkasBaru, FilterPendaftaran, Halaman, JadwalSeleksi, JumlahBerjalan,
    LogStatus, LokasiBerkas, Pendaftaran, PendaftaranRingkas, PlottingDpm, PosisiTujuan,
    SimpanJadwal, SimpanPlotting, SimpanTahapResult, SuratKeterangan, UpdateStatusResult,
    tim_mb_can_change,
};

/// Holds back this mahasiswa's other requests until the transaction ends, so that two requests
/// cannot both pass the checks on how many pendaftaran are running.
pub async fn lock_mahasiswa(conn: &mut PgConnection, id_pengguna: i32) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1 FROM mahasiswa WHERE id_pengguna = $1 FOR UPDATE")
        .bind(id_pengguna)
        .execute(conn)
        .await?;
    Ok(())
}

/// Holds back other changes to these pendaftaran until the transaction ends, then reads their
/// latest statuses, ordered by id. The fixed order keeps two requests from deadlocking.
///
/// Two statements on purpose. FOR UPDATE on a join with status_pendaftaran would lose a row whose
/// status changed while waiting for the lock: Postgres rechecks the join using the new id_status
/// against the old status row, so the row looks as if it does not exist.
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

/// Whether the position exists, is not hidden, belongs to a company that is not hidden, and its
/// period is running. Its quota is information only, so any number of mahasiswa may register for it.
///
/// waktu_mulai and waktu_tutup have no time zone and are written in WIB.
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

/// Counts the pendaftaran that have not reached a final status, in total, for this position, and
/// those that are Aktif.
pub async fn count_berjalan(
    conn: &mut PgConnection,
    id_pengguna: i32,
    id_posisi: i32,
) -> Result<JumlahBerjalan, sqlx::Error> {
    sqlx::query_as(
        "SELECT count(*) AS total,
                count(*) FILTER (WHERE p.id_posisi = $2) AS di_posisi,
                count(*) FILTER (WHERE s.status = 'Aktif') AS aktif
         FROM pendaftaran p
         JOIN status_pendaftaran s ON s.id_status = p.id_status
         WHERE p.id_pengguna = $1
           AND s.status NOT IN ('Selesai', 'Tidak Lolos', 'Batal/Mengundurkan Diri')",
    )
    .bind(id_pengguna)
    .bind(id_posisi)
    .fetch_one(conn)
    .await
}

/// Saves the pendaftaran with its first status log and its files.
///
/// The time columns have no time zone, so they are written in WIB explicitly instead of relying
/// on the database server's own time zone.
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

/// The rows that `list` and `count` look at, so that the total always follows the same filters.
/// Takes the fields of `FilterPendaftaran` as $1 to $7.
///
/// strpos rather than LIKE, so that a % or _ in the search is just a character to look for.
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

/// The pendaftaran matching the filters, newest first. All of them if no page is given.
///
/// The time columns are written in WIB, so they are sent out with the +07:00 offset. The cast drops
/// the CHAR padding from nim.
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

/// How many rows `list` has for these filters, across all of its pages.
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

/// The status changes of a pendaftaran, oldest first.
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

pub async fn find_plotting(
    db: &PgPool,
    id_pendaftaran: i32,
) -> Result<Option<PlottingDpm>, sqlx::Error> {
    sqlx::query_as("SELECT nama_dpm, kontak_dpm FROM plotting_dpm WHERE id_pendaftaran = $1")
        .bind(id_pendaftaran)
        .fetch_optional(db)
        .await
}

/// Changes the status and logs who did it. The quota stays as it is, whatever the new status.
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
    if !tim_mb_can_change(&status_lama, status_baru) {
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

    Ok(UpdateStatusResult::Updated)
}

/// Locks these pendaftaran and says why they cannot take the data of a stage, if so. Every one of
/// them must exist and be in this status, and that stays true until the transaction ends.
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

/// Shows on the mahasiswa's dashboard as the last change. The status stays as it is, so nothing is
/// added to log_status.
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

/// Saves the same jadwal on every pendaftaran listed, replacing any they already have, or on none
/// of them if one cannot take it.
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

/// Saves the same DPM on every pendaftaran listed, replacing any they already have, or on none of
/// them if one cannot take it.
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

/// Makes this pendaftaran Aktif and withdraws the mahasiswa from their other running ones, logging
/// each change. The quota stays as it is, and pendaftaran with a final status are left alone.
///
/// id_pengubah only refers to Tim MB, so it stays empty for what the mahasiswa does themselves, as
/// in the first log of a new pendaftaran.
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
    // back from Tim MB's changes, so the checks and the changes below all see the latest ones.
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
