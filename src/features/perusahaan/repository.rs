use sqlx::PgPool;

use super::model::Perusahaan;

pub async fn list(db: &PgPool) -> Result<Vec<Perusahaan>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id_perusahaan, nama_perusahaan
         FROM perusahaan_mitra
         ORDER BY nama_perusahaan, id_perusahaan",
    )
    .fetch_all(db)
    .await
}

/// Saves a company. Returns `None` if the name is already taken, in any letter case.
pub async fn create(db: &PgPool, nama: &str) -> Result<Option<Perusahaan>, sqlx::Error> {
    let mut tx = db.begin().await?;

    // Without the lock, two requests with the same name could both find it free and both be saved.
    sqlx::query("LOCK TABLE perusahaan_mitra IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;

    let perusahaan = sqlx::query_as(
        "INSERT INTO perusahaan_mitra (nama_perusahaan)
         SELECT $1::varchar
         WHERE NOT EXISTS (
             SELECT 1 FROM perusahaan_mitra WHERE lower(nama_perusahaan) = lower($1)
         )
         RETURNING id_perusahaan, nama_perusahaan",
    )
    .bind(nama)
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(perusahaan)
}
