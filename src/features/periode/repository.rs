use sqlx::PgPool;

use super::model::{
    NewPeriode, Periode, StartCheck, StartResult, StatusPeriode, StopCheck, StopResult,
};

/// The periods matching the filters, newest first.
///
/// waktu_mulai and waktu_tutup have no time zone and are written in WIB, so they are compared
/// against the time in Jakarta, and sent out with the +07:00 offset.
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

/// Saves a period in WIB. Returns `None` if its range touches an existing period.
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

/// Moves waktu_mulai of a period that has not started to the current time in WIB, keeping
/// waktu_tutup. The new range must not touch another period, the same rule as for a new one.
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

/// Moves waktu_tutup of a running period to the current time in WIB, keeping waktu_mulai. The range
/// only gets shorter, so it cannot come to touch another period.
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
