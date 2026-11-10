use sqlx::PgPool;

use super::model::{PerusahaanMitra, RenameResult};

/// The companies by name, leaving out the hidden ones unless asked for. Hidden ones come last.
///
/// deleted_at has no time zone and is written in WIB, so it is sent out with the +07:00 offset.
pub async fn list(
    db: &PgPool,
    termasuk_dihapus: bool,
) -> Result<Vec<PerusahaanMitra>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id_perusahaan, nama_perusahaan,
                to_char(deleted_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS deleted_at
         FROM perusahaan_mitra
         WHERE $1 OR deleted_at IS NULL
         ORDER BY deleted_at IS NOT NULL, nama_perusahaan, id_perusahaan",
    )
    .bind(termasuk_dihapus)
    .fetch_all(db)
    .await
}

/// Saves a company. Returns `None` if the name is already taken by a company that is not hidden,
/// in any letter case. A hidden company's name is free to use again.
pub async fn create(db: &PgPool, nama: &str) -> Result<Option<PerusahaanMitra>, sqlx::Error> {
    let mut tx = db.begin().await?;

    // Without the lock, two requests with the same name could both find it free and both be saved.
    sqlx::query("LOCK TABLE perusahaan_mitra IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;

    let perusahaan = sqlx::query_as(
        "INSERT INTO perusahaan_mitra (nama_perusahaan)
         SELECT $1::varchar
         WHERE NOT EXISTS (
             SELECT 1 FROM perusahaan_mitra
             WHERE lower(nama_perusahaan) = lower($1) AND deleted_at IS NULL
         )
         RETURNING id_perusahaan, nama_perusahaan, NULL::text AS deleted_at",
    )
    .bind(nama)
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(perusahaan)
}

/// Renames a company that is not hidden. The new name may not belong to another company that is
/// not hidden, in any letter case.
pub async fn rename(
    db: &PgPool,
    id_perusahaan: i32,
    nama: &str,
) -> Result<RenameResult, sqlx::Error> {
    let mut tx = db.begin().await?;

    // The same lock as `create`, so that a rename and a create cannot both take one name.
    sqlx::query("LOCK TABLE perusahaan_mitra IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;

    let ada: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM perusahaan_mitra WHERE id_perusahaan = $1 AND deleted_at IS NULL
         )",
    )
    .bind(id_perusahaan)
    .fetch_one(&mut *tx)
    .await?;
    if !ada {
        return Ok(RenameResult::NotFound);
    }

    let perusahaan: Option<PerusahaanMitra> = sqlx::query_as(
        "UPDATE perusahaan_mitra
         SET nama_perusahaan = $2
         WHERE id_perusahaan = $1
           AND NOT EXISTS (
               SELECT 1 FROM perusahaan_mitra lain
               WHERE lower(lain.nama_perusahaan) = lower($2)
                 AND lain.deleted_at IS NULL
                 AND lain.id_perusahaan <> $1
           )
         RETURNING id_perusahaan, nama_perusahaan, NULL::text AS deleted_at",
    )
    .bind(id_perusahaan)
    .bind(nama)
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(match perusahaan {
        Some(perusahaan) => RenameResult::Renamed(perusahaan),
        None => RenameResult::NameTaken,
    })
}

/// Hides a company. Returns `None` if there is no such company, or it is hidden already.
pub async fn soft_delete(
    db: &PgPool,
    id_perusahaan: i32,
) -> Result<Option<PerusahaanMitra>, sqlx::Error> {
    sqlx::query_as(
        "UPDATE perusahaan_mitra
         SET deleted_at = clock_timestamp() AT TIME ZONE 'Asia/Jakarta'
         WHERE id_perusahaan = $1 AND deleted_at IS NULL
         RETURNING id_perusahaan, nama_perusahaan,
                   to_char(deleted_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS deleted_at",
    )
    .bind(id_perusahaan)
    .fetch_optional(db)
    .await
}
