use sqlx::PgPool;

use super::model::{
    NewPeriode, Periode, StartCheck, StartResult, StatusPeriode, StopCheck, StopResult,
};

/// Daftar periode yang cocok dengan filter, dari waktu mulai yang paling baru.
///
/// Status periode dihitung saat query dijalankan dengan membandingkan waktu sekarang di
/// Asia/Jakarta terhadap `waktu_mulai` dan `waktu_tutup`. Kedua kolom itu tidak punya zona waktu
/// dan
/// disimpan dalam WIB, sehingga dikirim keluar dengan offset `+07:00`. Filter `id` dan `status`
/// bersifat opsional; `None` berarti tidak difilter.
pub async fn list(
    db: &PgPool,
    id: Option<i32>,
    status: Option<StatusPeriode>,
) -> Result<Vec<Periode>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id_periode, catatan,
                to_char(waktu_mulai, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS waktu_mulai,
                to_char(waktu_tutup, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS waktu_tutup,
                status
         FROM (
             SELECT *,
                    CASE
                        WHEN now() AT TIME ZONE 'Asia/Jakarta' < waktu_mulai THEN 'belum_dimulai'
                        WHEN now() AT TIME ZONE 'Asia/Jakarta' > waktu_tutup THEN 'selesai'
                        ELSE 'berlangsung'
                    END AS status
             FROM periode_pendaftaran
         ) periode
         WHERE ($1::int IS NULL OR id_periode = $1)
           AND ($2::text IS NULL OR status = $2)
         ORDER BY periode.waktu_mulai DESC, id_periode DESC",
    )
    .bind(id)
    .bind(status.map(StatusPeriode::as_str))
    .fetch_all(db)
    .await
}

/// Menyimpan periode baru dalam WIB.
///
/// Mengembalikan id periode baru, atau `None` bila rentangnya beririsan atau bersentuhan dengan
/// periode lain. Tabel dikunci (`SHARE ROW EXCLUSIVE`) selama transaksi; tanpa kunci ini, dua
/// request yang rentangnya beririsan bisa sama-sama tidak menemukan irisan lalu sama-sama
/// tersimpan.
pub async fn create(db: &PgPool, periode: &NewPeriode) -> Result<Option<i32>, sqlx::Error> {
    let mut tx = db.begin().await?;

    // Without the lock, two overlapping requests could both find no overlap and both be saved.
    sqlx::query("LOCK TABLE periode_pendaftaran IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;

    let id = sqlx::query_scalar(
        "WITH baru AS (
             SELECT $2::timestamptz AT TIME ZONE 'Asia/Jakarta' AS mulai,
                    $3::timestamptz AT TIME ZONE 'Asia/Jakarta' AS tutup
         )
         INSERT INTO periode_pendaftaran (catatan, waktu_mulai, waktu_tutup)
         SELECT $1, mulai, tutup
         FROM baru
         WHERE NOT EXISTS (
             SELECT 1
             FROM periode_pendaftaran p
             WHERE p.waktu_mulai <= baru.tutup AND p.waktu_tutup >= baru.mulai
         )
         RETURNING id_periode",
    )
    .bind(&periode.catatan)
    .bind(periode.waktu_mulai)
    .bind(periode.waktu_tutup)
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(id)
}

/// Memajukan `waktu_mulai` periode yang belum dimulai ke waktu sekarang (WIB), dengan `waktu_tutup`
/// tetap.
///
/// Rentang baru tidak boleh beririsan dengan periode lain, sama seperti aturan periode baru. Tabel
/// dikunci agar dua request tidak sama-sama menganggap periode belum dimulai. Waktu sekarang
/// diambil
/// dengan `clock_timestamp()`, bukan `now()`. `now()` tetap bernilai waktu awal transaksi, sehingga
/// bisa tertinggal dari periode yang baru saja dimulai oleh request yang memegang kunci sebelumnya.
///
/// Mengembalikan hasil berupa [`StartResult`]: berhasil, tidak ditemukan, sudah berlangsung, sudah
/// selesai, atau beririsan.
pub async fn start(db: &PgPool, id: i32) -> Result<StartResult, sqlx::Error> {
    let mut tx = db.begin().await?;

    // Without the lock, two requests could both find the period not started yet.
    sqlx::query("LOCK TABLE periode_pendaftaran IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;

    // clock_timestamp instead of now(), which stays at the time the transaction began and so
    // would be behind a period started by whoever held the lock before this request.
    let check: Option<StartCheck> = sqlx::query_as(
        "WITH sekarang AS (
             SELECT clock_timestamp() AT TIME ZONE 'Asia/Jakarta' AS waktu
         )
         SELECT t.waktu_mulai <= s.waktu AS sudah_dimulai,
                t.waktu_tutup < s.waktu AS sudah_selesai,
                EXISTS (
                    SELECT 1
                    FROM periode_pendaftaran p
                    WHERE p.id_periode <> t.id_periode
                      AND p.waktu_mulai <= t.waktu_tutup
                      AND p.waktu_tutup >= s.waktu
                ) AS beririsan
         FROM periode_pendaftaran t, sekarang s
         WHERE t.id_periode = $1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(check) = check else {
        return Ok(StartResult::NotFound);
    };
    if check.sudah_selesai {
        return Ok(StartResult::Finished);
    }
    if check.sudah_dimulai {
        return Ok(StartResult::AlreadyRunning);
    }
    if check.beririsan {
        return Ok(StartResult::Overlapping);
    }

    sqlx::query(
        "UPDATE periode_pendaftaran
         SET waktu_mulai = clock_timestamp() AT TIME ZONE 'Asia/Jakarta'
         WHERE id_periode = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(StartResult::Started)
}

/// Memundurkan `waktu_tutup` periode yang sedang berlangsung ke waktu sekarang (WIB), dengan
/// `waktu_mulai` tetap.
///
/// Rentangnya hanya memendek, sehingga tidak mungkin menjadi beririsan dengan periode lain. Tabel
/// dikunci dan waktu diambil dengan `clock_timestamp()`, dengan alasan yang sama seperti
/// [`start`]. Mengembalikan hasil berupa [`StopResult`]: berhasil, tidak ditemukan, belum dimulai,
/// atau sudah selesai.
pub async fn stop(db: &PgPool, id: i32) -> Result<StopResult, sqlx::Error> {
    let mut tx = db.begin().await?;

    // Without the lock, two requests could both find the period still running.
    sqlx::query("LOCK TABLE periode_pendaftaran IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;

    // clock_timestamp for the same reason as in `start`.
    let check: Option<StopCheck> = sqlx::query_as(
        "SELECT waktu_mulai <= clock_timestamp() AT TIME ZONE 'Asia/Jakarta' AS sudah_dimulai,
                waktu_tutup < clock_timestamp() AT TIME ZONE 'Asia/Jakarta' AS sudah_selesai
         FROM periode_pendaftaran
         WHERE id_periode = $1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(check) = check else {
        return Ok(StopResult::NotFound);
    };
    if check.sudah_selesai {
        return Ok(StopResult::Finished);
    }
    if !check.sudah_dimulai {
        return Ok(StopResult::NotStarted);
    }

    sqlx::query(
        "UPDATE periode_pendaftaran
         SET waktu_tutup = clock_timestamp() AT TIME ZONE 'Asia/Jakarta'
         WHERE id_periode = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(StopResult::Stopped)
}
