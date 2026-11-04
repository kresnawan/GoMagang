use sqlx::PgPool;

use super::model::RegisterTimMb;

/// Creates the pengguna and tim_mb rows together, with `google_sub` left empty until first login.
///
/// Returns `None` if the email is already taken, in any letter case.
pub async fn register(db: &PgPool, data: &RegisterTimMb) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar(
        "WITH baru AS (
             INSERT INTO pengguna (email)
             SELECT $1::varchar
             WHERE NOT EXISTS (SELECT 1 FROM pengguna WHERE lower(email) = lower($1))
             RETURNING id_pengguna
         )
         INSERT INTO tim_mb (id_pengguna, nama, no_hp)
         SELECT id_pengguna, $2, $3 FROM baru
         RETURNING id_pengguna",
    )
    .bind(&data.email)
    .bind(&data.nama)
    .bind(&data.no_hp)
    .fetch_optional(db)
    .await
}
