//! Membatalkan pendaftaran `Diterima` yang tidak dikonfirmasi mahasiswa dalam 12 jam, lalu memberi
//! tahu mahasiswa lewat email.

use std::time::Duration;

use super::{
    handler::load_changed_detail,
    notifikasi::{self, Pengubah},
    repository,
};
use crate::state::AppState;

/// Seberapa sering pemeriksaan dijalankan. Pendaftaran dibatalkan paling lambat selama selang ini
/// setelah tenggatnya lewat, dan selama selang itu konfirmasi yang terlambat sudah ditolak.
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
            tracing::error!(error = %err, "failed to cancel unconfirmed pendaftaran");
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
            Err(_) => tracing::error!(id_pendaftaran, "failed to load cancelled pendaftaran"),
        }
    }
}
