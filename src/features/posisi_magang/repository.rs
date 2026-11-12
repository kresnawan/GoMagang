use sqlx::PgPool;

use super::model::{CreatePosisi, PosisiMagang, PosisiQuery, UbahPosisi};

// waktu_mulai and waktu_tutup have no time zone and are written in WIB, so they are compared
// against the time in Jakarta instead of the database server's own time zone.

/// Id periode yang sedang berlangsung saat ini, dari waktu mulai yang paling baru.
///
/// Waktu sekarang dibandingkan dalam zona Asia/Jakarta karena `waktu_mulai` dan `waktu_tutup`
/// disimpan dalam WIB tanpa zona waktu.
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

/// Periode bawaan untuk Panitia Magang: periode berlangsung yang paling baru, atau bila tidak ada,
/// periode dengan waktu mulai paling baru.
///
/// Mengembalikan `None` bila belum ada periode sama sekali.
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

/// Daftar posisi dalam satu periode, diurutkan berdasarkan nama posisi.
///
/// - Posisi milik perusahaan yang disembunyikan selalu tidak disertakan.
/// - Posisi yang disembunyikan hanya disertakan bila `termasuk_dihapus` bernilai `true`, dan
///   ditaruh di akhir daftar.
/// - `query.p` memfilter perusahaan, dan `query.s` mencari nama posisi tanpa membedakan huruf
///   besar-kecil. Pencarian memakai `strpos`, bukan `LIKE`, agar karakter `%` dan `_` dianggap
///   karakter biasa.
///
/// `deleted_at` dikirim dalam WIB dengan offset `+07:00`.
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

/// Menyimpan posisi baru dan mengembalikannya beserta data perusahaan.
///
/// Mengembalikan `None` bila perusahaan tidak ada atau sudah disembunyikan. Periode yang tidak ada
/// membuat query gagal dengan pelanggaran foreign key `posisi_magang_id_periode_fkey`, yang
/// diterjemahkan oleh handler.
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

/// Mengubah nama, deskripsi persyaratan, dan kuota posisi.
///
/// Mengembalikan posisi setelah diubah, atau `None` bila posisi tidak ada, sudah disembunyikan,
/// atau
/// perusahaannya disembunyikan.
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

/// Menyembunyikan posisi dengan mengisi `deleted_at` dengan waktu sekarang dalam WIB.
///
/// Mengembalikan posisi setelah disembunyikan, atau `None` bila posisi tidak ada, sudah
/// disembunyikan, atau perusahaannya disembunyikan.
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
