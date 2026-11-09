use std::{io, path::Path};

use tokio::fs;

use super::model::BerkasBaru;

/// Writes the files to the upload folder, which no route serves directly. If one of them fails,
/// the ones already written are removed again.
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

/// Best effort: a file left behind here has no row pointing at it, so nobody can reach it.
pub async fn remove(dir: &Path, berkas: &[BerkasBaru]) {
    for file in berkas {
        let _ = fs::remove_file(dir.join(&file.nama)).await;
    }
}
