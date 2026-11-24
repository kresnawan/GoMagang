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
        LokasiBerkas, MAX_PENDAFTARAN_PER_PERIODE, MahasiswaDetail, Pendaftaran, PendaftaranDetail,
        PendaftaranRingkas, SimpanJadwal, SimpanPlotting, SimpanTahapResult, StatusQuery,
        UpdateStatus, UpdateStatusResult,
    },
    notifikasi::{self, Pengubah},
    rekap_mk, repository, storage,
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

/// `GET /api/v1/pendaftaran`: daftar pendaftaran per halaman, dari yang paling baru.
///
/// Panitia Magang melihat semua pendaftaran, sedangkan mahasiswa hanya pendaftarannya sendiri.
/// Filter opsional: `periode`, `status`, `perusahaan`, `posisi`, dan `s` (pencarian nama
/// mahasiswa).
/// Halaman diatur dengan `p` dan `r`. Respons memuat data halaman ini beserta `total` dan
/// `total_pages` untuk filter yang sama. Pengguna tanpa peran dijawab `403`.
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
        Peran::PanitiaMagang => None,
    };

    let filter = query.filter(id_pengguna);

    let data = repository::list(&state.db, &filter, Some(halaman)).await?;
    let total = repository::count(&state.db, &filter).await?;

    Ok(Json(DaftarPendaftaran::new(data, halaman, total)))
}

/// Mencari satu pendaftaran berdasarkan id, milik siapa pun, dengan query yang sama seperti daftar
/// pendaftaran tanpa batas halaman.
///
/// Pemeriksaan hak akses menjadi tanggung jawab pemanggil.
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

/// `GET /api/v1/pendaftaran/{id}`: detail lengkap satu pendaftaran.
///
/// Isinya data pendaftaran, data diri mahasiswa, berkas, riwayat status, jadwal seleksi, surat
/// keterangan, dan plotting DPM. Panitia Magang bisa melihat pendaftaran mana pun; mahasiswa hanya
/// miliknya sendiri, dan pendaftaran orang lain dijawab `403`. Pendaftaran yang tidak ada dijawab
/// `404`.
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

/// `PATCH /api/v1/pendaftaran?id=...`: Panitia Magang mengubah status satu pendaftaran.
///
/// Status baru harus dikenal (`422`) dan perubahannya harus diizinkan (lihat
/// [`panitia_magang_can_change`](super::model::panitia_magang_can_change)); bila tidak, dijawab
/// `409`. Pendaftaran yang tidak ada dijawab `404`.
///
/// Bila berhasil, mahasiswa pemilik pendaftaran dikirimi email pemberitahuan di latar belakang,
/// dan detail pendaftaran terbaru dikembalikan. Untuk status `Diterima`, email memuat tenggat
/// konfirmasi 1 x 12 jam.
pub async fn update_status(
    State(state): State<AppState>,
    user: User,
    Query(query): Query<StatusQuery>,
    Json(data): Json<UpdateStatus>,
) -> Result<Json<PendaftaranDetail>, AppError> {
    let data = data.validate()?;

    let status_lama =
        match repository::update_status(&state.db, query.id, user.id_pengguna, &data.status).await?
        {
            UpdateStatusResult::Updated { status_lama } => status_lama,
            UpdateStatusResult::NotFound => return Err(AppError::NotFound),
            UpdateStatusResult::NotAllowed => {
                return Err(AppError::Conflict("Perubahan status ini tidak diizinkan."));
            }
        };

    let detail = load_changed_detail(&state, query.id).await?;
    state.email.kirim(notifikasi::perubahan_status(
        &detail,
        &status_lama,
        Pengubah::PanitiaMagang,
        &state.frontend_url,
    ));

    Ok(Json(detail))
}

/// `PATCH /api/v1/pendaftaran/accept?id=...`: mahasiswa mengonfirmasi penerimaan.
///
/// Pendaftaran yang dikonfirmasi menjadi `Aktif`, dan pendaftaran lain milik mahasiswa itu yang
/// masih
/// berjalan menjadi `Batal/Mengundurkan Diri`. Error yang mungkin:
/// - `404`: pendaftaran tidak ada.
/// - `403`: pendaftaran milik mahasiswa lain.
/// - `409`: status bukan `Diterima`, mahasiswa sudah punya pendaftaran `Aktif`, atau tenggat
///   konfirmasi 12 jam sudah lewat.
///
/// Bila berhasil, detail pendaftaran terbaru dikembalikan.
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
        AcceptResult::Kedaluwarsa => {
            return Err(AppError::Conflict(
                "Batas waktu konfirmasi 1 x 12 jam sudah lewat, sehingga pendaftaran ini dibatalkan.",
            ));
        }
    }

    Ok(Json(load_changed_detail(&state, query.id).await?))
}

/// `PUT /api/v1/pendaftaran/jadwal-seleksi-substansi`: Panitia Magang menyimpan satu jadwal seleksi
/// substansi untuk satu atau banyak pendaftaran sekaligus.
///
/// Semua pendaftaran harus ada (`404`) dan berstatus `Lolos Seleksi Administrasi` (`409`). Bila ada
/// satu saja yang tidak memenuhi, tidak ada yang disimpan. Jadwal yang sudah ada diganti. Bila
/// berhasil, data yang tersimpan dikembalikan.
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

/// `PUT /api/v1/pendaftaran/plot-dpm`: Panitia Magang menyimpan satu DPM (Dosen Pembimbing Magang)
/// untuk satu atau banyak pendaftaran sekaligus.
///
/// Semua pendaftaran harus ada (`404`) dan berstatus `Aktif` (`409`). Bila ada satu saja yang tidak
/// memenuhi, tidak ada yang disimpan. Plotting yang sudah ada diganti. Bila berhasil, data yang
/// tersimpan dikembalikan.
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

/// `POST /api/v1/pendaftaran/ekspor`: Panitia Magang mengekspor pendaftaran pilihan menjadi berkas
/// Excel.
///
/// Baris mengikuti urutan pada daftar pendaftaran. Setiap berkas pendaftaran diberi tautan publik
/// bertanda tangan yang berlaku selama [`MASA_BERLAKU`] (30 hari), karena berkas Excel ini
/// ditujukan
/// untuk pihak luar (perusahaan mitra, kaprodi) yang tidak login. Bila ada id yang tidak ditemukan,
/// dijawab `404`.
///
/// Penyusunan berkas dijalankan di thread terpisah (`spawn_blocking`) karena berat di CPU. Respons
/// diberi header `Cache-Control: private, no-store` karena memuat data pribadi mahasiswa.
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

    let isi = bangun_berkas(move || ekspor::buat_xlsx(&baris, &info)).await?;
    Ok(respons_berkas(data.format, "pendaftar", isi))
}

/// `POST /api/v1/pendaftaran/ekspor-rekap-mk`: Panitia Magang mengekspor rekap penyetaraan mata
/// kuliah dari pendaftaran pilihan menjadi berkas Excel.
///
/// Body dan aturannya sama dengan [`ekspor`]: id diurutkan dan duplikatnya dibuang, maksimal
/// 10.000, dan satu id yang tidak ditemukan membuat seluruh ekspor dijawab `404`. Pendaftaran
/// yang belum memilih mata kuliah tetap muncul di lembar Ringkasan. Isi berkas dijelaskan di
/// [`rekap_mk::buat_xlsx`].
pub async fn ekspor_rekap_mk(
    State(state): State<AppState>,
    Json(data): Json<Ekspor>,
) -> Result<impl IntoResponse, AppError> {
    let data = data.validate()?;

    let pendaftaran = ekspor::find_baris(&state.db, &data.id_pendaftaran).await?;
    if pendaftaran.len() != data.id_pendaftaran.len() {
        return Err(AppError::NotFound);
    }
    let mk = rekap_mk::find_mk(&state.db, &data.id_pendaftaran).await?;
    let diekspor = OffsetDateTime::now_utc();

    let isi = bangun_berkas(move || rekap_mk::buat_xlsx(&pendaftaran, &mk, diekspor)).await?;
    Ok(respons_berkas(data.format, "rekap-mk", isi))
}

/// Menyusun berkas ekspor di thread terpisah (`spawn_blocking`), karena berat di CPU.
async fn bangun_berkas<F>(bangun: F) -> Result<Vec<u8>, AppError>
where
    F: FnOnce() -> Result<Vec<u8>, rust_xlsxwriter::XlsxError> + Send + 'static,
{
    tokio::task::spawn_blocking(bangun)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "export task failed");
            AppError::Internal
        })?
        .map_err(|err| {
            tracing::error!(error = %err, "failed to build export");
            AppError::Internal
        })
}

/// Respons unduhan berkas ekspor, dengan nama berkas `{awalan}-{YYYYMMDD}-{HHMM}.{ekstensi}`.
fn respons_berkas(
    format: ekspor::FormatEkspor,
    awalan: &str,
    isi: Vec<u8>,
) -> impl IntoResponse + use<> {
    let headers = [
        (header::CONTENT_TYPE, format.content_type().to_string()),
        (
            header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=\"{}\"",
                ekspor::nama_berkas(awalan, format)
            ),
        ),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        // Personal data of mahasiswa, so no cache may keep a copy.
        (header::CACHE_CONTROL, "private, no-store".to_string()),
    ];
    (headers, isi)
}

/// Memuat detail pendaftaran yang baru saja diubah, misalnya setelah perubahan status.
///
/// Pendaftaran itu seharusnya pasti ada; bila tidak ditemukan, dijawab [`AppError::Internal`],
/// bukan
/// `404`.
pub(super) async fn load_changed_detail(
    state: &AppState,
    id_pendaftaran: i32,
) -> Result<PendaftaranDetail, AppError> {
    let ringkas = find_ringkas(state, id_pendaftaran)
        .await?
        .ok_or(AppError::Internal)?;

    load_detail(state, ringkas).await
}

/// Melengkapi ringkasan pendaftaran menjadi detail: data diri pemilik, berkas, riwayat status,
/// jadwal seleksi, surat keterangan, dan plotting DPM.
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

/// `POST /api/v1/pendaftaran`: mahasiswa mendaftar ke sebuah posisi magang dengan mengunggah
/// berkas.
///
/// Syarat pendaftaran:
/// - Data diri sudah lengkap (`422`).
/// - Posisi ada dan tidak disembunyikan, termasuk perusahaannya (`422`), dan periodenya sedang
///   berlangsung (`409`).
/// - Mahasiswa belum punya pendaftaran `Aktif` (`409`).
/// - Tidak ada pendaftaran yang masih berjalan di posisi yang sama (`409`).
/// - Belum mendaftar 2 kali di periode yang sama, termasuk pendaftaran yang batal, tidak lolos,
///   atau
///   selesai (`409`).
///
/// Body multipart dibaca sebelum transaksi dimulai, agar unggahan yang lambat tidak menahan kunci.
/// Mahasiswa dikunci selama pemeriksaan, sehingga dua request bersamaan tidak bisa sama-sama lolos
/// batas jumlah. Berkas ditulis ke disk paling akhir: bila gagal, transaksi dibatalkan, dan bila
/// commit gagal, berkas dihapus lagi. Bila berhasil, dijawab `201` beserta pendaftaran dan
/// berkasnya.
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

    let jumlah = repository::count_pendaftaran(&mut tx, user.id_pengguna, form.id_posisi).await?;
    if jumlah.aktif > 0 {
        return Err(AppError::Conflict(SUDAH_AKTIF));
    }
    if jumlah.berjalan_di_posisi > 0 {
        return Err(AppError::Conflict(
            "Kamu masih punya pendaftaran yang berjalan di posisi ini.",
        ));
    }
    // Cancelled, rejected, and finished registrations count too.
    if jumlah.di_periode >= MAX_PENDAFTARAN_PER_PERIODE {
        return Err(AppError::Conflict(
            "Kamu sudah mendaftar 2 kali di periode ini.",
        ));
    }

    let pendaftaran =
        repository::insert(&mut tx, user.id_pengguna, form.id_posisi, &berkas).await?;

    // Written last so that a rejected request leaves nothing on disk. If the files fail, dropping
    // the transaction undoes the rows.
    storage::save(&state.upload_dir, &berkas)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "failed to save berkas");
            AppError::Internal
        })?;

    if let Err(err) = tx.commit().await {
        storage::remove(&state.upload_dir, &berkas).await;
        return Err(err.into());
    }

    Ok((StatusCode::CREATED, Json(pendaftaran)))
}

/// `GET /api/v1/berkas/{id_berkas}`: mengunduh berkas pendaftaran dengan login.
///
/// Mahasiswa hanya bisa mengunduh berkas yang ia unggah sendiri (`403` untuk milik orang lain),
/// sedangkan Panitia Magang bisa mengunduh semua berkas. Berkas dikirim sebagai lampiran PDF dengan
/// header `Cache-Control: private, no-store`.
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
        // The file is private to its owner and Panitia Magang, so no cache may keep a copy.
        (header::CACHE_CONTROL, "private, no-store".to_string()),
    ];

    Ok((headers, isi))
}

/// `GET /api/v1/berkas/publik/{token}`: mengunduh berkas lewat tautan publik dari hasil ekspor,
/// tanpa login.
///
/// Tautan berlaku sampai kedaluwarsa. Token yang diubah, kedaluwarsa, atau dibuat di tempat lain
/// dijawab `404`, sama seperti berkas yang tidak ada, sehingga tidak ada informasi yang bocor.
///
/// Berkas ditampilkan langsung di browser (`inline`) dengan nama yang mudah dibaca, misalnya
/// `cv-225150200111001.pdf`, bukan nama berkas di server. Header `X-Robots-Tag: noindex` mencegah
/// tautan diindeks mesin pencari.
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

/// Membaca isi berkas dari folder upload.
///
/// Berkas yang hilang dari disk dijawab `404`; error baca lain dicatat ke log dan dijawab
/// [`AppError::Internal`].
async fn baca_berkas(
    state: &AppState,
    id_berkas: i32,
    berkas: &LokasiBerkas,
) -> Result<Vec<u8>, AppError> {
    fs::read(state.upload_dir.join(&berkas.nama_berkas))
        .await
        .map_err(|err| {
            tracing::error!(error = %err, id_berkas, "failed to read berkas");
            match err.kind() {
                io::ErrorKind::NotFound => AppError::NotFound,
                _ => AppError::Internal,
            }
        })
}
