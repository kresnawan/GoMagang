//! Cancels the Diterima pendaftaran the mahasiswa did not confirm within 12 hours, and tells them by
//! email.

use std::time::Duration;

use super::{
    handler::load_changed_detail,
    notifikasi::{self, Pengubah},
    repository,
};
use crate::state::AppState;

/// How often to look. A pendaftaran is cancelled at most this long after its time ran out, and
/// a late confirmation is refused in between.
const INTERVAL: Duration = Duration::from_secs(60);

/// Menjalankan pembatalan otomatis di latar belakang setiap `INTERVAL` (1 menit).
///
/// Putaran pertama langsung berjalan saat server start, sehingga pendaftaran yang tenggatnya lewat
/// selama server mati langsung diproses. Tidak ada timer per pendaftaran; setiap putaran cukup
/// mencari pendaftaran yang tenggatnya sudah lewat menurut database.
pub fn spawn_pembatalan_otomatis(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(INTERVAL);
        loop {
            interval.tick().await;
            batalkan_yang_kedaluwarsa(&state).await;
        }
    });
}

/// Satu putaran pembatalan otomatis.
///
/// Pendaftaran `Diterima` yang tidak dikonfirmasi dalam 12 jam diubah menjadi
/// `Batal/Mengundurkan Diri` (lihat [`repository::batalkan_yang_kedaluwarsa`]). Setelah itu setiap
/// mahasiswa yang pendaftarannya dibatalkan dikirimi email pemberitahuan dengan keterangan
/// pembatalan otomatis. Kegagalan hanya dicatat ke log, lalu dicoba lagi pada putaran berikutnya.
pub(super) async fn batalkan_yang_kedaluwarsa(state: &AppState) {
    let batal = match repository::batalkan_yang_kedaluwarsa(&state.db).await {
        Ok(batal) => batal,
        Err(err) => {
            eprintln!("failed to cancel unconfirmed pendaftaran: {err}");
            return;
        }
    };

    for id_pendaftaran in batal {
        match load_changed_detail(state, id_pendaftaran).await {
            Ok(detail) => state.email.kirim(notifikasi::perubahan_status(
                &detail,
                "Diterima",
                Pengubah::Sistem,
                &state.frontend_url,
            )),
            Err(_) => eprintln!("failed to load cancelled pendaftaran {id_pendaftaran}"),
        }
    }
}
