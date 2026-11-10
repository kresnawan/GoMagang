use rand::seq::SliceRandom;
use sqlx::{PgConnection, PgPool};

use super::{
    dokumen,
    model::{BerkasSeed, JUMLAH_SEED, MahasiswaAcak, PILIHAN_JUMLAH_TAMBAHAN, Terhapus},
};
use crate::features::pendaftaran::JENIS_BERKAS_TAMBAHAN;

/// Everything hanging off a pendaftaran, except the tables whose file names are needed:
/// berkas_pendaftaran and surat_keterangan.
const DELETE_ANAK_PENDAFTARAN: [&str; 4] = [
    "DELETE FROM penyetaraan_mk WHERE id_pendaftaran = ANY($1)",
    "DELETE FROM log_status WHERE id_pendaftaran = ANY($1)",
    "DELETE FROM jadwal_seleksi_substansi WHERE id_pendaftaran = ANY($1)",
    "DELETE FROM plotting_dpm WHERE id_pendaftaran = ANY($1)",
];

/// Deletes every mahasiswa and all that belongs to them. Tim MB accounts and the positions, with
/// their quota, are not touched.
pub async fn hapus_semua(db: &PgPool) -> Result<Terhapus, sqlx::Error> {
    let mut tx = db.begin().await?;

    // Holds back the registrations and accepts of these mahasiswa until the transaction ends.
    let id_pengguna: Vec<i32> =
        sqlx::query_scalar("SELECT id_pengguna FROM mahasiswa ORDER BY id_pengguna FOR UPDATE")
            .fetch_all(&mut *tx)
            .await?;

    // Holds back Tim MB's changes, whose log rows would otherwise stop these from going.
    let id_pendaftaran: Vec<i32> = sqlx::query_scalar(
        "SELECT id_pendaftaran
         FROM pendaftaran
         WHERE id_pengguna = ANY($1)
         ORDER BY id_pendaftaran
         FOR UPDATE",
    )
    .bind(&id_pengguna)
    .fetch_all(&mut *tx)
    .await?;

    for sql in DELETE_ANAK_PENDAFTARAN {
        sqlx::query(sql)
            .bind(&id_pendaftaran)
            .execute(&mut *tx)
            .await?;
    }

    let mut berkas: Vec<String> = Vec::new();
    for sql in [
        "DELETE FROM berkas_pendaftaran WHERE id_pendaftaran = ANY($1) RETURNING nama_berkas",
        "DELETE FROM surat_keterangan WHERE id_pendaftaran = ANY($1) RETURNING nama_berkas",
    ] {
        berkas.extend(
            sqlx::query_scalar::<_, String>(sql)
                .bind(&id_pendaftaran)
                .fetch_all(&mut *tx)
                .await?,
        );
    }

    sqlx::query("DELETE FROM pendaftaran WHERE id_pendaftaran = ANY($1)")
        .bind(&id_pendaftaran)
        .execute(&mut *tx)
        .await?;

    // token_refresh goes along with pengguna, through its ON DELETE CASCADE.
    for sql in [
        "DELETE FROM mahasiswa WHERE id_pengguna = ANY($1)",
        "DELETE FROM pengguna WHERE id_pengguna = ANY($1)",
    ] {
        sqlx::query(sql)
            .bind(&id_pengguna)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    Ok(Terhapus {
        jumlah_mahasiswa: id_pengguna.len(),
        jumlah_pendaftaran: id_pendaftaran.len(),
        berkas,
    })
}

/// Adds made-up mahasiswa, each with one pendaftaran at a position of the period open right now.
/// The pendaftaran are spread evenly over all of its positions, in random order. Returns the files
/// whose rows were saved, for the caller to write before committing, or nothing if no position is
/// in a running period.
///
/// Every pendaftaran has a CV and a Surat Pernyataan DPA, and some also have up to three extra
/// files. The quota is information only, so a position may get more pendaftaran than its quota.
pub async fn seed(conn: &mut PgConnection) -> Result<Option<Vec<BerkasSeed>>, sqlx::Error> {
    // The positions open to register, as in POST /pendaftaran: not hidden, of a company that is not
    // hidden. The time columns have no time zone and are written in WIB.
    let id_posisi: Vec<i32> = sqlx::query_scalar(
        "SELECT pm.id_posisi
         FROM posisi_magang pm
         JOIN periode_pendaftaran pp ON pp.id_periode = pm.id_periode
         JOIN perusahaan_mitra pr ON pr.id_perusahaan = pm.id_perusahaan
         WHERE now() AT TIME ZONE 'Asia/Jakarta' BETWEEN pp.waktu_mulai AND pp.waktu_tutup
           AND pm.deleted_at IS NULL
           AND pr.deleted_at IS NULL",
    )
    .fetch_all(&mut *conn)
    .await?;

    if id_posisi.is_empty() {
        return Ok(None);
    }

    let jumlah_tambahan = bagi_rata(&PILIHAN_JUMLAH_TAMBAHAN, JUMLAH_SEED);
    let mut semua_berkas = Vec::new();

    for (id_posisi, jumlah_tambahan) in bagi_rata(&id_posisi, JUMLAH_SEED)
        .into_iter()
        .zip(jumlah_tambahan)
    {
        let mahasiswa = MahasiswaAcak::new();

        let (id_pengguna, id_pendaftaran): (i32, i32) = sqlx::query_as(
            "WITH pengguna_baru AS (
                 INSERT INTO pengguna (email) VALUES ($1) RETURNING id_pengguna
             ), mahasiswa_baru AS (
                 INSERT INTO mahasiswa
                     (id_pengguna, nim, nama, program_studi, ipk, no_hp, jenis_kelamin, semester)
                 SELECT id_pengguna, $2, $3, $4, $5::float8::numeric, $6, $7, $8
                 FROM pengguna_baru
             ), pendaftaran_baru AS (
                 INSERT INTO pendaftaran (id_posisi, id_status, id_pengguna, created_at, updated_at)
                 SELECT $9, s.id_status, pb.id_pengguna, now() AT TIME ZONE 'Asia/Jakarta',
                        now() AT TIME ZONE 'Asia/Jakarta'
                 FROM pengguna_baru pb, status_pendaftaran s
                 WHERE s.status = 'Menunggu Verifikasi'
                 RETURNING id_pendaftaran, id_status, id_pengguna, created_at
             ), log AS (
                 INSERT INTO log_status (id_pendaftaran, status_baru, created_at)
                 SELECT id_pendaftaran, id_status, created_at FROM pendaftaran_baru
             )
             SELECT id_pengguna, id_pendaftaran FROM pendaftaran_baru",
        )
        .bind(&mahasiswa.email)
        .bind(&mahasiswa.nim)
        .bind(&mahasiswa.nama)
        .bind(mahasiswa.program_studi)
        .bind(mahasiswa.ipk)
        .bind(&mahasiswa.no_hp)
        .bind(mahasiswa.jenis_kelamin)
        .bind(mahasiswa.semester)
        .bind(id_posisi)
        .fetch_one(&mut *conn)
        .await?;

        // Same kinds and the same naming as files uploaded through POST /pendaftaran.
        let mut berkas = vec![
            (
                "Surat Pernyataan DPA",
                "surat-dpa",
                dokumen::surat_dpa(&mahasiswa),
            ),
            ("CV", "cv", dokumen::cv(&mahasiswa)),
        ];
        berkas.extend(
            dokumen::tambahan(&mahasiswa, jumlah_tambahan)
                .into_iter()
                .map(|isi| (JENIS_BERKAS_TAMBAHAN, "berkas-tambahan", isi)),
        );

        let mut jenis = Vec::with_capacity(berkas.len());
        let mut nama = Vec::with_capacity(berkas.len());
        let mut ukuran = Vec::with_capacity(berkas.len());
        for (jenis_berkas, slug, isi) in berkas {
            let nama_berkas = format!("{id_pengguna}_{slug}_{:032x}.pdf", rand::random::<u128>());
            jenis.push(jenis_berkas);
            nama.push(nama_berkas.clone());
            ukuran.push(isi.len() as i32);
            semua_berkas.push(BerkasSeed {
                nama: nama_berkas,
                isi,
            });
        }

        // Inserted in the order given, so that the files keep it.
        sqlx::query(
            "INSERT INTO berkas_pendaftaran
                 (id_pendaftaran, jenis_berkas, nama_berkas, ukuran_berkas)
             SELECT $1, berkas.jenis, berkas.nama, berkas.ukuran
             FROM unnest($2::text[], $3::text[], $4::int[])
                  WITH ORDINALITY AS berkas (jenis, nama, ukuran, urutan)
             ORDER BY berkas.urutan",
        )
        .bind(id_pendaftaran)
        .bind(&jenis)
        .bind(&nama)
        .bind(&ukuran)
        .execute(&mut *conn)
        .await?;
    }

    Ok(Some(semua_berkas))
}

/// One of `pilihan` for each of `jumlah` pendaftaran, so that every one is used the same number
/// of times, or once more, in random order. Which ones are used once more is random too.
fn bagi_rata<T: Copy>(pilihan: &[T], jumlah: usize) -> Vec<T> {
    let mut pilihan = pilihan.to_vec();
    pilihan.shuffle(&mut rand::rng());
    let mut hasil: Vec<T> = pilihan.into_iter().cycle().take(jumlah).collect();
    hasil.shuffle(&mut rand::rng());
    hasil
}

#[cfg(test)]
mod tests {
    use super::bagi_rata;

    #[test]
    fn spreads_the_pendaftaran_evenly_over_the_positions() {
        let hasil = bagi_rata(&[1, 2, 3, 4, 5, 6], 200);

        assert_eq!(hasil.len(), 200);
        for id in 1..=6 {
            let jumlah = hasil.iter().filter(|&&posisi| posisi == id).count();
            assert!(jumlah == 33 || jumlah == 34, "{id}: {jumlah}");
        }
    }

    #[test]
    fn works_with_one_position_or_more_positions_than_pendaftaran() {
        assert_eq!(bagi_rata(&[7], 3), [7, 7, 7]);

        let mut hasil = bagi_rata(&[1, 2, 3, 4], 2);
        hasil.sort_unstable();
        hasil.dedup();
        assert_eq!(hasil.len(), 2);
    }
}
