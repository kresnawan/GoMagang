use sqlx::{PgConnection, PgExecutor, PgPool};
use time::Date;

use super::model::{DataPendaftaran, SuratBaru, SuratTersimpan};

pub async fn find_pendaftaran<'e>(
    db: impl PgExecutor<'e>,
    id_pendaftaran: i32,
) -> Result<Option<DataPendaftaran>, sqlx::Error> {
    sqlx::query_as(
        "SELECT p.id_pengguna, s.status, m.nama, m.nim::text AS nim, m.program_studi,
                pm.posisi, pr.nama_perusahaan AS perusahaan
         FROM pendaftaran p
         JOIN status_pendaftaran s ON s.id_status = p.id_status
         JOIN mahasiswa m ON m.id_pengguna = p.id_pengguna
         JOIN posisi_magang pm ON pm.id_posisi = p.id_posisi
         JOIN perusahaan_mitra pr ON pr.id_perusahaan = pm.id_perusahaan
         WHERE p.id_pendaftaran = $1",
    )
    .bind(id_pendaftaran)
    .fetch_optional(db)
    .await
}

/// Holds back status changes and other first downloads of this pendaftaran until the transaction
/// ends, so that a surat is issued once, and only while the pendaftaran is Aktif.
pub async fn lock_pendaftaran(
    conn: &mut PgConnection,
    id_pendaftaran: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1 FROM pendaftaran WHERE id_pendaftaran = $1 FOR UPDATE")
        .bind(id_pendaftaran)
        .execute(conn)
        .await?;
    Ok(())
}

const KOLOM_SURAT: &str = "SELECT id_surat, nama_berkas, isi, tanda_tangan FROM surat_keterangan";

pub async fn find_by_pendaftaran<'e>(
    db: impl PgExecutor<'e>,
    id_pendaftaran: i32,
) -> Result<Option<SuratTersimpan>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{KOLOM_SURAT} WHERE id_pendaftaran = $1"
    )))
    .bind(id_pendaftaran)
    .fetch_optional(db)
    .await
}

pub async fn find_by_id(db: &PgPool, id_surat: i32) -> Result<Option<SuratTersimpan>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{KOLOM_SURAT} WHERE id_surat = $1"
    )))
    .bind(id_surat)
    .fetch_optional(db)
    .await
}

pub async fn find_by_hash(db: &PgPool, hash: &str) -> Result<Option<SuratTersimpan>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{KOLOM_SURAT} WHERE hash_berkas = $1"
    )))
    .bind(hash)
    .fetch_optional(db)
    .await
}

/// Today in WIB, the date printed on the surat.
pub async fn hari_ini(conn: &mut PgConnection) -> Result<Date, sqlx::Error> {
    sqlx::query_scalar("SELECT (now() AT TIME ZONE 'Asia/Jakarta')::date")
        .fetch_one(conn)
        .await
}

/// Takes the next id ahead of the insert, because the number and the QR printed in the file
/// need it before the row can be written.
pub async fn next_id_surat(conn: &mut PgConnection) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT nextval(pg_get_serial_sequence('surat_keterangan', 'id_surat'))::int",
    )
    .fetch_one(conn)
    .await
}

pub async fn insert(conn: &mut PgConnection, surat: &SuratBaru) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO surat_keterangan
             (id_surat, id_pendaftaran, nama_berkas, ukuran_berkas, nomor_surat, isi,
              tanda_tangan, hash_berkas)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(surat.id_surat)
    .bind(surat.id_pendaftaran)
    .bind(&surat.nama_berkas)
    .bind(surat.ukuran_berkas)
    .bind(&surat.nomor_surat)
    .bind(&surat.isi)
    .bind(&surat.tanda_tangan)
    .bind(&surat.hash_berkas)
    .execute(conn)
    .await?;
    Ok(())
}
