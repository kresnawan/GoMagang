use sqlx::PgPool;

use super::model::{NewPeriode, Periode, StatusPeriode};

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
