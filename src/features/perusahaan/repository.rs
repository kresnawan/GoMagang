use sqlx::PgPool;

use super::model::{PerusahaanMitra, RenameResult};

/// Daftar perusahaan berdasarkan nama; yang disembunyikan tidak disertakan kecuali diminta.
///
/// Bila `termasuk_dihapus` bernilai `true`, perusahaan yang disembunyikan ditaruh di akhir daftar.
/// `deleted_at` tidak punya zona waktu dan disimpan dalam WIB, sehingga dikirim dengan offset
/// `+07:00`.
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

/// Menyimpan perusahaan baru.
///
/// Mengembalikan `None` bila nama sudah dipakai perusahaan yang tidak disembunyikan, tanpa
/// membedakan huruf besar-kecil. Nama perusahaan yang sudah disembunyikan boleh dipakai lagi.
///
/// Tabel dikunci selama transaksi; tanpa kunci ini, dua request dengan nama yang sama bisa
/// sama-sama menganggap nama itu bebas lalu sama-sama tersimpan.
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

/// Mengganti nama perusahaan yang tidak disembunyikan.
///
/// Nama baru tidak boleh dipakai perusahaan lain yang tidak disembunyikan, tanpa membedakan huruf
/// besar-kecil. Kunci tabelnya sama dengan [`create`], sehingga rename dan create tidak bisa
/// sama-sama mengambil satu nama.
///
/// Mengembalikan hasil berupa [`RenameResult`]: berhasil beserta data terbaru, tidak ditemukan,
/// atau nama sudah dipakai.
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

/// Menyembunyikan perusahaan dengan mengisi `deleted_at` dengan waktu sekarang dalam WIB.
///
/// Mengembalikan data perusahaan setelah disembunyikan, atau `None` bila perusahaan tidak ada atau
/// sudah disembunyikan sebelumnya.
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
