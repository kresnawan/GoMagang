use sqlx::PgPool;

use super::model::RegisterPanitiaMagang;

/// Membuat baris `pengguna` dan `panitia_magang` sekaligus dalam satu statement.
///
/// `google_sub` dibiarkan kosong sampai anggota tersebut login pertama kali. Mengembalikan
/// `id_pengguna` yang baru, atau `None` bila email sudah dipakai, tanpa membedakan huruf
/// besar-kecil.
pub async fn register(
    db: &PgPool,
    data: &RegisterPanitiaMagang,
) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar(
        "WITH baru AS (
             INSERT INTO pengguna (email)
             SELECT $1::varchar
             WHERE NOT EXISTS (SELECT 1 FROM pengguna WHERE lower(email) = lower($1))
             RETURNING id_pengguna
         )
         INSERT INTO panitia_magang (id_pengguna, nama, no_hp)
         SELECT id_pengguna, $2, $3 FROM baru
         RETURNING id_pengguna",
    )
    .bind(&data.email)
    .bind(&data.nama)
    .bind(&data.no_hp)
    .fetch_optional(db)
    .await
}
