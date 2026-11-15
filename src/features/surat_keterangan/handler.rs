use std::{io, sync::Arc};

use axum::{
    Json,
    extract::{Multipart, Path, Query, State, multipart::MultipartError},
    http::{StatusCode, header},
    response::IntoResponse,
};
use sha2::{Digest, Sha256};
use tokio::fs;

use super::{
    dokumen,
    model::{
        DataPendaftaran, IsiSurat, KodeQuery, KunciPublik, MAX_BERKAS_VERIFIKASI, SuratBaru,
        SuratTersimpan, VERSI_ISI,
    },
    repository,
    tanda_tangan::{PenandaSurat, kode, parse_kode},
    templat,
};
use crate::{
    error::AppError,
    features::auth::{Peran, User, find_peran},
    state::AppState,
};

const STATUS_AKTIF: &str = "Aktif";

/// Mengambil penanda tangan surat dari state.
///
/// Tanpa `SURAT_SIGNING_KEY`, tidak ada surat yang bisa ditandatangani atau diperiksa, sehingga
/// semua endpoint surat keterangan dijawab [`AppError::ServiceUnavailable`] (`503`).
fn penanda(state: &AppState) -> Result<Arc<PenandaSurat>, AppError> {
    state
        .penanda_surat
        .clone()
        .ok_or(AppError::ServiceUnavailable)
}

/// `GET /api/v1/pendaftaran/{id}/surat-keterangan`: mengunduh surat keterangan sebuah pendaftaran.
///
/// Bisa diunduh oleh mahasiswa pemilik pendaftaran (milik orang lain dijawab `403`) dan oleh
/// Panitia
/// Magang. Unduhan pertama untuk pendaftaran `Aktif` menerbitkan surat (lihat [`terbitkan`]).
/// Unduhan
/// berikutnya selalu mengirim berkas yang sama persis, sehingga hash-nya tetap cocok saat
/// diverifikasi.
///
/// Respons berupa PDF dengan nama berkas `surat-keterangan-magang-<nim>.pdf`, dan header
/// `Cache-Control: private, no-store` agar tidak ada cache yang menyimpan salinannya. Pendaftaran
/// yang tidak ada dijawab `404`, dan yang tidak `Aktif` saat surat belum terbit dijawab `409`.
pub async fn unduh(
    State(state): State<AppState>,
    user: User,
    Path(id_pendaftaran): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let penanda = penanda(&state)?;
    let peran = find_peran(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;

    let pendaftaran = repository::find_pendaftaran(&state.db, id_pendaftaran)
        .await?
        .ok_or(AppError::NotFound)?;
    if matches!(peran, Peran::Mahasiswa) && pendaftaran.id_pengguna != user.id_pengguna {
        return Err(AppError::Forbidden);
    }

    let surat = match repository::find_by_pendaftaran(&state.db, id_pendaftaran).await? {
        Some(surat) => surat,
        None => terbitkan(&state, &penanda, id_pendaftaran).await?,
    };

    let isi_berkas = fs::read(state.upload_dir.join(&surat.nama_berkas))
        .await
        .map_err(|err| {
            tracing::error!(error = %err, id_surat = surat.id_surat, "failed to read surat");
            match err.kind() {
                io::ErrorKind::NotFound => AppError::NotFound,
                _ => AppError::Internal,
            }
        })?;
    let nim = baca_isi(&surat)?.nim;

    let headers = [
        (header::CONTENT_TYPE, "application/pdf".to_string()),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"surat-keterangan-magang-{nim}.pdf\""),
        ),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        // Private to its owner and Panitia Magang, so no cache may keep a copy.
        (header::CACHE_CONTROL, "private, no-store".to_string()),
    ];

    Ok((headers, isi_berkas))
}

/// Menyusun, menandatangani, dan menyimpan surat keterangan baru.
///
/// Dalam satu transaksi dengan baris pendaftaran dikunci:
/// 1. Bila surat ternyata sudah diterbitkan oleh request lain yang memegang kunci lebih dulu, surat
///    itu yang dikembalikan. Dengan begitu, dua unduhan pertama yang bersamaan tetap menghasilkan
///    satu surat.
/// 2. Status harus `Aktif`, dan statusnya tidak bisa berubah selama proses karena terkunci.
/// 3. Tanggal surat (WIB) dan id surat diambil lebih dulu, karena keduanya dicetak di nomor surat
///    dan
///    QR code.
/// 4. Isi surat disusun lalu dibekukan sebagai JSON, ditandatangani, dan di-render menjadi PDF
///    dengan QR code berisi tautan verifikasi.
/// 5. Baris disimpan bersama hash SHA-256 berkasnya, lalu berkas ditulis ke folder upload.
///
/// Berkas ditulis paling akhir: bila penulisan gagal, transaksi dibatalkan, dan bila commit gagal,
/// berkas dihapus lagi.
async fn terbitkan(
    state: &AppState,
    penanda: &PenandaSurat,
    id_pendaftaran: i32,
) -> Result<SuratTersimpan, AppError> {
    let mut tx = state.db.begin().await?;
    repository::lock_pendaftaran(&mut tx, id_pendaftaran).await?;

    // Issued by a request that held the lock before this one.
    if let Some(surat) = repository::find_by_pendaftaran(&mut *tx, id_pendaftaran).await? {
        return Ok(surat);
    }

    let pendaftaran = repository::find_pendaftaran(&mut *tx, id_pendaftaran)
        .await?
        .ok_or(AppError::NotFound)?;
    if pendaftaran.status != STATUS_AKTIF {
        return Err(AppError::Conflict(
            "Surat keterangan hanya tersedia untuk pendaftaran berstatus Aktif.",
        ));
    }

    let tanggal_surat = repository::hari_ini(&mut tx).await?;
    let id_surat = repository::next_id_surat(&mut tx).await?;
    let isi = isi_surat(pendaftaran, id_surat, tanggal_surat)?;

    let isi_json = serde_json::to_string(&isi).map_err(|err| {
        tracing::error!(error = %err, "failed to serialise surat");
        AppError::Internal
    })?;
    let tanda_tangan = penanda.tandatangani(&isi_json);

    let halaman = format!("{}/verifikasi-surat", state.frontend_url);
    let url = format!("{halaman}?kode={}", kode(id_surat, &tanda_tangan));
    let pdf = dokumen::render(&isi, &url, &halaman).map_err(|err| {
        tracing::error!(error = %err, "failed to render surat");
        AppError::Internal
    })?;

    let surat = SuratBaru {
        id_surat,
        id_pendaftaran,
        nama_berkas: format!(
            "surat-keterangan_{id_pendaftaran}_{:032x}.pdf",
            rand::random::<u128>()
        ),
        ukuran_berkas: i32::try_from(pdf.len()).map_err(|_| AppError::Internal)?,
        nomor_surat: isi.nomor_surat.clone(),
        isi: isi_json,
        tanda_tangan,
        hash_berkas: hex_sha256(&pdf),
    };

    repository::insert(&mut tx, &surat).await?;

    // Written last, as in POST /pendaftaran: if the file fails, dropping the transaction undoes
    // the row, and if the commit fails, the file is removed again.
    let path = state.upload_dir.join(&surat.nama_berkas);
    let tulis = async {
        fs::create_dir_all(&state.upload_dir).await?;
        fs::write(&path, &pdf).await
    };
    tulis.await.map_err(|err| {
        tracing::error!(error = %err, "failed to save surat");
        AppError::Internal
    })?;

    if let Err(err) = tx.commit().await {
        let _ = fs::remove_file(&path).await;
        return Err(err.into());
    }

    Ok(SuratTersimpan {
        id_surat,
        nama_berkas: surat.nama_berkas,
        isi: surat.isi,
        tanda_tangan: surat.tanda_tangan,
    })
}

/// Menyusun isi surat dari data pendaftaran, id surat, dan tanggal surat.
///
/// Data yang belum tersedia di database (pejabat, alamat perusahaan, jenis program, masa magang,
/// format nomor) diambil dari [`templat`] yang masih berupa MOCK. Program studi ditulis dengan
/// awalan
/// `S1`. Dijawab `409` bila nama, NIM, atau program studi kosong; seharusnya tidak terjadi, karena
/// mendaftar mensyaratkan data diri lengkap dan data itu tidak bisa dikosongkan lagi.
fn isi_surat(
    pendaftaran: DataPendaftaran,
    id_surat: i32,
    tanggal_surat: time::Date,
) -> Result<IsiSurat, AppError> {
    // Registering requires complete data diri, and it cannot be emptied afterwards.
    let (Some(nama), Some(nim), Some(program_studi)) =
        (pendaftaran.nama, pendaftaran.nim, pendaftaran.program_studi)
    else {
        return Err(AppError::Conflict(
            "Data diri mahasiswa belum lengkap untuk surat keterangan.",
        ));
    };

    let (mulai, selesai) = templat::masa_magang(tanggal_surat);
    Ok(IsiSurat {
        versi: VERSI_ISI,
        nomor_surat: templat::nomor_surat(id_surat, tanggal_surat),
        tanggal_surat: tanggal_surat.to_string(),
        nama,
        nim,
        program_studi: format!("S1 {program_studi}"),
        posisi: pendaftaran.posisi,
        perusahaan: pendaftaran.perusahaan,
        alamat_perusahaan: templat::ALAMAT_PERUSAHAAN.to_string(),
        program: templat::PROGRAM.to_string(),
        tanggal_mulai: mulai.to_string(),
        tanggal_selesai: selesai.to_string(),
        nama_pejabat: templat::NAMA_PEJABAT.to_string(),
        nip_pejabat: templat::NIP_PEJABAT.to_string(),
        jabatan_pejabat: templat::JABATAN_PEJABAT.to_string(),
    })
}

/// Membaca kembali isi surat dari JSON yang tersimpan.
///
/// JSON yang tidak bisa dibaca (misalnya diubah manual di database) dicatat ke log dan dijawab
/// [`AppError::Internal`].
fn baca_isi(surat: &SuratTersimpan) -> Result<IsiSurat, AppError> {
    serde_json::from_str(&surat.isi).map_err(|err| {
        tracing::error!(error = %err, id_surat = surat.id_surat, "surat holds unreadable content");
        AppError::Internal
    })
}

/// Hash SHA-256 dari `isi` dalam 64 karakter heksadesimal huruf kecil, sesuai format kolom
/// `hash_berkas`.
fn hex_sha256(isi: &[u8]) -> String {
    Sha256::digest(isi)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Mengembalikan isi surat hanya bila tanda tangannya masih cocok dengan isi yang tersimpan.
///
/// `tanda_tangan` adalah tanda tangan dari kode QR; bila `None` (verifikasi lewat berkas), dipakai
/// tanda tangan yang tersimpan. Baris yang diubah langsung di database, atau kode yang tidak
/// diterbitkan server ini, dijawab `404` sama seperti surat yang tidak ada, sehingga tidak ada
/// informasi yang bocor.
fn isi_asli(
    penanda: &PenandaSurat,
    surat: Option<SuratTersimpan>,
    tanda_tangan: Option<&str>,
) -> Result<Json<IsiSurat>, AppError> {
    let surat = surat.ok_or(AppError::NotFound)?;
    let tanda_tangan = tanda_tangan.unwrap_or(&surat.tanda_tangan);
    if !penanda.cocok(&surat.isi, tanda_tangan) {
        return Err(AppError::NotFound);
    }
    Ok(Json(baca_isi(&surat)?))
}

/// `GET /api/v1/surat-keterangan/verifikasi?kode=...` (publik): memeriksa surat lewat kode di QR
/// code.
///
/// Kode `{id_surat}.{tanda_tangan}` diurai, baris surat dicari dengan id-nya, lalu tanda tangan
/// dicocokkan dengan isi surat. Bila sah, isi surat dikembalikan. Kode yang rusak, tidak dikenal,
/// atau tanda tangannya tidak cocok dijawab `404`.
pub async fn verifikasi(
    State(state): State<AppState>,
    Query(query): Query<KodeQuery>,
) -> Result<Json<IsiSurat>, AppError> {
    let penanda = penanda(&state)?;
    let (id_surat, tanda_tangan) = parse_kode(&query.kode).ok_or(AppError::NotFound)?;
    let surat = repository::find_by_id(&state.db, id_surat).await?;
    isi_asli(&penanda, surat, Some(tanda_tangan))
}

const BERKAS_WAJIB: AppError = AppError::Validation("Pilih berkas PDF surat keterangan.");

/// `POST /api/v1/surat-keterangan/verifikasi` (publik): memeriksa surat lewat unggahan berkas PDF.
///
/// Berkas dikirim sebagai multipart di field `berkas`. Surat dikenali hanya bila berkasnya sama
/// persis byte demi byte dengan berkas yang diterbitkan, dicocokkan lewat hash SHA-256, dan tanda
/// tangan barisnya masih sah. Berkas kosong atau tidak dikirim dijawab `422`; berkas yang tidak
/// dikenali dijawab `404`.
pub async fn verifikasi_berkas(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<IsiSurat>, AppError> {
    let penanda = penanda(&state)?;

    let mut berkas = None;
    while let Some(field) = multipart.next_field().await.map_err(read_error)? {
        if field.name() == Some("berkas") && berkas.is_none() {
            berkas = Some(field.bytes().await.map_err(read_error)?);
        }
    }
    let berkas = berkas.filter(|isi| !isi.is_empty()).ok_or(BERKAS_WAJIB)?;

    let surat = repository::find_by_hash(&state.db, &hex_sha256(&berkas)).await?;
    isi_asli(&penanda, surat, None)
}

/// Menerjemahkan error pembacaan multipart: body yang melebihi batas ukuran dijawab pesan "Berkas
/// terlalu besar", dan error lain dianggap berkas tidak dikirim dengan benar.
fn read_error(err: MultipartError) -> AppError {
    if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
        AppError::Validation("Berkas terlalu besar untuk sebuah surat keterangan.")
    } else {
        BERKAS_WAJIB
    }
}

/// `GET /api/v1/surat-keterangan/kunci-publik` (publik): kunci publik Ed25519 untuk memeriksa tanda
/// tangan surat.
///
/// Dengan kunci ini, siapa pun bisa memverifikasi tanda tangan secara mandiri tanpa mempercayai
/// jawaban server ini.
pub async fn kunci_publik(State(state): State<AppState>) -> Result<Json<KunciPublik>, AppError> {
    Ok(Json(KunciPublik {
        algoritma: "Ed25519",
        kunci_publik: penanda(&state)?.kunci_publik(),
    }))
}

/// Untuk route: batas ukuran body unggahan verifikasi.
pub const BATAS_BODY_VERIFIKASI: usize = MAX_BERKAS_VERIFIKASI + 64 * 1024;
