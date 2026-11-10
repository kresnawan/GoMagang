use sqlx::PgPool;

use super::model::{CreatePosisi, PosisiMagang, PosisiQuery, UbahPosisi};

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

/// The positions of a period, by name, leaving out those of hidden companies. Hidden positions are
/// left out too, unless asked for, and then they come last.
///
/// deleted_at has no time zone and is written in WIB, so it is sent out with the +07:00 offset.
pub async fn list(
    db: &PgPool,
    id_periode: i32,
    query: &PosisiQuery,
    termasuk_dihapus: bool,
) -> Result<Vec<PosisiMagang>, sqlx::Error> {
    let search = query
        .s
        .as_deref()
        .map(str::trim)
        .filter(|search| !search.is_empty());

    // strpos instead of LIKE, so that % and _ in the search are plain characters.
    sqlx::query_as(
        "SELECT pm.id_posisi, pm.posisi, pm.deskripsi_persyaratan, pm.kuota, pm.id_periode,
                pr.id_perusahaan, pr.nama_perusahaan,
                to_char(pm.deleted_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS deleted_at
         FROM posisi_magang pm
         JOIN perusahaan_mitra pr ON pr.id_perusahaan = pm.id_perusahaan
         WHERE pm.id_periode = $1
           AND pr.deleted_at IS NULL
           AND ($4 OR pm.deleted_at IS NULL)
           AND ($2::int IS NULL OR pm.id_perusahaan = $2)
           AND ($3::text IS NULL OR strpos(lower(pm.posisi), lower($3)) > 0)
         ORDER BY pm.deleted_at IS NOT NULL, pm.posisi, pm.id_posisi",
    )
    .bind(id_periode)
    .bind(query.p)
    .bind(search)
    .bind(termasuk_dihapus)
    .fetch_all(db)
    .await
}

/// Saves a position and returns it with its company. Returns `None` if the company does not
/// exist or is hidden.
pub async fn create(
    db: &PgPool,
    posisi: &CreatePosisi,
) -> Result<Option<PosisiMagang>, sqlx::Error> {
    sqlx::query_as(
        "WITH baru AS (
             INSERT INTO posisi_magang
                 (id_perusahaan, posisi, deskripsi_persyaratan, kuota, id_periode)
             SELECT pr.id_perusahaan, $2, $3, $4, $5
             FROM perusahaan_mitra pr
             WHERE pr.id_perusahaan = $1 AND pr.deleted_at IS NULL
             RETURNING *
         )
         SELECT baru.id_posisi, baru.posisi, baru.deskripsi_persyaratan, baru.kuota,
                baru.id_periode, pr.id_perusahaan, pr.nama_perusahaan, NULL::text AS deleted_at
         FROM baru
         JOIN perusahaan_mitra pr ON pr.id_perusahaan = baru.id_perusahaan",
    )
    .bind(posisi.id_perusahaan)
    .bind(&posisi.posisi)
    .bind(&posisi.deskripsi_persyaratan)
    .bind(posisi.kuota)
    .bind(posisi.id_periode)
    .fetch_optional(db)
    .await
}

/// Changes the name, description and quota of a position. Returns `None` if the position does not
/// exist or is hidden, or its company is hidden.
pub async fn ubah(
    db: &PgPool,
    id_posisi: i32,
    data: &UbahPosisi,
) -> Result<Option<PosisiMagang>, sqlx::Error> {
    sqlx::query_as(
        "UPDATE posisi_magang pm
         SET posisi = $2, deskripsi_persyaratan = $3, kuota = $4
         FROM perusahaan_mitra pr
         WHERE pm.id_posisi = $1
           AND pm.deleted_at IS NULL
           AND pr.id_perusahaan = pm.id_perusahaan
           AND pr.deleted_at IS NULL
         RETURNING pm.id_posisi, pm.posisi, pm.deskripsi_persyaratan, pm.kuota, pm.id_periode,
                   pr.id_perusahaan, pr.nama_perusahaan, NULL::text AS deleted_at",
    )
    .bind(id_posisi)
    .bind(&data.posisi)
    .bind(&data.deskripsi_persyaratan)
    .bind(data.kuota)
    .fetch_optional(db)
    .await
}

/// Hides a position. Returns `None` if the position does not exist or is hidden already, or its
/// company is hidden.
pub async fn sembunyikan(db: &PgPool, id_posisi: i32) -> Result<Option<PosisiMagang>, sqlx::Error> {
    sqlx::query_as(
        "UPDATE posisi_magang pm
         SET deleted_at = clock_timestamp() AT TIME ZONE 'Asia/Jakarta'
         FROM perusahaan_mitra pr
         WHERE pm.id_posisi = $1
           AND pm.deleted_at IS NULL
           AND pr.id_perusahaan = pm.id_perusahaan
           AND pr.deleted_at IS NULL
         RETURNING pm.id_posisi, pm.posisi, pm.deskripsi_persyaratan, pm.kuota, pm.id_periode,
                   pr.id_perusahaan, pr.nama_perusahaan,
                   to_char(pm.deleted_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS deleted_at",
    )
    .bind(id_posisi)
    .fetch_optional(db)
    .await
}
