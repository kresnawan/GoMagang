use std::io;

use axum::{
    Extension, Json,
    extract::{Multipart, Path, Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use time::OffsetDateTime;
use tokio::fs;

use super::{
    ekspor::{self, Ekspor},
    model::{
        AcceptResult, DaftarPendaftaran, FilterPendaftaran, FormPendaftaran, ListQuery,
        LokasiBerkas, MAX_PENDAFTARAN_BERJALAN, MahasiswaDetail, Pendaftaran, PendaftaranDetail,
        PendaftaranRingkas, SimpanJadwal, SimpanPlotting, SimpanTahapResult, StatusQuery,
        UpdateStatus, UpdateStatusResult,
    },
    repository, storage,
    tautan::MASA_BERLAKU,
};
use crate::{
    error::AppError,
    features::{
        auth::{Peran, User, find_peran},
        mahasiswa::{Mahasiswa, find_by_id_pengguna},
    },
    state::AppState,
};

const SUDAH_AKTIF: &str = "Kamu sudah memiliki pendaftaran yang aktif.";

/// Everything for Tim MB, and only their own for a mahasiswa.
pub async fn list(
    State(state): State<AppState>,
    user: User,
    Query(query): Query<ListQuery>,
) -> Result<Json<DaftarPendaftaran>, AppError> {
    let halaman = query.halaman()?;

    let peran = find_peran(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;

    let id_pengguna = match peran {
        Peran::Mahasiswa => Some(user.id_pengguna),
        Peran::TimMb => None,
    };

    let filter = query.filter(id_pengguna);

    let data = repository::list(&state.db, &filter, Some(halaman)).await?;
    let total = repository::count(&state.db, &filter).await?;

    Ok(Json(DaftarPendaftaran::new(data, halaman, total)))
}

/// Looks up one pendaftaran, whoever it belongs to.
async fn find_ringkas(
    state: &AppState,
    id_pendaftaran: i32,
) -> Result<Option<PendaftaranRingkas>, AppError> {
    let filter = FilterPendaftaran {
        id_pendaftaran: Some(id_pendaftaran),
        ..Default::default()
    };

    Ok(repository::list(&state.db, &filter, None).await?.pop())
}

pub async fn detail(
    State(state): State<AppState>,
    user: User,
    Path(id_pendaftaran): Path<i32>,
) -> Result<Json<PendaftaranDetail>, AppError> {
    let peran = find_peran(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;

    let ringkas = find_ringkas(&state, id_pendaftaran)
        .await?
        .ok_or(AppError::NotFound)?;

    if matches!(peran, Peran::Mahasiswa) && ringkas.mahasiswa.id_pengguna != user.id_pengguna {
        return Err(AppError::Forbidden);
    }

    Ok(Json(load_detail(&state, ringkas).await?))
}

pub async fn update_status(
    State(state): State<AppState>,
    user: User,
    Query(query): Query<StatusQuery>,
    Json(data): Json<UpdateStatus>,
) -> Result<Json<PendaftaranDetail>, AppError> {
    let data = data.validate()?;

    match repository::update_status(&state.db, query.id, user.id_pengguna, &data.status).await? {
        UpdateStatusResult::Updated => {}
        UpdateStatusResult::NotFound => return Err(AppError::NotFound),
        UpdateStatusResult::NotAllowed => {
            return Err(AppError::Conflict("Perubahan status ini tidak diizinkan."));
        }
    }

    Ok(Json(load_changed_detail(&state, query.id).await?))
}

/// The mahasiswa takes up a place they were accepted for, and withdraws from their other running
/// pendaftaran.
pub async fn accept(
    State(state): State<AppState>,
    user: User,
    Query(query): Query<StatusQuery>,
) -> Result<Json<PendaftaranDetail>, AppError> {
    match repository::accept(&state.db, query.id, user.id_pengguna).await? {
        AcceptResult::Accepted => {}
        AcceptResult::NotFound => return Err(AppError::NotFound),
        AcceptResult::NotOwner => return Err(AppError::Forbidden),
        AcceptResult::NotDiterima => {
            return Err(AppError::Conflict(
                "Hanya pendaftaran berstatus Diterima yang bisa dikonfirmasi.",
            ));
        }
        AcceptResult::HasAktif => return Err(AppError::Conflict(SUDAH_AKTIF)),
    }

    Ok(Json(load_changed_detail(&state, query.id).await?))
}

pub async fn simpan_jadwal(
    State(state): State<AppState>,
    Json(data): Json<SimpanJadwal>,
) -> Result<Json<SimpanJadwal>, AppError> {
    let data = data.validate()?;

    match repository::simpan_jadwal(&state.db, &data).await? {
        SimpanTahapResult::Saved => Ok(Json(data)),
        SimpanTahapResult::NotFound => Err(AppError::NotFound),
        SimpanTahapResult::NotAllowed => Err(AppError::Conflict(
            "Semua pendaftaran harus berstatus Lolos Seleksi Administrasi.",
        )),
    }
}

pub async fn simpan_plotting(
    State(state): State<AppState>,
    Json(data): Json<SimpanPlotting>,
) -> Result<Json<SimpanPlotting>, AppError> {
    let data = data.validate()?;

    match repository::simpan_plotting(&state.db, &data).await? {
        SimpanTahapResult::Saved => Ok(Json(data)),
        SimpanTahapResult::NotFound => Err(AppError::NotFound),
        SimpanTahapResult::NotAllowed => Err(AppError::Conflict(
            "Semua pendaftaran harus berstatus Aktif.",
        )),
    }
}

/// Sends the pendaftaran picked by Tim MB as a file, in the order of the list, with a public link
/// to each of their files.
pub async fn ekspor(
    State(state): State<AppState>,
    Json(data): Json<Ekspor>,
) -> Result<impl IntoResponse, AppError> {
    let data = data.validate()?;

    let mut baris = ekspor::find_baris(&state.db, &data.id_pendaftaran).await?;
    if baris.len() != data.id_pendaftaran.len() {
        return Err(AppError::NotFound);
    }

    // The files get public links, as the file is meant for people who do not log in.
    let diekspor = OffsetDateTime::now_utc();
    let tautan_berlaku_sampai = diekspor + MASA_BERLAKU;
    let berkas = ekspor::find_berkas(&state.db, &data.id_pendaftaran).await?;
    ekspor::lampirkan_berkas(&mut baris, berkas, |id_berkas| {
        let token = state
            .tautan_berkas
            .buat(id_berkas, tautan_berlaku_sampai.unix_timestamp());
        format!("{}/berkas/{token}", state.frontend_url)
    });
    let info = ekspor::InfoEkspor {
        diekspor,
        tautan_berlaku_sampai,
    };

    // Building the file is CPU work, so it runs off the async workers.
    let isi = tokio::task::spawn_blocking(move || ekspor::buat_xlsx(&baris, &info))
        .await
        .map_err(|err| {
            eprintln!("export task failed: {err}");
            AppError::Internal
        })?
        .map_err(|err| {
            eprintln!("failed to build export: {err}");
            AppError::Internal
        })?;

    let headers = [
        (header::CONTENT_TYPE, data.format.content_type().to_string()),
        (
            header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=\"{}\"",
                ekspor::nama_berkas(data.format)
            ),
        ),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        // Personal data of mahasiswa, so no cache may keep a copy.
        (header::CACHE_CONTROL, "private, no-store".to_string()),
    ];

    Ok((headers, isi))
}

async fn load_changed_detail(
    state: &AppState,
    id_pendaftaran: i32,
) -> Result<PendaftaranDetail, AppError> {
    let ringkas = find_ringkas(state, id_pendaftaran)
        .await?
        .ok_or(AppError::Internal)?;

    load_detail(state, ringkas).await
}

async fn load_detail(
    state: &AppState,
    ringkas: PendaftaranRingkas,
) -> Result<PendaftaranDetail, AppError> {
    let id_pendaftaran = ringkas.info.id_pendaftaran;
    let id_pemilik = ringkas.mahasiswa.id_pengguna;

    let data = find_by_id_pengguna(&state.db, id_pemilik)
        .await?
        .ok_or(AppError::Internal)?;

    Ok(PendaftaranDetail {
        info: ringkas.info,
        mahasiswa: MahasiswaDetail {
            id_pengguna: id_pemilik,
            data,
        },
        berkas: repository::list_berkas(&state.db, id_pendaftaran).await?,
        log: repository::list_log(&state.db, id_pendaftaran).await?,
        jadwal_seleksi: repository::find_jadwal(&state.db, id_pendaftaran).await?,
        surat_keterangan: repository::find_surat(&state.db, id_pendaftaran).await?,
        plotting_dpm: repository::find_plotting(&state.db, id_pendaftaran).await?,
    })
}

pub async fn create(
    State(state): State<AppState>,
    user: User,
    Extension(mahasiswa): Extension<Mahasiswa>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<Pendaftaran>), AppError> {
    if !mahasiswa.is_lengkap() {
        return Err(AppError::Validation(
            "Lengkapi data diri terlebih dahulu sebelum mendaftar.",
        ));
    }

    // Read before the transaction starts, so that a slow upload holds no locks.
    let form = FormPendaftaran::read(multipart).await?;
    let berkas = form.berkas(user.id_pengguna);

    let mut tx = state.db.begin().await?;

    repository::lock_mahasiswa(&mut tx, user.id_pengguna).await?;
    let posisi = repository::find_posisi(&mut tx, form.id_posisi)
        .await?
        .ok_or(AppError::Validation("Posisi tidak ditemukan."))?;

    if !posisi.periode_berlangsung {
        return Err(AppError::Conflict(
            "Periode pendaftaran posisi ini tidak sedang berlangsung.",
        ));
    }

    let berjalan = repository::count_berjalan(&mut tx, user.id_pengguna, form.id_posisi).await?;
    if berjalan.aktif > 0 {
        return Err(AppError::Conflict(SUDAH_AKTIF));
    }
    if berjalan.di_posisi > 0 {
        return Err(AppError::Conflict(
            "Kamu masih punya pendaftaran yang berjalan di posisi ini.",
        ));
    }
    if berjalan.total >= MAX_PENDAFTARAN_BERJALAN {
        return Err(AppError::Conflict(
            "Kamu sudah memiliki 2 pendaftaran yang sedang berjalan.",
        ));
    }

    let pendaftaran =
        repository::insert(&mut tx, user.id_pengguna, form.id_posisi, &berkas).await?;

    // Written last so that a rejected request leaves nothing on disk. If the files fail, dropping
    // the transaction undoes the rows.
    storage::save(&state.upload_dir, &berkas)
        .await
        .map_err(|err| {
            eprintln!("failed to save berkas: {err}");
            AppError::Internal
        })?;

    if let Err(err) = tx.commit().await {
        storage::remove(&state.upload_dir, &berkas).await;
        return Err(err.into());
    }

    Ok((StatusCode::CREATED, Json(pendaftaran)))
}

/// Sends a file to the mahasiswa who uploaded it, or to Tim MB.
pub async fn download_berkas(
    State(state): State<AppState>,
    user: User,
    Path(id_berkas): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let peran = find_peran(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;

    let berkas = repository::find_berkas(&state.db, id_berkas)
        .await?
        .ok_or(AppError::NotFound)?;

    if matches!(peran, Peran::Mahasiswa) && berkas.id_pengguna != user.id_pengguna {
        return Err(AppError::Forbidden);
    }

    let isi = baca_berkas(&state, id_berkas, &berkas).await?;

    let headers = [
        (header::CONTENT_TYPE, "application/pdf".to_string()),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", berkas.nama_berkas),
        ),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        // The file is private to its owner and Tim MB, so no cache may keep a copy.
        (header::CACHE_CONTROL, "private, no-store".to_string()),
    ];

    Ok((headers, isi))
}

/// Sends a file to anyone holding a link from an export, without logging in, until the link
/// expires. A link that is changed, expired, or made elsewhere is a 404, like a missing file.
pub async fn download_berkas_publik(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let sekarang = OffsetDateTime::now_utc().unix_timestamp();
    let id_berkas = state
        .tautan_berkas
        .baca(&token, sekarang)
        .ok_or(AppError::NotFound)?;

    let berkas = repository::find_berkas(&state.db, id_berkas)
        .await?
        .ok_or(AppError::NotFound)?;
    let isi = baca_berkas(&state, id_berkas, &berkas).await?;

    // "cv-225150200111001.pdf", rather than the name kept on the server.
    let jenis = berkas.jenis_berkas.to_lowercase().replace(' ', "-");
    let nama = match berkas.nim.as_deref() {
        Some(nim) => format!("{jenis}-{nim}.pdf"),
        None => format!("{jenis}.pdf"),
    };

    let headers = [
        (header::CONTENT_TYPE, "application/pdf".to_string()),
        (
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{nama}\""),
        ),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        (header::CACHE_CONTROL, "private, no-store".to_string()),
        (
            header::HeaderName::from_static("x-robots-tag"),
            "noindex".to_string(),
        ),
    ];

    Ok((headers, isi))
}

async fn baca_berkas(
    state: &AppState,
    id_berkas: i32,
    berkas: &LokasiBerkas,
) -> Result<Vec<u8>, AppError> {
    fs::read(state.upload_dir.join(&berkas.nama_berkas))
        .await
        .map_err(|err| {
            eprintln!("failed to read berkas {id_berkas}: {err}");
            match err.kind() {
                io::ErrorKind::NotFound => AppError::NotFound,
                _ => AppError::Internal,
            }
        })
}
