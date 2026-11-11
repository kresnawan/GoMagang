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

/// Without SURAT_SIGNING_KEY nothing can be signed or checked.
fn penanda(state: &AppState) -> Result<Arc<PenandaSurat>, AppError> {
    state
        .penanda_surat
        .clone()
        .ok_or(AppError::ServiceUnavailable)
}

/// Sends the surat of a pendaftaran to the mahasiswa it belongs to, or to Panitia Magang. The first
/// download of an Aktif pendaftaran issues it; every later one sends the same file.
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
            eprintln!("failed to read surat {}: {err}", surat.id_surat);
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

/// Builds, signs and stores the surat. Runs under a lock on the pendaftaran, so two first
/// downloads at once still give one surat, and a status change cannot slip in between.
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
        eprintln!("failed to serialise surat: {err}");
        AppError::Internal
    })?;
    let tanda_tangan = penanda.tandatangani(&isi_json);

    let halaman = format!("{}/verifikasi-surat", state.frontend_url);
    let url = format!("{halaman}?kode={}", kode(id_surat, &tanda_tangan));
    let pdf = dokumen::render(&isi, &url, &halaman).map_err(|err| {
        eprintln!("failed to render surat: {err}");
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
        eprintln!("failed to save surat: {err}");
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

fn baca_isi(surat: &SuratTersimpan) -> Result<IsiSurat, AppError> {
    serde_json::from_str(&surat.isi).map_err(|err| {
        eprintln!("surat {} holds unreadable content: {err}", surat.id_surat);
        AppError::Internal
    })
}

fn hex_sha256(isi: &[u8]) -> String {
    Sha256::digest(isi)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Only a row whose signature still matches its content counts. A row edited in the database,
/// or a code that was not issued here, fails like an unknown one.
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

/// Public: what the QR on a surat links to.
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

/// Public: recognises an uploaded PDF as an issued surat only if it is the same file, byte for
/// byte.
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

fn read_error(err: MultipartError) -> AppError {
    if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
        AppError::Validation("Berkas terlalu besar untuk sebuah surat keterangan.")
    } else {
        BERKAS_WAJIB
    }
}

/// Public: lets anyone check a signature without trusting this server's answer.
pub async fn kunci_publik(State(state): State<AppState>) -> Result<Json<KunciPublik>, AppError> {
    Ok(Json(KunciPublik {
        algoritma: "Ed25519",
        kunci_publik: penanda(&state)?.kunci_publik(),
    }))
}

/// For the route: the body limit of the verification upload.
pub const BATAS_BODY_VERIFIKASI: usize = MAX_BERKAS_VERIFIKASI + 64 * 1024;
