//! The email a mahasiswa gets when the status of their pendaftaran changes, by Panitia Magang or by
//! the system cancelling an unconfirmed acceptance. The wording follows the dashboard in the
//! frontend, so the two say the same thing.

use time::{OffsetDateTime, format_description::well_known::Rfc3339};

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

/// Judul, pesan langkah berikutnya, ikon, dan warna badge email untuk setiap status.
///
/// Teksnya mengikuti dasbor di frontend, sehingga email dan aplikasi menyampaikan hal yang sama.
/// Status yang berhasil (Lolos Seleksi Administrasi, Diterima, Aktif, Selesai) memakai ikon centang
/// hijau, status gagal (Tidak Lolos, Batal/Mengundurkan Diri) memakai ikon silang merah, dan
/// Menunggu Verifikasi tidak memakai ikon.
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

/// Who changed the status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pengubah {
    PanitiaMagang,
    /// The time to confirm a Diterima pendaftaran ran out.
    Sistem,
}

/// Menyusun email pemberitahuan perubahan status pendaftaran untuk mahasiswa pemiliknya.
///
/// Email berisi versi HTML (dari templat `templates/email/perubahan_status.html`, dengan logo dan
/// ikon sebagai gambar inline) dan versi teks biasa. Isinya: sapaan, judul sesuai status,
/// keterangan siapa yang mengubah (`pengubah`), kartu posisi dan perusahaan dengan status lama dan
/// baru, langkah berikutnya, dan tombol ke `{frontend_url}/dasbor/{id_pendaftaran}`.
///
/// Bila status baru `Diterima`, subjek diberi keterangan tenggat, tombol menjadi "Konfirmasi
/// penerimaan", dan ditambahkan kotak peringatan merah berisi tenggat konfirmasi dari
/// `batas_konfirmasi`. Semua data dari database di-escape sebelum masuk HTML.
pub fn perubahan_status(
    detail: &PendaftaranDetail,
    status_lama: &str,
    pengubah: Pengubah,
    frontend_url: &str,
) -> Email {
    let mahasiswa = &detail.mahasiswa.data;
    let info = &detail.info;
    let tampilan = tampilan(&info.status);
    let sapaan = match mahasiswa.nama.as_deref() {
        Some(nama) => format!("Halo {nama},"),
        None => "Halo,".to_string(),
    };
    let keterangan = match pengubah {
        Pengubah::PanitiaMagang => {
            "Status pendaftaran magang kamu telah diperbarui oleh Panitia Magang."
        }
        Pengubah::Sistem => {
            "Pendaftaran ini dibatalkan otomatis karena penerimaan tidak dikonfirmasi dalam \
             waktu 1 x 12 jam."
        }
    };
    // Only a Diterima pendaftaran has a time limit, and the list query always gives it one.
    let batas = info.batas_konfirmasi.as_deref().map(format_waktu);
    let tautan = format!("{frontend_url}/dasbor/{}", info.id_pendaftaran);
    let (subjek, tombol) = match &batas {
        Some(_) => (
            format!(
                "Status pendaftaran magang: {} (konfirmasi dalam 1 x 12 jam)",
                info.status
            ),
            "Konfirmasi penerimaan",
        ),
        None => (
            format!("Status pendaftaran magang: {}", info.status),
            "Lihat detail pendaftaran",
        ),
    };

    let mut isi = format!(
        "{sapaan}\n\n\
         {judul}\n\n\
         {keterangan}\n\n\
         Posisi      : {posisi}\n\
         Perusahaan  : {perusahaan}\n\
         Status lama : {status_lama}\n\
         Status baru : {status_baru}\n\n",
        judul = tampilan.judul,
        posisi = info.posisi,
        perusahaan = info.perusahaan.nama_perusahaan,
        status_baru = info.status,
    );
    if let Some(batas) = &batas {
        isi.push_str(&format!("PENTING: {}\n\n", peringatan(batas)));
    }
    isi.push_str(&format!(
        "{pesan}\n\n\
         {tombol}: {tautan}\n\n\
         Email ini dikirim otomatis oleh GoMagang, mohon tidak dibalas.",
        pesan = tampilan.pesan,
    ));

    let ikon = match &tampilan.ikon {
        Some(ikon) => format!(
            r#"<img src="cid:{}" width="56" height="56" alt="" style="display:block;width:56px;height:56px;margin:0 auto 20px;border:0;">"#,
            ikon.cid
        ),
        None => String::new(),
    };
    let kotak_peringatan = match &batas {
        Some(batas) => format!(
            r#"<tr>
      <td style="padding:24px 32px 0;">
        <table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0" style="background-color:#fef2f2;border:1px solid #fecaca;border-left:4px solid #dc2626;border-radius:4px;">
          <tr>
            <td style="padding:16px 20px;font-size:15px;line-height:1.6;color:#991b1b;">
              <p style="margin:0 0 4px;font-weight:700;">PENTING: Konfirmasi dalam 1 x 12 jam</p>
              <p style="margin:0;">{}</p>
            </td>
          </tr>
        </table>
      </td>
    </tr>"#,
            peringatan_html(batas)
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
            ("keterangan", escape(keterangan)),
            ("posisi", escape(&info.posisi)),
            ("perusahaan", escape(&info.perusahaan.nama_perusahaan)),
            ("status_baru", escape(&info.status)),
            ("status_lama", escape(status_lama)),
            ("badge_latar", tampilan.nada.latar.to_string()),
            ("badge_teks", tampilan.nada.teks.to_string()),
            ("badge_garis", tampilan.nada.garis.to_string()),
            ("peringatan", kotak_peringatan),
            ("pesan", escape(tampilan.pesan)),
            ("tombol", escape(tombol)),
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

/// Kalimat peringatan tenggat konfirmasi untuk versi teks biasa, dengan `batas` yang sudah
/// diformat.
fn peringatan(batas: &str) -> String {
    format!(
        "Konfirmasi penerimaan wajib dilakukan paling lambat {batas}. Jika tidak dikonfirmasi \
         hingga batas waktu tersebut, pendaftaran ini otomatis dinyatakan Batal/Mengundurkan Diri."
    )
}

/// Kalimat peringatan tenggat konfirmasi untuk versi HTML, dengan tenggat dan status
/// `Batal/Mengundurkan Diri` dicetak tebal. `batas` di-escape lebih dulu.
fn peringatan_html(batas: &str) -> String {
    format!(
        "Konfirmasi penerimaan wajib dilakukan paling lambat <strong>{}</strong>. Jika tidak \
         dikonfirmasi hingga batas waktu tersebut, pendaftaran ini otomatis dinyatakan \
         <strong>Batal/Mengundurkan Diri</strong>.",
        escape(batas)
    )
}

/// Menulis waktu dari API (RFC 3339, selalu dalam WIB) seperti di frontend, misalnya
/// `Rabu, 11 November 2026 pukul 20.15 WIB`.
///
/// Bila teksnya tidak bisa dibaca sebagai waktu, teks aslinya dikembalikan apa adanya.
fn format_waktu(waktu: &str) -> String {
    const HARI: [&str; 7] = [
        "Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu",
    ];
    const BULAN: [&str; 12] = [
        "Januari",
        "Februari",
        "Maret",
        "April",
        "Mei",
        "Juni",
        "Juli",
        "Agustus",
        "September",
        "Oktober",
        "November",
        "Desember",
    ];

    let Ok(waktu) = OffsetDateTime::parse(waktu, &Rfc3339) else {
        return waktu.to_string();
    };
    format!(
        "{}, {} {} {} pukul {:02}.{:02} WIB",
        HARI[waktu.weekday().number_days_from_monday() as usize],
        waktu.day(),
        BULAN[waktu.month() as usize - 1],
        waktu.year(),
        waktu.hour(),
        waktu.minute(),
    )
}

/// Mengisi setiap placeholder `{{nama}}` di templat dengan nilainya, dalam satu kali lintasan.
///
/// Karena hanya satu lintasan, nilai yang kebetulan berisi `{{...}}` tidak pernah dibaca sebagai
/// placeholder. Nilai harus sudah berupa HTML yang aman (sudah di-escape bila berasal dari data
/// pengguna).
///
/// # Panics
/// Panic bila templat memuat placeholder yang tidak diberi nilai atau tidak ditutup. Ini kesalahan
/// pemrograman yang ditangkap test, bukan kesalahan data.
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

/// Meng-escape karakter khusus HTML (`&`, `<`, `>`, `"`, `'`) agar data dari database aman
/// ditampilkan di email dan tidak bisa menyisipkan tag atau atribut.
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
    fn format_waktu_reads_like_the_frontend() {
        assert_eq!(
            format_waktu("2026-11-11T08:05:00+07:00"),
            "Rabu, 11 November 2026 pukul 08.05 WIB"
        );
    }

    #[test]
    fn escape_neutralises_html() {
        assert_eq!(
            escape(r#"<script>"PT A&B's"</script>"#),
            "&lt;script&gt;&quot;PT A&amp;B&#39;s&quot;&lt;/script&gt;"
        );
    }
}
