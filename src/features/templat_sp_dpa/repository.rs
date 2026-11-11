use sqlx::PgPool;

use super::model::TemplatSpDpa;

/// The template in use: the one saved last. Older rows are kept as history.
///
/// updated_at has no time zone and is written in WIB, so it is sent out with the +07:00 offset.
pub async fn find(db: &PgPool) -> Result<Option<TemplatSpDpa>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id_templat, tautan_templat,
                to_char(updated_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS updated_at
         FROM templat_sp_dpa
         ORDER BY updated_at DESC, id_templat DESC
         LIMIT 1",
    )
    .fetch_optional(db)
    .await
}

/// Saves a new link as the template in use.
pub async fn simpan(db: &PgPool, tautan: &str) -> Result<TemplatSpDpa, sqlx::Error> {
    sqlx::query_as(
        "INSERT INTO templat_sp_dpa (tautan_templat, updated_at)
         VALUES ($1, clock_timestamp() AT TIME ZONE 'Asia/Jakarta')
         RETURNING id_templat, tautan_templat,
                   to_char(updated_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS updated_at",
    )
    .bind(tautan)
    .fetch_one(db)
    .await
}
