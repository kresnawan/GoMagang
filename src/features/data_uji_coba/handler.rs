use std::{ffi::OsStr, io, path::Path};

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use tokio::fs;

use super::{
    model::{BerkasSeed, HasilHapus, HasilSeed, JUMLAH_SEED, TanpaQuery},
    repository,
};
use crate::{error::AppError, state::AppState};

pub async fn hapus_semua(
    State(state): State<AppState>,
    Query(_): Query<TanpaQuery>,
) -> Result<Json<HasilHapus>, AppError> {
    let terhapus = repository::hapus_semua(&state.db).await?;

    // After the commit, as a file cannot be brought back if the transaction fails.
    let mut jumlah_berkas_gagal = 0;
    for nama in &terhapus.berkas {
        if !remove_berkas(&state.upload_dir, nama).await {
            jumlah_berkas_gagal += 1;
        }
    }

    Ok(Json(HasilHapus {
        jumlah_mahasiswa: terhapus.jumlah_mahasiswa,
        jumlah_pendaftaran: terhapus.jumlah_pendaftaran,
        jumlah_berkas: terhapus.berkas.len() - jumlah_berkas_gagal,
        jumlah_berkas_gagal,
    }))
}

pub async fn seed(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<HasilSeed>), AppError> {
    let mut tx = state.db.begin().await?;

    let Some(berkas) = repository::seed(&mut tx).await? else {
        return Err(AppError::Conflict(
            "Tidak ada posisi di periode yang sedang berlangsung.",
        ));
    };

    // Written last, so that a failure leaves nothing behind. If the files fail, dropping the
    // transaction undoes the rows.
    tulis_berkas(&state.upload_dir, &berkas)
        .await
        .map_err(|err| {
            eprintln!("failed to save seeded berkas: {err}");
            AppError::Internal
        })?;

    if let Err(err) = tx.commit().await {
        hapus_berkas_seed(&state.upload_dir, &berkas).await;
        return Err(err.into());
    }

    Ok((
        StatusCode::CREATED,
        Json(HasilSeed {
            jumlah_mahasiswa: JUMLAH_SEED,
            jumlah_pendaftaran: JUMLAH_SEED,
            jumlah_berkas: berkas.len(),
        }),
    ))
}

/// Writes every file, or none: if one fails, the ones already written are removed again.
async fn tulis_berkas(dir: &Path, berkas: &[BerkasSeed]) -> io::Result<()> {
    fs::create_dir_all(dir).await?;
    for file in berkas {
        if let Err(err) = fs::write(dir.join(&file.nama), &file.isi).await {
            hapus_berkas_seed(dir, berkas).await;
            return Err(err);
        }
    }
    Ok(())
}

/// Best effort: a file left behind has no row pointing at it, so nobody can reach it.
async fn hapus_berkas_seed(dir: &Path, berkas: &[BerkasSeed]) {
    for file in berkas {
        let _ = fs::remove_file(dir.join(&file.nama)).await;
    }
}

/// Whether the file is gone. It only ever looks inside the upload folder, whatever the row says.
async fn remove_berkas(dir: &Path, nama: &str) -> bool {
    if Path::new(nama).file_name() != Some(OsStr::new(nama)) {
        eprintln!("refused to remove berkas outside the upload folder: {nama}");
        return false;
    }

    match fs::remove_file(dir.join(nama)).await {
        Ok(()) => true,
        Err(err) if err.kind() == io::ErrorKind::NotFound => true,
        Err(err) => {
            eprintln!("failed to remove berkas {nama}: {err}");
            false
        }
    }
}
