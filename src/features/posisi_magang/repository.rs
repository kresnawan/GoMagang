use sqlx::PgPool;

use super::model::{CreatePosisi, PosisiMagang, PosisiQuery};

// waktu_mulai and waktu_tutup have no time zone and are written in WIB, so they are compared
// against the time in Jakarta instead of the database server's own time zone.

/// The periods running right now, newest first.
pub async fn running_periode_ids(db: &PgPool) -> Result<Vec<i32>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id_periode
         FROM periode_pendaftaran
         WHERE now() AT TIME ZONE 'Asia/Jakarta' BETWEEN waktu_mulai AND waktu_tutup
         ORDER BY waktu_mulai DESC, id_periode DESC",
    )
    .fetch_all(db)
    .await
}

/// The newest period running right now, or else the newest period there is.
pub async fn default_periode_id(db: &PgPool) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id_periode
         FROM periode_pendaftaran
         ORDER BY now() AT TIME ZONE 'Asia/Jakarta' BETWEEN waktu_mulai AND waktu_tutup DESC,
                  waktu_mulai DESC, id_periode DESC
         LIMIT 1",
    )
    .fetch_optional(db)
    .await
}

pub async fn list(
    db: &PgPool,
    id_periode: i32,
    query: &PosisiQuery,
) -> Result<Vec<PosisiMagang>, sqlx::Error> {
    let search = query
        .s
        .as_deref()
        .map(str::trim)
        .filter(|search| !search.is_empty());

    // strpos instead of LIKE, so that % and _ in the search are plain characters.
    sqlx::query_as(
        "SELECT pm.id_posisi, pm.posisi, pm.deskripsi_persyaratan, pm.kuota, pm.id_periode,
                pr.id_perusahaan, pr.nama_perusahaan
         FROM posisi_magang pm
         JOIN perusahaan_mitra pr ON pr.id_perusahaan = pm.id_perusahaan
         WHERE pm.id_periode = $1
           AND ($2::int IS NULL OR pm.id_perusahaan = $2)
           AND ($3::text IS NULL OR strpos(lower(pm.posisi), lower($3)) > 0)
         ORDER BY pm.posisi, pm.id_posisi",
    )
    .bind(id_periode)
    .bind(query.p)
    .bind(search)
    .fetch_all(db)
    .await
}

/// Saves a position and returns it with its company.
pub async fn create(db: &PgPool, posisi: &CreatePosisi) -> Result<PosisiMagang, sqlx::Error> {
    sqlx::query_as(
        "WITH baru AS (
             INSERT INTO posisi_magang
                 (id_perusahaan, posisi, deskripsi_persyaratan, kuota, id_periode)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING *
         )
         SELECT baru.id_posisi, baru.posisi, baru.deskripsi_persyaratan, baru.kuota,
                baru.id_periode, pr.id_perusahaan, pr.nama_perusahaan
         FROM baru
         JOIN perusahaan_mitra pr ON pr.id_perusahaan = baru.id_perusahaan",
    )
    .bind(posisi.id_perusahaan)
    .bind(&posisi.posisi)
    .bind(&posisi.deskripsi_persyaratan)
    .bind(posisi.kuota)
    .bind(posisi.id_periode)
    .fetch_one(db)
    .await
}
