use std::{io, path::Path};

use tokio::fs;

use super::model::BerkasBaru;

/// Menulis berkas-berkas pendaftaran ke folder upload, semuanya atau tidak sama sekali.
///
/// Folder dibuat bila belum ada. Folder ini tidak dilayani langsung oleh route mana pun; berkas
/// hanya
/// bisa diunduh lewat endpoint yang memeriksa hak akses. Bila salah satu berkas gagal ditulis,
/// berkas
/// yang sudah sempat ditulis dihapus lagi.
pub async fn save(dir: &Path, berkas: &[BerkasBaru]) -> io::Result<()> {
    fs::create_dir_all(dir).await?;

    for file in berkas {
        if let Err(err) = fs::write(dir.join(&file.nama), &file.isi).await {
            remove(dir, berkas).await;
            return Err(err);
        }
    }

    Ok(())
}

/// Menghapus berkas-berkas dari folder upload sebisanya, tanpa melaporkan kegagalan.
///
/// Berkas yang tertinggal tidak berbahaya, karena tidak ada baris di database yang merujuknya
/// sehingga tidak bisa diakses siapa pun.
pub async fn remove(dir: &Path, berkas: &[BerkasBaru]) {
    for file in berkas {
        let _ = fs::remove_file(dir.join(&file.nama)).await;
    }
}
