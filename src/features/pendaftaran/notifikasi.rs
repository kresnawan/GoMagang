//! The email a mahasiswa gets when Panitia Magang changes the status of their pendaftaran. The
//! wording follows the dashboard in the frontend, so the two say the same thing.

use super::model::PendaftaranDetail;
use crate::email::{Email, GambarInline, Html};

const TEMPLAT: &str = include_str!("../../../templates/email/perubahan_status.html");

const LOGO: GambarInline = GambarInline {
    cid: "logo",
    png: include_bytes!("../../../assets/email/logo.png"),
};
const IKON_BERHASIL: GambarInline = GambarInline {
    cid: "ikon-berhasil",
    png: include_bytes!("../../../assets/email/ikon-berhasil.png"),
};
const IKON_GAGAL: GambarInline = GambarInline {
    cid: "ikon-gagal",
    png: include_bytes!("../../../assets/email/ikon-gagal.png"),
};

/// Badge colours, the same as the StatusBadge tones in the frontend.
struct Nada {
    latar: &'static str,
    teks: &'static str,
    garis: &'static str,
}

const BERHASIL: Nada = Nada {
    latar: "#f0fdf4",
    teks: "#166534",
    garis: "#bbf7d0",
};
const GAGAL: Nada = Nada {
    latar: "#fef2f2",
    teks: "#991b1b",
    garis: "#fecaca",
};
const MENUNGGU: Nada = Nada {
    latar: "#fffbeb",
    teks: "#92400e",
    garis: "#fde68a",
};

struct Tampilan {
    judul: &'static str,
    pesan: &'static str,
    ikon: Option<GambarInline>,
    nada: Nada,
}

fn tampilan(status: &str) -> Tampilan {
    let (judul, pesan, ikon, nada) = match status {
        "Lolos Seleksi Administrasi" => (
            "Kamu lolos seleksi administrasi",
            "Selanjutnya seleksi substansi. Jadwalnya ada di detail pendaftaran.",
            Some(IKON_BERHASIL),
            BERHASIL,
        ),
        "Diterima" => (
            "Selamat, kamu diterima!",
            "Konfirmasi penerimaan di detail pendaftaran untuk memulai magang.",
            Some(IKON_BERHASIL),
            BERHASIL,
        ),
        "Aktif" => (
            "Magang kamu sudah aktif",
            "Magang sedang berjalan. Jangan lupa isi konversi mata kuliah.",
            Some(IKON_BERHASIL),
            BERHASIL,
        ),
        "Selesai" => (
            "Magang kamu telah selesai",
            "Terima kasih telah menyelesaikan program magang. Selamat atas pencapaianmu!",
            Some(IKON_BERHASIL),
            BERHASIL,
        ),
        "Tidak Lolos" => (
            "Kamu belum lolos seleksi",
            "Terima kasih telah mendaftar. Tetap semangat dan coba lagi di kesempatan berikutnya.",
            Some(IKON_GAGAL),
            GAGAL,
        ),
        "Batal/Mengundurkan Diri" => (
            "Pendaftaran kamu dibatalkan",
            "Pendaftaran ini tidak lagi diproses.",
            Some(IKON_GAGAL),
            GAGAL,
        ),
        _ => (
            "Pendaftaran kamu sedang diperiksa",
            "Berkasmu sedang diperiksa panitia.",
            None,
            MENUNGGU,
        ),
    };
    Tampilan {
        judul,
        pesan,
        ikon,
        nada,
    }
}

pub fn perubahan_status(
    detail: &PendaftaranDetail,
    status_lama: &str,
    frontend_url: &str,
) -> Email {
    let mahasiswa = &detail.mahasiswa.data;
    let info = &detail.info;
    let tampilan = tampilan(&info.status);
    let sapaan = match mahasiswa.nama.as_deref() {
        Some(nama) => format!("Halo {nama},"),
        None => "Halo,".to_string(),
    };
    let tautan = format!("{frontend_url}/dasbor/{}", info.id_pendaftaran);
    let subjek = format!("Status pendaftaran magang: {}", info.status);

    let isi = format!(
        "{sapaan}\n\n\
         {judul}\n\n\
         Status pendaftaran magang kamu telah diperbarui oleh Panitia Magang.\n\n\
         Posisi      : {posisi}\n\
         Perusahaan  : {perusahaan}\n\
         Status lama : {status_lama}\n\
         Status baru : {status_baru}\n\n\
         {pesan}\n\n\
         Lihat detail pendaftaran kamu di {tautan}\n\n\
         Email ini dikirim otomatis oleh GoMagang, mohon tidak dibalas.",
        judul = tampilan.judul,
        posisi = info.posisi,
        perusahaan = info.perusahaan.nama_perusahaan,
        status_baru = info.status,
        pesan = tampilan.pesan,
    );

    let ikon = match &tampilan.ikon {
        Some(ikon) => format!(
            r#"<img src="cid:{}" width="56" height="56" alt="" style="display:block;width:56px;height:56px;margin:0 auto 20px;border:0;">"#,
            ikon.cid
        ),
        None => String::new(),
    };
    let html = render(
        TEMPLAT,
        &[
            ("subjek", escape(&subjek)),
            ("ikon", ikon),
            ("judul", escape(tampilan.judul)),
            ("sapaan", escape(&sapaan)),
            ("posisi", escape(&info.posisi)),
            ("perusahaan", escape(&info.perusahaan.nama_perusahaan)),
            ("status_baru", escape(&info.status)),
            ("status_lama", escape(status_lama)),
            ("badge_latar", tampilan.nada.latar.to_string()),
            ("badge_teks", tampilan.nada.teks.to_string()),
            ("badge_garis", tampilan.nada.garis.to_string()),
            ("pesan", escape(tampilan.pesan)),
            ("tautan", escape(&tautan)),
        ],
    );

    let mut gambar = vec![LOGO];
    gambar.extend(tampilan.ikon);

    Email {
        kepada: mahasiswa.email.clone(),
        subjek,
        isi,
        html: Some(Html { isi: html, gambar }),
    }
}

/// Fills every `{{nama}}` in one pass, so a value is never read as a placeholder itself. The
/// values must already be safe HTML.
fn render(templat: &str, nilai: &[(&str, String)]) -> String {
    let mut hasil = String::with_capacity(templat.len());
    let mut sisa = templat;
    while let Some(awal) = sisa.find("{{") {
        hasil.push_str(&sisa[..awal]);
        let setelah = &sisa[awal + 2..];
        let akhir = setelah
            .find("}}")
            .expect("unclosed placeholder in email template");
        let nama = &setelah[..akhir];
        let (_, isi) = nilai
            .iter()
            .find(|(kunci, _)| *kunci == nama)
            .unwrap_or_else(|| panic!("no value for {{{{{nama}}}}} in email template"));
        hasil.push_str(isi);
        sisa = &setelah[akhir + 2..];
    }
    hasil.push_str(sisa);
    hasil
}

fn escape(teks: &str) -> String {
    let mut hasil = String::with_capacity(teks.len());
    for c in teks.chars() {
        match c {
            '&' => hasil.push_str("&amp;"),
            '<' => hasil.push_str("&lt;"),
            '>' => hasil.push_str("&gt;"),
            '"' => hasil.push_str("&quot;"),
            '\'' => hasil.push_str("&#39;"),
            _ => hasil.push(c),
        }
    }
    hasil
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_fills_each_placeholder_once() {
        let hasil = render(
            "<p>{{a}} dan {{b}}</p>",
            &[("a", "{{b}}".to_string()), ("b", "dua".to_string())],
        );
        assert_eq!(hasil, "<p>{{b}} dan dua</p>");
    }

    #[test]
    fn escape_neutralises_html() {
        assert_eq!(
            escape(r#"<script>"PT A&B's"</script>"#),
            "&lt;script&gt;&quot;PT A&amp;B&#39;s&quot;&lt;/script&gt;"
        );
    }
}
