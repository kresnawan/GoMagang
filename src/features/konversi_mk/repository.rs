use sqlx::{PgConnection, PgPool};

use super::model::{HapusResult, MataKuliah, Penyetaraan, TambahResult};

/// Daftar mata kuliah, diurutkan berdasarkan nama MK konversi lalu MK asal.
///
/// Mata kuliah yang disembunyikan tidak disertakan kecuali `termasuk_dihapus` bernilai `true`, dan
/// dalam hal itu ditaruh di akhir daftar. `deleted_at` dikirim dalam WIB dengan offset `+07:00`.
pub async fn list_mk(db: &PgPool, termasuk_dihapus: bool) -> Result<Vec<MataKuliah>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id_mk_konversi, mk_asal, mk_konversi, sks,
                to_char(deleted_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS deleted_at
         FROM mata_kuliah_konversi
         WHERE $1 OR deleted_at IS NULL
         ORDER BY deleted_at IS NOT NULL, mk_konversi, mk_asal, id_mk_konversi",
    )
    .bind(termasuk_dihapus)
    .fetch_all(db)
    .await
}

/// Menyimpan mata kuliah baru.
///
/// Mengembalikan `None` bila mata kuliah yang tidak disembunyikan sudah memiliki pasangan MK asal
/// dan
/// MK konversi yang sama, tanpa membedakan huruf besar-kecil. Tabel dikunci selama transaksi; tanpa
/// kunci ini, dua request dengan pasangan yang sama bisa sama-sama tersimpan.
pub async fn create_mk(
    db: &PgPool,
    mk_asal: &str,
    mk_konversi: &str,
    sks: i32,
) -> Result<Option<MataKuliah>, sqlx::Error> {
    let mut tx = db.begin().await?;

    // Without the lock, two requests with the same pair could both find it free and both be saved.
    sqlx::query("LOCK TABLE mata_kuliah_konversi IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;

    let mata_kuliah = sqlx::query_as(
        "INSERT INTO mata_kuliah_konversi (mk_asal, mk_konversi, sks)
         SELECT $1::varchar, $2::varchar, $3
         WHERE NOT EXISTS (
             SELECT 1 FROM mata_kuliah_konversi
             WHERE lower(mk_asal) = lower($1)
               AND lower(mk_konversi) = lower($2)
               AND deleted_at IS NULL
         )
         RETURNING id_mk_konversi, mk_asal, mk_konversi, sks, NULL::text AS deleted_at",
    )
    .bind(mk_asal)
    .bind(mk_konversi)
    .bind(sks)
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(mata_kuliah)
}

/// Menyembunyikan mata kuliah dengan mengisi `deleted_at` dengan waktu sekarang dalam WIB.
///
/// Mengembalikan mata kuliah setelah disembunyikan, atau `None` bila tidak ada atau sudah
/// disembunyikan.
pub async fn soft_delete_mk(
    db: &PgPool,
    id_mk_konversi: i32,
) -> Result<Option<MataKuliah>, sqlx::Error> {
    sqlx::query_as(
        "UPDATE mata_kuliah_konversi
         SET deleted_at = clock_timestamp() AT TIME ZONE 'Asia/Jakarta'
         WHERE id_mk_konversi = $1 AND deleted_at IS NULL
         RETURNING id_mk_konversi, mk_asal, mk_konversi, sks,
                   to_char(deleted_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS deleted_at",
    )
    .bind(id_mk_konversi)
    .fetch_optional(db)
    .await
}

/// `id_pengguna` mahasiswa pemilik pendaftaran, atau `None` bila pendaftarannya tidak ada.
pub async fn find_pemilik(db: &PgPool, id_pendaftaran: i32) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar("SELECT id_pengguna FROM pendaftaran WHERE id_pendaftaran = $1")
        .bind(id_pendaftaran)
        .fetch_optional(db)
        .await
}

/// The SELECT shared by the penyetaraan queries, as a literal so that it can be joined with
/// `concat!`.
macro_rules! select_penyetaraan {
    () => {
        "SELECT py.id_penyetaraan, py.id_pendaftaran,
        to_char(py.created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS created_at,
        mk.id_mk_konversi, mk.mk_asal, mk.mk_konversi, mk.sks,
        to_char(mk.deleted_at, 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00') AS deleted_at
 FROM penyetaraan_mk py
 JOIN mata_kuliah_konversi mk ON mk.id_mk_konversi = py.id_mk_konversi "
    };
}

/// Mata kuliah yang dipilih untuk sebuah pendaftaran, sesuai urutan waktu dipilih.
///
/// Mata kuliah yang sudah disembunyikan setelah dipilih tetap ada di daftar, dengan `deleted_at`
/// terisi.
pub async fn list_penyetaraan(
    db: &PgPool,
    id_pendaftaran: i32,
) -> Result<Vec<Penyetaraan>, sqlx::Error> {
    sqlx::query_as(concat!(
        select_penyetaraan!(),
        "WHERE py.id_pendaftaran = $1
         ORDER BY py.created_at, py.id_penyetaraan"
    ))
    .bind(id_pendaftaran)
    .fetch_all(db)
    .await
}

/// Mengunci baris pendaftaran, lalu mengembalikan pemiliknya dan apakah statusnya `Aktif`.
///
/// Kunci `FOR UPDATE` menahan perubahan lain pada pendaftaran ini (misalnya Panitia Magang mengubah
/// statusnya) sampai transaksi selesai, sehingga status yang dicek tetap berlaku selama penyetaraan
/// diubah. Penguncian dan pembacaan status sengaja dipisah menjadi dua statement, seperti di fitur
/// pendaftaran. `FOR UPDATE` pada join dengan `status_pendaftaran` bisa kehilangan baris yang
/// statusnya berubah saat menunggu kunci.
///
/// Mengembalikan `None` bila pendaftaran tidak ada.
async fn lock_pendaftaran(
    conn: &mut PgConnection,
    id_pendaftaran: i32,
) -> Result<Option<(i32, bool)>, sqlx::Error> {
    let id_pengguna: Option<i32> = sqlx::query_scalar(
        "SELECT id_pengguna FROM pendaftaran WHERE id_pendaftaran = $1 FOR UPDATE",
    )
    .bind(id_pendaftaran)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(id_pengguna) = id_pengguna else {
        return Ok(None);
    };

    let aktif: bool = sqlx::query_scalar(
        "SELECT s.status = 'Aktif'
         FROM pendaftaran p
         JOIN status_pendaftaran s ON s.id_status = p.id_status
         WHERE p.id_pendaftaran = $1",
    )
    .bind(id_pendaftaran)
    .fetch_one(&mut *conn)
    .await?;

    Ok(Some((id_pengguna, aktif)))
}

/// Memilihkan mata kuliah untuk pendaftaran milik mahasiswa ini yang berstatus `Aktif`.
///
/// Dalam satu transaksi:
/// 1. Pendaftaran dikunci dan diperiksa: ada, milik `id_pengguna`, dan `Aktif`.
/// 2. Mata kuliah diperiksa masih ada dan tidak disembunyikan, dengan kunci `FOR SHARE` agar tidak
///    bisa disembunyikan sebelum transaksi selesai.
/// 3. Penyetaraan disimpan. Pilihan ganda ditolak oleh constraint unik
///    `(id_pendaftaran, id_mk_konversi)`.
///
/// Mengembalikan hasil berupa [`TambahResult`].
pub async fn tambah_penyetaraan(
    db: &PgPool,
    id_pendaftaran: i32,
    id_pengguna: i32,
    id_mk_konversi: i32,
) -> Result<TambahResult, sqlx::Error> {
    let mut tx = db.begin().await?;

    match lock_pendaftaran(&mut tx, id_pendaftaran).await? {
        None => return Ok(TambahResult::PendaftaranNotFound),
        Some((pemilik, _)) if pemilik != id_pengguna => return Ok(TambahResult::NotOwner),
        Some((_, false)) => return Ok(TambahResult::NotAktif),
        Some(_) => {}
    }

    // FOR SHARE keeps the mata kuliah from being hidden before this transaction ends.
    let ada: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM mata_kuliah_konversi
         WHERE id_mk_konversi = $1 AND deleted_at IS NULL
         FOR SHARE",
    )
    .bind(id_mk_konversi)
    .fetch_optional(&mut *tx)
    .await?;
    if ada.is_none() {
        return Ok(TambahResult::MataKuliahNotFound);
    }

    let id_penyetaraan: Option<i32> = sqlx::query_scalar(
        "INSERT INTO penyetaraan_mk (id_mk_konversi, id_pendaftaran, created_at)
         VALUES ($1, $2, now() AT TIME ZONE 'Asia/Jakarta')
         ON CONFLICT (id_pendaftaran, id_mk_konversi) DO NOTHING
         RETURNING id_penyetaraan",
    )
    .bind(id_mk_konversi)
    .bind(id_pendaftaran)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(id_penyetaraan) = id_penyetaraan else {
        return Ok(TambahResult::SudahAda);
    };

    let penyetaraan = sqlx::query_as(concat!(
        select_penyetaraan!(),
        "WHERE py.id_penyetaraan = $1"
    ))
    .bind(id_penyetaraan)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(TambahResult::Ditambahkan(penyetaraan))
}

/// Menghapus pilihan mata kuliah dari pendaftaran milik mahasiswa ini yang berstatus `Aktif`.
///
/// Pendaftaran pemilik penyetaraan dikunci dulu, sehingga statusnya tidak bisa berubah di tengah
/// proses. Mengembalikan hasil berupa [`HapusResult`]: terhapus, tidak ditemukan, bukan pemilik,
/// atau pendaftaran tidak `Aktif`.
pub async fn hapus_penyetaraan(
    db: &PgPool,
    id_penyetaraan: i32,
    id_pengguna: i32,
) -> Result<HapusResult, sqlx::Error> {
    let mut tx = db.begin().await?;

    let id_pendaftaran: Option<i32> =
        sqlx::query_scalar("SELECT id_pendaftaran FROM penyetaraan_mk WHERE id_penyetaraan = $1")
            .bind(id_penyetaraan)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(id_pendaftaran) = id_pendaftaran else {
        return Ok(HapusResult::NotFound);
    };

    match lock_pendaftaran(&mut tx, id_pendaftaran).await? {
        None => return Ok(HapusResult::NotFound),
        Some((pemilik, _)) if pemilik != id_pengguna => return Ok(HapusResult::NotOwner),
        Some((_, false)) => return Ok(HapusResult::NotAktif),
        Some(_) => {}
    }

    let dihapus = sqlx::query("DELETE FROM penyetaraan_mk WHERE id_penyetaraan = $1")
        .bind(id_penyetaraan)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    tx.commit().await?;

    Ok(if dihapus == 0 {
        HapusResult::NotFound
    } else {
        HapusResult::Dihapus
    })
}
