# Dokumentasi API GoMagang

Daftar endpoint yang tersedia di backend saat ini. File ini diperbarui setiap kali ada endpoint yang ditambah atau diubah, dan riwayatnya ada di [bagian paling bawah](#riwayat-perubahan).

**Terakhir diperbarui:** 10 November 2026

## Ringkasan

Base URL lokal: `http://localhost:3000`

| Status | Arti |
|---|---|
| ✅ Jadi | Sudah memakai logika dan data sungguhan |
| 🚧 Placeholder | Route ada, tapi hanya mengembalikan teks `TODO` |

Kolom **Frontend** menunjukkan apakah `gomagang-web` sudah memanggil endpoint itu, atau halamannya masih memakai data mock.

| Method | Path | Fungsi | Akses | Status | Frontend |
|---|---|---|---|---|---|
| `GET` | [`/auth/google/login`](#get-authgooglelogin) | Memulai login Google | Publik | ✅ Jadi | Tersambung |
| `GET` | [`/auth/google`](#get-authgoogle) | Callback dari Google setelah login | Dipanggil oleh Google | ✅ Jadi | Lewat Google |
| `GET` | [`/auth/me`](#get-authme) | Mengambil data user yang sedang login | Login | ✅ Jadi | Tersambung |
| `POST` | [`/auth/refresh`](#post-authrefresh) | Memperbarui access dan refresh token | Refresh token | ✅ Jadi | Tersambung |
| `POST` | [`/auth/logout`](#post-authlogout) | Logout dari perangkat yang dipakai | Publik | ✅ Jadi | Tersambung |
| `GET` | [`/api/v1/mahasiswa/data`](#get-apiv1mahasiswadata) | Mengambil data mahasiswa yang sedang login | Mahasiswa | ✅ Jadi | Tersambung |
| `PATCH` | [`/api/v1/mahasiswa/data`](#patch-apiv1mahasiswadata) | Mengubah sebagian data mahasiswa yang sedang login | Mahasiswa | ✅ Jadi | Tersambung |
| `GET` | [`/api/v1/posisi`](#get-apiv1posisi) | Daftar posisi magang | Mahasiswa, Panitia Magang | ✅ Jadi | Tersambung: halaman Pendaftaran dan Katalog |
| `POST` | [`/api/v1/posisi`](#post-apiv1posisi) | Membuat posisi magang | Panitia Magang | ✅ Jadi | Tersambung: halaman Katalog |
| `PATCH` | [`/api/v1/posisi/{id}`](#patch-apiv1posisiid) | Mengubah nama, kuota, dan deskripsi posisi magang | Panitia Magang | ✅ Jadi | Tersambung: halaman Katalog |
| `DELETE` | [`/api/v1/posisi/{id}`](#delete-apiv1posisiid) | Menyembunyikan posisi magang (soft delete) | Panitia Magang | ✅ Jadi | Tersambung: halaman Katalog |
| `GET` | [`/api/v1/periode`](#get-apiv1periode) | Daftar periode pendaftaran | Mahasiswa, Panitia Magang | ✅ Jadi | Tersambung: halaman Periode, Katalog, dan Pendaftar |
| `POST` | [`/api/v1/periode`](#post-apiv1periode) | Membuat periode pendaftaran | Panitia Magang | ✅ Jadi | Tersambung |
| `PATCH` | [`/api/v1/periode/{id}/start`](#patch-apiv1periodeidstart) | Memulai periode saat itu juga | Panitia Magang | ✅ Jadi | Tersambung: halaman Periode |
| `PATCH` | [`/api/v1/periode/{id}/stop`](#patch-apiv1periodeidstop) | Menghentikan periode saat itu juga | Panitia Magang | ✅ Jadi | Tersambung: halaman Periode |
| `GET` | [`/api/v1/perusahaan`](#get-apiv1perusahaan) | Daftar perusahaan mitra | Mahasiswa, Panitia Magang | ✅ Jadi | Tersambung |
| `POST` | [`/api/v1/perusahaan`](#post-apiv1perusahaan) | Membuat perusahaan mitra | Panitia Magang | ✅ Jadi | Tersambung |
| `PATCH` | [`/api/v1/perusahaan/{id}`](#patch-apiv1perusahaanid) | Mengubah nama perusahaan mitra | Panitia Magang | ✅ Jadi | Tersambung: halaman Perusahaan |
| `DELETE` | [`/api/v1/perusahaan/{id}`](#delete-apiv1perusahaanid) | Menyembunyikan perusahaan mitra (soft delete) | Panitia Magang | ✅ Jadi | Tersambung: halaman Perusahaan |
| `GET` | [`/api/v1/mata-kuliah`](#get-apiv1mata-kuliah) | Daftar mata kuliah konversi | Mahasiswa, Panitia Magang | ✅ Jadi | Tersambung: halaman Konversi MK (mahasiswa dan Panitia Magang) |
| `POST` | [`/api/v1/mata-kuliah`](#post-apiv1mata-kuliah) | Menambah mata kuliah konversi beserta SKS-nya | Panitia Magang | ✅ Jadi | Tersambung: halaman Konversi MK Panitia Magang |
| `DELETE` | [`/api/v1/mata-kuliah/{id}`](#delete-apiv1mata-kuliahid) | Menghapus mata kuliah konversi (soft delete) | Panitia Magang | ✅ Jadi | Tersambung: halaman Konversi MK Panitia Magang |
| `GET` | [`/api/v1/pendaftaran/{id}/penyetaraan-mk`](#get-apiv1pendaftaranidpenyetaraan-mk) | Mata kuliah yang dipilih untuk satu pendaftaran | Mahasiswa (miliknya), Panitia Magang (semua) | ✅ Jadi | Tersambung: halaman Konversi MK mahasiswa |
| `POST` | [`/api/v1/pendaftaran/{id}/penyetaraan-mk`](#post-apiv1pendaftaranidpenyetaraan-mk) | Memilih mata kuliah untuk pendaftaran berstatus Aktif | Mahasiswa pemilik pendaftaran | ✅ Jadi | Tersambung: halaman Konversi MK mahasiswa |
| `DELETE` | [`/api/v1/penyetaraan-mk/{id}`](#delete-apiv1penyetaraan-mkid) | Membatalkan pilihan mata kuliah | Mahasiswa pemilik pendaftaran | ✅ Jadi | Tersambung: halaman Konversi MK mahasiswa |
| `POST` | [`/api/v1/panitia-magang`](#post-apiv1panitia-magang-sementara) | Mendaftarkan anggota Panitia Magang baru (sementara) | `ADMIN_SECRET` | ✅ Jadi | Tidak dipakai, lewat `curl` |
| `POST` | [`/api/v1/pendaftaran`](#post-apiv1pendaftaran) | Mendaftar ke posisi magang, beserta unggah berkas | Mahasiswa | ✅ Jadi | Tersambung: form pendaftaran |
| `PATCH` | [`/api/v1/pendaftaran?id={id}`](#patch-apiv1pendaftaran) | Mengubah status pendaftaran | Panitia Magang | ✅ Jadi | Tersambung: dialog Pendaftar |
| `PUT` | [`/api/v1/pendaftaran/jadwal-seleksi-substansi`](#put-apiv1pendaftaranjadwal-seleksi-substansi) | Menyimpan satu jadwal seleksi substansi untuk satu atau banyak pendaftaran | Panitia Magang | ✅ Jadi | Tersambung: halaman Pendaftar |
| `PUT` | [`/api/v1/pendaftaran/plot-dpm`](#put-apiv1pendaftaranplot-dpm) | Menyimpan satu plotting DPM untuk satu atau banyak pendaftaran | Panitia Magang | ✅ Jadi | Tersambung: halaman Pendaftar |
| `POST` | [`/api/v1/pendaftaran/ekspor`](#post-apiv1pendaftaranekspor) | Mengekspor pendaftaran terpilih ke Excel (.xlsx) | Panitia Magang | ✅ Jadi | Tersambung: halaman Pendaftar |
| `PATCH` | [`/api/v1/pendaftaran/accept?id={id}`](#patch-apiv1pendaftaranaccept) | Mengonfirmasi penerimaan, status `Diterima` menjadi `Aktif` | Mahasiswa pemilik pendaftaran | ✅ Jadi | Tersambung: halaman detail pendaftaran |
| `GET` | [`/api/v1/berkas/{id_berkas}`](#get-apiv1berkasid_berkas) | Mengunduh berkas pendaftaran | Pemilik berkas, Panitia Magang | ✅ Jadi | Tersambung |
| `GET` | [`/api/v1/berkas/publik/{token}`](#get-apiv1berkaspubliktoken) | Membuka berkas pendaftaran lewat link dari file ekspor, tanpa login | Publik, dengan link yang masih berlaku | ✅ Jadi | Tersambung: halaman `/berkas/{token}` |
| `GET` | [`/api/v1/pendaftaran/{id}/surat-keterangan`](#get-apiv1pendaftaranidsurat-keterangan) | Mengunduh surat keterangan magang, diterbitkan dan ditandatangani saat pertama diunduh | Mahasiswa pemilik pendaftaran, Panitia Magang | ✅ Jadi | Tersambung: halaman detail pendaftaran |
| `GET` | [`/api/v1/surat-keterangan/verifikasi?kode=`](#get-apiv1surat-keteranganverifikasi) | Memverifikasi surat dari kode di QR-nya | Publik | ✅ Jadi | Tersambung: halaman `/verifikasi-surat` |
| `POST` | [`/api/v1/surat-keterangan/verifikasi`](#post-apiv1surat-keteranganverifikasi) | Memverifikasi surat dari berkas PDF-nya | Publik | ✅ Jadi | Tersambung: halaman `/verifikasi-surat` |
| `GET` | [`/api/v1/surat-keterangan/kunci-publik`](#get-apiv1surat-keterangankunci-publik) | Kunci publik Ed25519 penanda tangan surat | Publik | ✅ Jadi | Tidak dipakai |
| `GET` | [`/api/v1/pendaftaran`](#get-apiv1pendaftaran) | Daftar pendaftaran per halaman, bisa disaring menurut periode, status, perusahaan, dan posisi | Mahasiswa (miliknya), Panitia Magang (semua) | ✅ Jadi | Tersambung: halaman Dasbor, Konversi MK, dan Pendaftar |
| `GET` | [`/api/v1/pendaftaran/{id}`](#get-apiv1pendaftaranid) | Detail pendaftaran | Mahasiswa (miliknya), Panitia Magang (semua) | ✅ Jadi | Tersambung: halaman detail pendaftaran dan dialog Pendaftar |
| `GET` | [`/api/v1/templat-sp-dpa`](#get-apiv1templat-sp-dpa) | Tautan templat surat pernyataan DPA yang berlaku | Mahasiswa, Panitia Magang | ✅ Jadi | Tersambung: halaman Templat SP DPA (mahasiswa dan Panitia Magang) dan form pendaftaran |
| `PUT` | [`/api/v1/templat-sp-dpa`](#put-apiv1templat-sp-dpa) | Mengganti tautan templat surat pernyataan DPA | Panitia Magang | ✅ Jadi | Tersambung: halaman Templat SP DPA Panitia Magang |

## Progress

**Semua 44 endpoint sudah jadi**, tidak ada lagi yang placeholder.

| Area | Backend | Frontend |
|---|---|---|
| Login Google, access token, refresh token, logout | ✅ Jadi | Tersambung |
| Data diri mahasiswa | ✅ Baca dan ubah | Tersambung |
| Posisi magang | ✅ Baca, tambah, ubah, dan sembunyikan | Tersambung |
| Periode | ✅ Baca, tambah, mulai, dan hentikan | Tersambung |
| Perusahaan mitra | ✅ Baca, tambah, ubah nama, dan sembunyikan | Tersambung |
| Pendaftaran Panitia Magang (`POST /api/v1/panitia-magang`) | ✅ Sementara, lewat `curl` | Tidak dipakai |
| Pendaftaran mahasiswa | ✅ Kirim, daftar, detail, unduh berkas, dan konfirmasi penerimaan | Tersambung |
| Pengelolaan pendaftar oleh Panitia Magang | ✅ Baca, cari, saring, ubah status, jadwal seleksi, plotting DPM, dan ekspor Excel | Tersambung |
| Surat keterangan magang | 🟡 Dibuat dari template, ditandatangani, dan bisa diverifikasi, tapi sebagian isinya masih mockup | Tersambung: unduh di detail pendaftaran dan halaman verifikasi publik |
| Konversi mata kuliah | ✅ Katalog: baca, tambah, dan hapus (soft delete). Penyetaraan: baca, pilih, dan batalkan | Tersambung |
| Templat SP DPA | ✅ Baca dan ganti tautan | Tersambung |

### Yang masih perlu dibuat di backend

Daftar ini diturunkan dari fitur frontend yang masih mock atau tombolnya masih nonaktif. Rancangan endpoint-nya belum ditentukan.

**Panitia Magang:**
- Daftar status beserta `id_status`, karena filter `status` di `GET /api/v1/pendaftaran` memakai id sedangkan frontend hanya mengenal nama status. Frontend kini menurunkannya dari urutan seed migration

**Surat keterangan:** isi berikut masih mockup, dan ditandai `MOCK` di `src/features/surat_keterangan/templat.rs`. Surat yang sudah terbit tidak berubah saat mockup diganti, karena isinya dibekukan saat terbit.
- Tanggal mulai dan selesai magang: sementara tanggal terbit sampai 120 hari setelahnya, karena yang tersimpan baru periode pendaftaran
- Alamat perusahaan: `perusahaan_mitra` baru punya nama
- Nama dan NIP pejabat penanda tangan, serta jenis program (Magang Mandiri atau MBKM)
- Format nomor surat: sementara `{id_surat 5 digit}/SKM/MB-FILKOM/{tahun}`
- Logo fakultas di kop surat: sementara bingkai bertuliskan `LOGO FILKOM`

**Keamanan dan peran:**
- `GET /auth/me` belum mengirim peran, jadi frontend menebaknya dari domain email

### Yang sudah ada di backend, tapi belum dipakai frontend

- `GET /api/v1/surat-keterangan/kunci-publik`: disediakan untuk pihak luar yang ingin memeriksa tanda tangan sendiri

## Autentikasi

Backend memakai dua cookie httpOnly, jadi JavaScript tidak bisa membacanya dan tidak perlu mengirimnya secara manual. Header `Authorization` tidak dibaca. Kedua cookie memakai `SameSite=Lax`, dan otomatis `Secure` kalau `GOOGLE_REDIRECT_URI` memakai `https://`.

| Cookie | Masa berlaku | Path | Isi |
|---|---|---|---|
| `access_token` | 15 menit | `/` | JWT berisi `id_pengguna` dan `email` |
| `refresh_token` | 7 hari sejak login, tidak diperpanjang saat refresh | `/auth` | String acak, hanya hash-nya yang disimpan di tabel `token_refresh` |

Semua request dari frontend harus mengirim cookie, misalnya `withCredentials: true` di axios. Origin frontend juga harus terdaftar di `CORS_ALLOWED_ORIGINS`, dan CORS hanya mengizinkan method `GET`, `POST`, `PATCH`, `PUT`, dan `DELETE`, dan membuka header `Content-Disposition` supaya frontend bisa membaca nama file unduhan.

### Siapa yang boleh login

| Akun Google | Syarat | Setelah login |
|---|---|---|
| `@student.ub.ac.id` | Email terverifikasi | Baris `pengguna` dan `mahasiswa` dibuat otomatis, lalu diarahkan ke `/dasbor` |
| `@gmail.com` | Email terverifikasi dan terdaftar di tabel `panitia_magang`, misalnya lewat [`POST /api/v1/panitia-magang`](#post-apiv1panitia-magang-sementara) | Diarahkan ke `/panitia-magang` |

### Format error

Error `409` dan error dari aturan validasi `422` membawa pesan yang bisa langsung ditampilkan ke user. Error `401`, `403`, `404`, dan `500` hanya berisi status HTTP, tanpa body.

```json
{ "message": "IPK harus di antara 0 dan 4." }
```

| Status | Arti |
|---|---|
| `400` | Body bukan JSON atau multipart yang valid, atau nilai query atau path tidak valid. Body-nya teks biasa dari framework, bukan format di atas |
| `401` | Token tidak ada, tidak valid, atau kedaluwarsa. Coba `POST /auth/refresh`, lalu ulangi request |
| `403` | Token valid, tapi user tidak punya akses |
| `409` | Data bentrok dengan data lain |
| `415` | Header `Content-Type: application/json` tidak dikirim |
| `422` | Isi body ditolak. Salah tipe, `null`, atau field tak dikenal dibalas teks biasa dari framework, sedangkan aturan validasi dibalas format di atas |
| `500` | Terjadi kesalahan di server |

## Endpoint auth

### `GET /auth/google/login`

Memulai login. Buka lewat navigasi browser, bukan `fetch` atau axios.

- **Yang dilakukan:** membuat `state` anti-CSRF, menyimpannya di cookie `oauth_state` selama 10 menit, lalu mengarahkan browser ke halaman login Google.
- **Respons:** `303` ke Google.

### `GET /auth/google`

Callback yang dipanggil Google setelah user memilih akun. URL ini adalah redirect URI yang terdaftar di Google Console, dan frontend tidak perlu memanggilnya.

**Query:** `code`, `state`, `error`, semuanya dikirim oleh Google.

**Yang dilakukan:** mencocokkan `state`, menukar `code` ke Google, memeriksa akun, menyimpan session, lalu mengeset cookie `access_token` dan `refresh_token`.

**Respons:** selalu `303` ke frontend.

| Hasil | Diarahkan ke |
|---|---|
| Mahasiswa berhasil login | `/dasbor` |
| Panitia Magang berhasil login | `/panitia-magang` |
| Login dibatalkan user | `/login?error=cancelled` |
| Domain email tidak diizinkan atau email belum terverifikasi | `/login?error=domain` |
| Gmail tidak terdaftar di `panitia_magang`, atau baris itu sudah terikat ke akun Google lain | `/login?error=not_registered` |
| `state` tidak cocok, `code` ditolak, email sudah terikat ke akun Google lain, atau server error | `/login?error=failed` |

### `GET /auth/me`

Mengambil data user dari access token, tanpa query ke database. Berlaku untuk mahasiswa dan Panitia Magang.

**Respons `200`:**
```json
{
  "id_pengguna": 1,
  "email": "budi@student.ub.ac.id"
}
```

**Error:** `401` kalau access token tidak valid.

### `POST /auth/refresh`

Menukar refresh token dengan pasangan token baru. Tanpa body.

| Status | Arti |
|---|---|
| `204` | Berhasil. Cookie baru dikirim lewat `Set-Cookie`, tanpa body |
| `401` | Refresh token tidak valid atau kedaluwarsa. Cookie dihapus, dan user harus login ulang |
| `500` | Terjadi kesalahan di server. Cookie tidak dihapus, jadi request bisa dicoba lagi |

**Rotasi token:** setiap refresh menerbitkan token baru.
- Token lama masih diterima selama 10 detik, supaya beberapa tab yang refresh bersamaan tidak saling mengeluarkan.
- Lewat dari 10 detik, pemakaian token lama dianggap pencurian, jadi semua token dari login itu dicabut.
- Refresh tidak memperpanjang batas 7 hari sejak login.

### `POST /auth/logout`

Logout dari perangkat yang sedang dipakai. Tanpa body. Login di perangkat lain tetap aktif.

**Yang dilakukan:** menghapus semua baris `token_refresh` dari login itu, termasuk token lama yang sudah dirotasi, lalu menghapus kedua cookie.

| Status | Arti |
|---|---|
| `204` | Berhasil, termasuk kalau cookie tidak ada atau tokennya tidak dikenal |
| `500` | Terjadi kesalahan di server. Cookie tidak dihapus, jadi user masih login |

Access token yang sudah tersalin tetap valid sampai kedaluwarsa, maksimal 15 menit.

## Endpoint mahasiswa

### `GET /api/v1/mahasiswa/data`

Mengambil data mahasiswa milik user yang sedang login. `id_pengguna` diambil dari access token, jadi user tidak bisa meminta data orang lain. Dilindungi middleware `require_mahasiswa`.

**Respons `200`:**
```json
{
  "nim": "215150200111001",
  "email": "budi@student.ub.ac.id",
  "nama": "Budi",
  "program_studi": "Teknik Informatika",
  "ipk": 3.75,
  "no_hp": "0812",
  "jenis_kelamin": "L",
  "semester": 6
}
```

Hanya `email` yang selalu terisi. Field lain bernilai `null` selama belum diisi, termasuk tepat setelah login pertama. `jenis_kelamin` berisi `L` atau `P`.

| Status | Arti |
|---|---|
| `401` | Access token tidak valid |
| `403` | User tidak punya baris di tabel `mahasiswa`, misalnya Panitia Magang |

### `PATCH /api/v1/mahasiswa/data`

Mengubah data mahasiswa milik user yang sedang login. Kirim **hanya field yang berubah**, karena field yang tidak dikirim tidak ikut diubah. Dilindungi middleware `require_mahasiswa`.

**Header:** `Content-Type: application/json`

**Contoh body:**
```json
{
  "no_hp": "081234567890",
  "semester": 7
}
```

| Field | Tipe | Aturan |
|---|---|---|
| `nim` | string | 1-15 digit angka, tidak boleh dipakai mahasiswa lain |
| `nama` | string | 1-255 karakter, spasi di awal dan akhir dibuang |
| `program_studi` | string | `Sistem Informasi`, `Teknologi Informasi`, `Pendidikan Teknologi Informasi`, `Teknik Komputer`, atau `Teknik Informatika` |
| `ipk` | angka | 0 sampai 4, dibulatkan ke 2 angka di belakang koma |
| `no_hp` | string | 9-15 digit angka, boleh diawali `+` |
| `jenis_kelamin` | string | `L` atau `P` |
| `semester` | bilangan bulat | 1 sampai 14 |

**Yang ditolak dengan `422`:**
- Nilai `null`. Data yang sudah diisi hanya bisa diganti, tidak bisa dikosongkan.
- Field lain seperti `email` atau `id_pengguna`.

Kalau ada satu field yang tidak valid, tidak ada field lain yang ikut tersimpan. Body `{}` tidak mengubah apa pun dan tetap dibalas `200`.

**Respons `200`:** data mahasiswa terbaru, dengan bentuk yang sama seperti [`GET /api/v1/mahasiswa/data`](#get-apiv1mahasiswadata).

| Status | Arti |
|---|---|
| `401` | Access token tidak valid |
| `403` | User tidak punya baris di tabel `mahasiswa`, misalnya Panitia Magang |
| `409` | NIM sudah dipakai mahasiswa lain |
| `422` | Isi body ditolak |

## Endpoint posisi magang

### `GET /api/v1/posisi`

Mengambil daftar posisi magang dalam satu periode, diurutkan berdasarkan nama posisi.

**Query:** semuanya opsional.

| Parameter | Tipe | Fungsi |
|---|---|---|
| `p` | bilangan bulat | Hanya posisi dari `id_perusahaan` ini |
| `s` | string | Mencari di nama posisi, tanpa membedakan huruf besar dan kecil |
| `id_periode` | bilangan bulat | Periode yang ditampilkan, dengan aturan di bawah |
| `termasuk_dihapus` | `true` atau `false` | `true` untuk ikut menampilkan posisi yang disembunyikan, di bawah yang lain. Hanya untuk Panitia Magang, mahasiswa ditolak `403` |

**Aturan periode:** waktu periode dibandingkan dengan jam WIB. Kalau ada beberapa periode yang berjalan bersamaan, yang dipakai sebagai default adalah yang mulai paling akhir.

| Kondisi | Mahasiswa | Panitia Magang |
|---|---|---|
| Tanpa `id_periode`, ada periode berjalan | Periode berjalan | Periode berjalan |
| Tanpa `id_periode`, tidak ada periode berjalan | Daftar kosong `[]` | Periode yang mulai paling akhir |
| `id_periode` milik periode berjalan | Periode itu | Periode itu |
| `id_periode` lain | `403` | Periode itu, atau `[]` kalau tidak ditemukan |

Posisi yang disembunyikan lewat [`DELETE /api/v1/posisi/{id}`](#delete-apiv1posisiid) tidak ikut ditampilkan, kecuali Panitia Magang memakai `termasuk_dihapus=true`. Posisi dari perusahaan yang sudah disembunyikan lewat [`DELETE /api/v1/perusahaan/{id}`](#delete-apiv1perusahaanid) tidak pernah ditampilkan, termasuk dengan `termasuk_dihapus=true`.

Setiap item punya `deleted_at`, berisi waktu posisi disembunyikan dalam WIB, atau `null` kalau masih aktif.

`kuota` adalah jumlah mahasiswa yang akan diterima di posisi itu. Nilainya hanya informasi: tidak membatasi jumlah pendaftar dan tidak berubah saat ada yang mendaftar.

**Respons `200`:**
```json
[
  {
    "id_posisi": 1,
    "posisi": "Backend Developer Intern",
    "deskripsi_persyaratan": "Memahami REST API dan basis data.",
    "kuota": 5,
    "id_periode": 2,
    "perusahaan": {
      "id_perusahaan": 1,
      "nama_perusahaan": "PT Nusantara Digital"
    },
    "deleted_at": null
  }
]
```

| Status | Arti |
|---|---|
| `400` | Nilai query tidak valid, misalnya `p=abc`. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | Mahasiswa meminta periode yang tidak berjalan, atau user bukan mahasiswa maupun Panitia Magang, atau mahasiswa mengirim `termasuk_dihapus=true` |

### `POST /api/v1/posisi`

Membuat posisi magang baru. Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`.

**Header:** `Content-Type: application/json`

| Field | Wajib | Aturan |
|---|---|---|
| `id_perusahaan` | Ya | Harus ada di tabel `perusahaan_mitra` dan belum disembunyikan |
| `id_periode` | Ya | Harus ada di tabel `periode_pendaftaran`, boleh periode apa saja |
| `posisi` | Ya | 1-255 karakter, spasi di awal dan akhir dibuang |
| `deskripsi_persyaratan` | Ya | Tidak boleh kosong, spasi di awal dan akhir dibuang |
| `kuota` | Ya | Bilangan bulat, minimal 1. Kuota penerimaan, hanya sebagai informasi |

**Contoh body:**
```json
{
  "id_perusahaan": 1,
  "id_periode": 2,
  "posisi": "Backend Developer Intern",
  "deskripsi_persyaratan": "Memahami REST API dan basis data.",
  "kuota": 5
}
```

**Respons `201`:** posisi yang dibuat, dengan bentuk yang sama seperti satu item di [`GET /api/v1/posisi`](#get-apiv1posisi).

| Status | Arti |
|---|---|
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang |
| `422` | Isi body ditolak, termasuk `id_perusahaan` atau `id_periode` yang tidak ditemukan |

### `PATCH /api/v1/posisi/{id}`

Mengubah nama, kuota, dan deskripsi persyaratan posisi magang yang masih aktif. Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`.

Perusahaan dan periode posisi **tidak bisa diubah**, karena pendaftaran di posisi itu ikut perusahaan dan periodenya. Kalau salah pilih, sembunyikan posisi lalu buat yang baru.

**Header:** `Content-Type: application/json`

Semua field wajib dikirim, karena ketiganya diganti sekaligus.

| Field | Wajib | Aturan |
|---|---|---|
| `posisi` | Ya | Aturan yang sama seperti [`POST /api/v1/posisi`](#post-apiv1posisi): 1-255 karakter, spasi di awal dan akhir dibuang |
| `deskripsi_persyaratan` | Ya | Tidak boleh kosong, spasi di awal dan akhir dibuang |
| `kuota` | Ya | Bilangan bulat, minimal 1. Hanya informasi, tidak membatasi pendaftar |

**Contoh:**
```bash
curl -X PATCH http://localhost:3000/api/v1/posisi/1 \
  -b "access_token=$ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"posisi":"Frontend Developer Intern","deskripsi_persyaratan":"Menguasai React.","kuota":8}'
```

Perubahan langsung tampil di katalog, dan nama posisi yang baru ikut tampil di pendaftaran yang sudah ada.

**Respons `200`:** posisi yang sudah diubah, dengan bentuk yang sama seperti satu item di [`GET /api/v1/posisi`](#get-apiv1posisi).

| Status | Arti |
|---|---|
| `400` | `id` bukan angka, atau body bukan JSON yang valid. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang |
| `404` | Posisi tidak ditemukan, sudah disembunyikan, atau perusahaannya sudah disembunyikan |
| `415` | Header `Content-Type: application/json` tidak dikirim |
| `422` | Isi body ditolak, termasuk field yang tidak dikirim, serta `id_perusahaan` atau `id_periode` yang ikut dikirim. Pesan aturan seperti `Kuota minimal 1.` dikirim dalam format `{ "message": "..." }`, sedangkan field yang hilang atau tidak dikenal body-nya teks biasa dari framework |

### `DELETE /api/v1/posisi/{id}`

Menyembunyikan posisi magang (soft delete). Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`. Tanpa body.

**Contoh:**
```bash
curl -X DELETE http://localhost:3000/api/v1/posisi/1 \
  -b "access_token=$ACCESS_TOKEN"
```

**Yang dilakukan:** baris `posisi_magang` tidak dihapus, hanya `deleted_at` diisi dengan waktu saat ini (WIB). Belum ada endpoint untuk mengaktifkannya kembali.

**Akibatnya:**

| Data | Setelah disembunyikan |
|---|---|
| [`GET /api/v1/posisi`](#get-apiv1posisi) | Tidak tampil, kecuali Panitia Magang memakai `termasuk_dihapus=true` |
| Pendaftaran baru | Ditolak `422` `Posisi tidak ditemukan.` oleh [`POST /api/v1/pendaftaran`](#post-apiv1pendaftaran) |
| Pendaftaran yang sudah masuk | Tetap ada, tetap tampil dengan nama posisinya, dan tetap bisa diproses Panitia Magang |
| Ubah posisi | Ditolak `404` |

**Respons `200`:** posisi yang disembunyikan, dengan `deleted_at` terisi, dengan bentuk yang sama seperti satu item di [`GET /api/v1/posisi`](#get-apiv1posisi).

| Status | Arti |
|---|---|
| `400` | `id` bukan angka. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang |
| `404` | Posisi tidak ditemukan, sudah disembunyikan, atau perusahaannya sudah disembunyikan |

## Endpoint periode

### `GET /api/v1/periode`

Mengambil daftar periode pendaftaran, dari yang mulai paling akhir. Hasilnya selalu berupa array, termasuk saat memakai `id`.

**Query:** semuanya opsional.

| Parameter | Tipe | Fungsi |
|---|---|---|
| `id` | bilangan bulat | Hanya periode dengan `id_periode` ini |
| `status` | `belum_dimulai`, `berlangsung`, atau `selesai` | Hanya periode dengan status ini |

Status dihitung dari jam WIB saat ini: `belum_dimulai` sebelum `waktu_mulai`, `selesai` setelah `waktu_tutup`, dan `berlangsung` di antaranya.

**Aturan akses:**

| Kondisi | Mahasiswa | Panitia Magang |
|---|---|---|
| Tanpa query | Periode `berlangsung` saja | Semua periode |
| `status=berlangsung` | Periode `berlangsung` | Periode `berlangsung` |
| `status` lain | `403` | Periode dengan status itu |
| `id` periode yang berlangsung | Periode itu | Periode itu |
| `id` periode lain | Daftar kosong `[]` | Periode itu, atau `[]` kalau tidak ditemukan |

**Respons `200`:**
```json
[
  {
    "id_periode": 2,
    "catatan": "Periode Ganjil 2026/2027",
    "waktu_mulai": "2026-10-01T00:00:00+07:00",
    "waktu_tutup": "2026-11-30T23:59:00+07:00",
    "status": "berlangsung"
  }
]
```

`catatan` bisa bernilai `null`.

| Status | Arti |
|---|---|
| `400` | Nilai query tidak valid, misalnya `status=aktif` atau `id=abc`. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | Mahasiswa meminta status selain `berlangsung`, atau user bukan mahasiswa maupun Panitia Magang |

### `POST /api/v1/periode`

Membuat periode pendaftaran baru. Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`.

**Header:** `Content-Type: application/json`

| Field | Wajib | Aturan |
|---|---|---|
| `catatan` | Tidak | Maksimal 255 karakter. Spasi di awal dan akhir dibuang, dan yang kosong disimpan sebagai `null` |
| `waktu_mulai` | Ya | ISO 8601 dengan zona waktu, misalnya `2026-10-01T08:00:00+07:00` atau `2026-10-01T01:00:00.000Z` |
| `waktu_tutup` | Ya | Format sama, dan harus setelah `waktu_mulai` |

**Contoh body:**
```json
{
  "catatan": "Periode Ganjil 2026/2027",
  "waktu_mulai": "2026-10-01T00:00:00+07:00",
  "waktu_tutup": "2026-11-30T23:59:00+07:00"
}
```

Waktu dari zona mana pun disimpan dalam WIB. Rentangnya tidak boleh beririsan dengan periode yang sudah ada, termasuk kalau waktu mulai sama persis dengan waktu tutup periode lain. Periode di masa lalu tetap diterima.

**Respons `201`:** periode yang dibuat, dengan bentuk yang sama seperti satu item di [`GET /api/v1/periode`](#get-apiv1periode).

| Status | Arti |
|---|---|
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang |
| `409` | Rentang waktu beririsan dengan periode lain |
| `422` | Isi body ditolak |

### `PATCH /api/v1/periode/{id}/start`

Memulai periode yang belum dimulai saat itu juga, supaya pendaftaran langsung terbuka tanpa menunggu `waktu_mulai`. Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`. Tanpa body.

**Yang dilakukan:** mengubah `waktu_mulai` menjadi waktu saat ini dalam WIB. `waktu_tutup` dan `catatan` tidak berubah.

Periode hanya bisa dimulai kalau statusnya `belum_dimulai`. Rentang barunya, dari saat ini sampai `waktu_tutup`, juga tidak boleh beririsan dengan periode lain. Jadi periode ini ditolak kalau ada periode lain yang sedang berlangsung, atau yang dijadwalkan sebelum `waktu_mulai` lamanya.

**Contoh:**
```bash
curl -X PATCH http://localhost:3000/api/v1/periode/2/start \
  -b "access_token=$ACCESS_TOKEN"
```

**Respons `200`:** periode yang sudah dimulai, dengan bentuk yang sama seperti satu item di [`GET /api/v1/periode`](#get-apiv1periode), dan `status` berisi `berlangsung`.

| Status | Arti |
|---|---|
| `400` | `id` bukan angka. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang |
| `404` | Periode tidak ditemukan |
| `409` | Periode sudah berlangsung, sudah selesai, atau rentang barunya beririsan dengan periode lain |

### `PATCH /api/v1/periode/{id}/stop`

Menghentikan periode yang sedang berlangsung saat itu juga, supaya pendaftaran langsung tertutup tanpa menunggu `waktu_tutup`. Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`. Tanpa body.

**Yang dilakukan:** mengubah `waktu_tutup` menjadi waktu saat ini dalam WIB. `waktu_mulai` dan `catatan` tidak berubah.

Periode hanya bisa dihentikan kalau statusnya `berlangsung`.

**Setelah dihentikan:**
- Mahasiswa tidak bisa lagi melihat periode dan posisinya, dan tidak bisa mendaftar ke posisi di periode itu.
- Pendaftaran yang sudah masuk tetap ada dan tidak berubah.
- Periode tidak bisa dimulai lagi lewat [`PATCH /api/v1/periode/{id}/start`](#patch-apiv1periodeidstart), karena statusnya sudah `selesai`.

**Contoh:**
```bash
curl -X PATCH http://localhost:3000/api/v1/periode/2/stop \
  -b "access_token=$ACCESS_TOKEN"
```

**Respons `200`:** periode yang sudah dihentikan, dengan bentuk yang sama seperti satu item di [`GET /api/v1/periode`](#get-apiv1periode), dan `status` berisi `selesai`.

| Status | Arti |
|---|---|
| `400` | `id` bukan angka. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang |
| `404` | Periode tidak ditemukan |
| `409` | Periode belum dimulai atau sudah selesai |

## Endpoint perusahaan

### `GET /api/v1/perusahaan`

Mengambil perusahaan mitra yang tidak disembunyikan, diurutkan berdasarkan nama. Bisa dipakai mahasiswa dan Panitia Magang, misalnya untuk mengisi filter `p` di [`GET /api/v1/posisi`](#get-apiv1posisi).

**Query:**

| Parameter | Wajib | Aturan |
|---|---|---|
| `termasuk_dihapus` | Tidak | `true` untuk ikut menampilkan perusahaan yang disembunyikan, di bawah yang lain. Hanya untuk Panitia Magang |

**Respons `200`:**
```json
[
  {
    "id_perusahaan": 1,
    "nama_perusahaan": "PT Nusantara Digital",
    "deleted_at": null
  },
  {
    "id_perusahaan": 2,
    "nama_perusahaan": "PT Lama",
    "deleted_at": "2026-11-10T09:30:00+07:00"
  }
]
```

`deleted_at` berisi waktu perusahaan disembunyikan lewat [`DELETE /api/v1/perusahaan/{id}`](#delete-apiv1perusahaanid), atau `null` kalau masih aktif. Tanpa `termasuk_dihapus=true`, semua item bernilai `null`.

| Status | Arti |
|---|---|
| `400` | Nilai `termasuk_dihapus` bukan `true` atau `false`, atau ada query lain. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan mahasiswa maupun Panitia Magang, atau mahasiswa mengirim `termasuk_dihapus=true` |

### `POST /api/v1/perusahaan`

Membuat perusahaan mitra baru. Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`.

**Header:** `Content-Type: application/json`

| Field | Wajib | Aturan |
|---|---|---|
| `nama_perusahaan` | Ya | 1-255 karakter, spasi di awal dan akhir dibuang. Tidak boleh sama dengan perusahaan lain yang masih aktif, tanpa membedakan huruf besar dan kecil. Nama perusahaan yang sudah disembunyikan boleh dipakai lagi |

**Contoh body:**
```json
{
  "nama_perusahaan": "PT Nusantara Digital"
}
```

**Respons `201`:** perusahaan yang dibuat, dengan bentuk yang sama seperti satu item di [`GET /api/v1/perusahaan`](#get-apiv1perusahaan).

| Status | Arti |
|---|---|
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang |
| `409` | Nama perusahaan sudah ada |
| `422` | Isi body ditolak |

### `PATCH /api/v1/perusahaan/{id}`

Mengubah nama perusahaan mitra yang masih aktif. Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`. Nama baru ikut tampil di semua posisi dan pendaftaran perusahaan itu, karena keduanya hanya menyimpan `id_perusahaan`.

**Header:** `Content-Type: application/json`

| Field | Wajib | Aturan |
|---|---|---|
| `nama_perusahaan` | Ya | Aturan yang sama seperti [`POST /api/v1/perusahaan`](#post-apiv1perusahaan). Nama sendiri dengan huruf besar-kecil yang berbeda boleh |

**Contoh:**
```bash
curl -X PATCH http://localhost:3000/api/v1/perusahaan/1 \
  -b "access_token=$ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"nama_perusahaan":"PT Nusantara Digital Indonesia"}'
```

**Respons `200`:** perusahaan dengan nama barunya, dengan bentuk yang sama seperti satu item di [`GET /api/v1/perusahaan`](#get-apiv1perusahaan).

| Status | Arti |
|---|---|
| `400` | `id` bukan angka, atau body bukan JSON yang valid. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang |
| `404` | Perusahaan tidak ditemukan, atau sudah disembunyikan |
| `409` | Nama perusahaan sudah dipakai perusahaan aktif lain |
| `415` | Header `Content-Type: application/json` tidak dikirim |
| `422` | Isi body ditolak |

### `DELETE /api/v1/perusahaan/{id}`

Menyembunyikan perusahaan mitra (soft delete). Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`. Tanpa body.

**Contoh:**
```bash
curl -X DELETE http://localhost:3000/api/v1/perusahaan/1 \
  -b "access_token=$ACCESS_TOKEN"
```

**Yang dilakukan:** baris `perusahaan_mitra` tidak dihapus, hanya `deleted_at` diisi dengan waktu saat ini (WIB). Belum ada endpoint untuk mengaktifkannya kembali.

**Akibatnya:**

| Data | Setelah disembunyikan |
|---|---|
| [`GET /api/v1/perusahaan`](#get-apiv1perusahaan) | Tidak tampil, kecuali Panitia Magang memakai `termasuk_dihapus=true` |
| Posisi magang perusahaan itu | Hilang dari [`GET /api/v1/posisi`](#get-apiv1posisi) untuk semua user, dan tidak bisa didaftar lagi: [`POST /api/v1/pendaftaran`](#post-apiv1pendaftaran) menjawab `422` `Posisi tidak ditemukan.` |
| Posisi baru untuk perusahaan itu | Ditolak `422` oleh [`POST /api/v1/posisi`](#post-apiv1posisi) |
| Pendaftaran yang sudah masuk | Tetap ada, tetap tampil dengan nama perusahaannya, dan tetap bisa diproses Panitia Magang |
| Nama perusahaan | Boleh dipakai perusahaan baru |
| Ubah nama | Ditolak `404` |

**Respons `200`:** perusahaan yang disembunyikan, dengan `deleted_at` terisi, dengan bentuk yang sama seperti satu item di [`GET /api/v1/perusahaan`](#get-apiv1perusahaan).

| Status | Arti |
|---|---|
| `400` | `id` bukan angka. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang |
| `404` | Perusahaan tidak ditemukan, atau sudah disembunyikan |

## Endpoint konversi mata kuliah

Katalog mata kuliah konversi dikelola Panitia Magang. Mahasiswa memilih mata kuliah dari katalog itu untuk pendaftarannya yang berstatus `Aktif` (penyetaraan).

### `GET /api/v1/mata-kuliah`

Mengambil mata kuliah yang belum dihapus, diurutkan berdasarkan `mk_konversi`, lalu `mk_asal`. Bisa dipakai mahasiswa dan Panitia Magang.

**Query:**

| Parameter | Wajib | Aturan |
|---|---|---|
| `termasuk_dihapus` | Tidak | `true` untuk ikut menampilkan mata kuliah yang sudah dihapus, di bawah yang lain. Hanya untuk Panitia Magang |

**Respons `200`:**
```json
[
  {
    "id_mk_konversi": 1,
    "mk_asal": "Pengembangan Antarmuka Web",
    "mk_konversi": "Pemrograman Web Lanjut",
    "sks": 3,
    "deleted_at": null
  }
]
```

`deleted_at` berisi waktu mata kuliah dihapus lewat [`DELETE /api/v1/mata-kuliah/{id}`](#delete-apiv1mata-kuliahid), atau `null` kalau masih aktif.

| Status | Arti |
|---|---|
| `400` | Nilai `termasuk_dihapus` bukan `true` atau `false`, atau ada query lain. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan mahasiswa maupun Panitia Magang, atau mahasiswa mengirim `termasuk_dihapus=true` |

### `POST /api/v1/mata-kuliah`

Menambah mata kuliah konversi. Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`.

**Header:** `Content-Type: application/json`

| Field | Wajib | Aturan |
|---|---|---|
| `mk_asal` | Ya | 1-255 karakter, spasi di awal dan akhir dibuang |
| `mk_konversi` | Ya | 1-255 karakter, spasi di awal dan akhir dibuang |
| `sks` | Ya | Bilangan bulat 1-24 |

Pasangan `mk_asal` dan `mk_konversi` tidak boleh sama dengan mata kuliah lain yang masih aktif, tanpa membedakan huruf besar dan kecil. Pasangan milik mata kuliah yang sudah dihapus boleh dipakai lagi.

**Contoh body:**
```json
{
  "mk_asal": "Pengembangan Antarmuka Web",
  "mk_konversi": "Pemrograman Web Lanjut",
  "sks": 3
}
```

**Respons `201`:** mata kuliah yang dibuat, dengan bentuk yang sama seperti satu item di [`GET /api/v1/mata-kuliah`](#get-apiv1mata-kuliah).

| Status | Arti |
|---|---|
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang |
| `409` | Pasangan MK asal dan MK konversi sudah ada |
| `422` | Isi body ditolak |

### `DELETE /api/v1/mata-kuliah/{id}`

Menghapus mata kuliah (soft delete). Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`. Tanpa body.

**Yang dilakukan:** baris `mata_kuliah_konversi` tidak dihapus, hanya `deleted_at` diisi dengan waktu saat ini (WIB). Belum ada endpoint untuk mengaktifkannya kembali.

**Akibatnya:**

| Data | Setelah dihapus |
|---|---|
| [`GET /api/v1/mata-kuliah`](#get-apiv1mata-kuliah) | Tidak tampil, kecuali Panitia Magang memakai `termasuk_dihapus=true` |
| Pilihan baru | Ditolak `422` `Mata kuliah tidak ditemukan.` oleh [`POST /api/v1/pendaftaran/{id}/penyetaraan-mk`](#post-apiv1pendaftaranidpenyetaraan-mk) |
| Penyetaraan yang sudah ada | Tetap ada dan tetap tampil, dengan `mata_kuliah.deleted_at` terisi. Mahasiswa tetap bisa membatalkannya |
| Pasangan MK asal dan MK konversi | Boleh dipakai mata kuliah baru |

**Respons `200`:** mata kuliah yang dihapus, dengan `deleted_at` terisi.

| Status | Arti |
|---|---|
| `400` | `id` bukan angka. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang |
| `404` | Mata kuliah tidak ditemukan, atau sudah dihapus |

### `GET /api/v1/pendaftaran/{id}/penyetaraan-mk`

Mengambil mata kuliah yang dipilih untuk satu pendaftaran, urut waktu dipilih. Mahasiswa hanya boleh melihat pendaftaran miliknya, dan Panitia Magang boleh melihat semua. Mata kuliah yang sudah dihapus dari katalog tetap ikut tampil.

**Respons `200`:**
```json
[
  {
    "id_penyetaraan": 4,
    "id_pendaftaran": 12,
    "created_at": "2026-11-10T10:15:00+07:00",
    "mata_kuliah": {
      "id_mk_konversi": 1,
      "mk_asal": "Pengembangan Antarmuka Web",
      "mk_konversi": "Pemrograman Web Lanjut",
      "sks": 3,
      "deleted_at": null
    }
  }
]
```

| Status | Arti |
|---|---|
| `400` | `id` bukan angka. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan mahasiswa maupun Panitia Magang, atau pendaftaran milik mahasiswa lain |
| `404` | Pendaftaran tidak ditemukan |

### `POST /api/v1/pendaftaran/{id}/penyetaraan-mk`

Memilih satu mata kuliah untuk pendaftaran. Hanya untuk mahasiswa pemilik pendaftaran, dilindungi middleware `require_mahasiswa`, dan pendaftarannya harus berstatus `Aktif`. Satu mata kuliah hanya bisa dipilih sekali per pendaftaran (constraint `UNIQUE (id_pendaftaran, id_mk_konversi)`).

**Header:** `Content-Type: application/json`

| Field | Wajib | Aturan |
|---|---|---|
| `id_mk_konversi` | Ya | Mata kuliah yang belum dihapus |

**Respons `201`:** penyetaraan yang dibuat, dengan bentuk yang sama seperti satu item di [`GET /api/v1/pendaftaran/{id}/penyetaraan-mk`](#get-apiv1pendaftaranidpenyetaraan-mk).

| Status | Arti |
|---|---|
| `401` | Access token tidak valid |
| `403` | User bukan mahasiswa, atau pendaftaran milik mahasiswa lain |
| `404` | Pendaftaran tidak ditemukan |
| `409` | Pendaftaran tidak berstatus `Aktif`, atau mata kuliah sudah dipilih |
| `422` | Isi body ditolak, atau mata kuliah tidak ditemukan atau sudah dihapus |

### `DELETE /api/v1/penyetaraan-mk/{id}`

Membatalkan satu pilihan mata kuliah. Barisnya dihapus permanen. Hanya untuk mahasiswa pemilik pendaftaran, dilindungi middleware `require_mahasiswa`, dan pendaftarannya harus berstatus `Aktif`.

**Respons `204`:** tanpa body.

| Status | Arti |
|---|---|
| `400` | `id` bukan angka. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan mahasiswa, atau pendaftaran milik mahasiswa lain |
| `404` | Penyetaraan tidak ditemukan |
| `409` | Pendaftaran tidak berstatus `Aktif` |

## Endpoint pendaftaran

### `GET /api/v1/pendaftaran`

Mengambil daftar pendaftaran, dari yang paling baru. Semua status ikut ditampilkan.

| User | Yang terlihat |
|---|---|
| Mahasiswa | Pendaftaran miliknya sendiri |
| Panitia Magang | Semua pendaftaran dari semua mahasiswa |

**Query:** semuanya opsional, dan berlaku sama untuk mahasiswa dan Panitia Magang.

| Parameter | Tipe | Nilai yang diterima | Bawaan | Fungsi |
|---|---|---|---|---|
| `p` | bilangan bulat | 1 sampai 2147483647 | `1` | Halaman yang diambil (page) |
| `r` | bilangan bulat | 1 sampai 2147483647 | `25` | Jumlah baris per halaman (rows) |
| `periode` | bilangan bulat | `id_periode` | Semua periode | Hanya pendaftaran di posisi pada periode ini |
| `status` | bilangan bulat | `id_status` dari tabel `status_pendaftaran`, bukan nama status | Semua status | Hanya pendaftaran dengan status ini |
| `perusahaan` | bilangan bulat | `id_perusahaan` | Semua perusahaan | Hanya pendaftaran di posisi pada perusahaan ini |
| `posisi` | bilangan bulat | `id_posisi` | Semua posisi | Hanya pendaftaran di posisi magang ini |
| `s` | teks | Bagian dari nama mahasiswa | Semua nama | Mencari pendaftaran menurut nama mahasiswa (search) |

**Aturan filter:**
- Beberapa filter yang dikirim bersamaan harus terpenuhi semuanya.
- `total` dan `total_pages` dihitung dari hasil yang sudah disaring.
- Id yang tidak ada bukan error: responsnya tetap `200` dengan `data` kosong.
- Mahasiswa tetap hanya melihat miliknya sendiri, apa pun filternya.

**Aturan pencarian `s`:**
- Cocok dengan bagian mana pun dari nama, tanpa membedakan huruf besar dan kecil. `?s=budi` menemukan `Budi Santoso` dan `Abu Budiman`.
- Spasi di awal dan akhir dibuang. Kalau isinya kosong, seperti `?s=` atau `?s=%20`, pencarian diabaikan.
- Spasi di tengah harus ditulis sebagai `%20` atau `+`, misalnya `?s=budi%20santoso`. Urutan katanya harus sama seperti di nama.
- `%` dan `_` dicari sebagai karakter biasa, bukan wildcard.

**Kombinasi query:**

| Query | Hasil | Status |
|---|---|---|
| Tanpa query | Halaman 1, 25 baris, tanpa filter | `200` |
| `?p=2` | Halaman 2, 25 baris | `200` |
| `?r=50` | Halaman 1, 50 baris | `200` |
| `?p=2&r=25` | Baris ke-26 sampai ke-50 | `200` |
| `?status=1` | Halaman 1 dari pendaftaran dengan `id_status` 1 | `200` |
| `?periode=2&status=1&perusahaan=3&p=2&r=5` | Halaman 2 dari pendaftaran yang memenuhi ketiga filter, 5 baris | `200` |
| `?s=budi` | Halaman 1 dari pendaftaran mahasiswa yang namanya mengandung `budi` | `200` |
| `?s=budi&status=1&p=2&r=5` | Halaman 2 dari pendaftaran yang cocok dengan pencarian dan filter, 5 baris | `200` |
| `?posisi=4&status=2` | Halaman 1 dari pendaftaran di posisi dengan `id_posisi` 4 yang berstatus `id_status` 2 | `200` |
| `?status=999` | `data` kosong, `total` 0, dan `total_pages` 0 | `200` |
| `?p=0` atau `?r=0` | Ditolak, karena nilainya kurang dari 1 | `422` |
| `?status=Diterima`, `?p=abc`, atau `?status=` | Ditolak, karena bukan bilangan bulat | `400` |
| `?page=1` | Ditolak, karena nama query lain tidak dikenal | `400` |

Untuk mengambil semua baris, baca `total` lalu minta lagi dengan `r` sebesar itu, atau ambil per halaman sampai `total_pages`.

**Cara menghitung halaman:** `total_pages` adalah `total` dibagi `r`, dibulatkan ke atas. Misalnya dengan 201 pendaftaran dan `r=10`, ada 21 halaman, dengan 10 baris di halaman 1 sampai 20 dan 1 baris di halaman 21. Halaman di atas `total_pages` tetap `200`, dengan `data` kosong.

Urutan baris selalu dari yang paling baru, jadi halaman 1 berisi pendaftaran terbaru. Pendaftaran yang masuk di antara dua permintaan menggeser baris ke halaman berikutnya, sehingga satu baris bisa muncul lagi di halaman selanjutnya.

**Contoh:**
```bash
# Halaman 1, 25 baris
curl "http://localhost:3000/api/v1/pendaftaran" \
  -b "access_token=$ACCESS_TOKEN"

# Halaman 2, 50 baris, status dengan id 1 di perusahaan dengan id 3
curl "http://localhost:3000/api/v1/pendaftaran?p=2&r=50&status=1&perusahaan=3" \
  -b "access_token=$ACCESS_TOKEN"
```

**Respons `200`:**
```json
{
  "data": [
    {
      "id_pendaftaran": 1,
      "status": "Menunggu Verifikasi",
      "created_at": "2026-11-04T17:00:00+07:00",
      "updated_at": "2026-11-04T17:00:00+07:00",
      "id_posisi": 1,
      "posisi": "Backend Developer Intern",
      "id_periode": 2,
      "perusahaan": {
        "id_perusahaan": 1,
        "nama_perusahaan": "PT Nusantara Digital"
      },
      "batas_konfirmasi": null,
      "ada_jadwal_seleksi": false,
      "ada_plotting_dpm": false,
      "mahasiswa": {
        "id_pengguna": 12,
        "nim": "215150200111001",
        "nama": "Budi"
      }
    }
  ],
  "page": 2,
  "rows": 10,
  "total": 201,
  "total_pages": 21
}
```

| Field | Arti |
|---|---|
| `data` | Baris pada halaman ini, dari yang paling baru |
| `page` | Halaman yang dikirim |
| `rows` | Jumlah baris per halaman yang diminta. `data` bisa lebih sedikit pada halaman terakhir |
| `total` | Jumlah seluruh pendaftaran yang cocok dengan filter dan bisa dilihat user ini, dari semua halaman |
| `total_pages` | `total` dibagi `rows`, dibulatkan ke atas. `0` kalau tidak ada data |

Field setiap baris di `data`:

| Field | Arti |
|---|---|
| `batas_konfirmasi` | Hanya terisi saat status `Diterima`: batas waktu mahasiswa mengonfirmasi penerimaan, yaitu 12 jam setelah status menjadi `Diterima`, dalam format `YYYY-MM-DDTHH:MM:SS+07:00`. Lewat dari itu, pendaftaran dibatalkan otomatis (lihat [Pembatalan otomatis](#pembatalan-otomatis-konfirmasi-penerimaan)). `null` pada status lain. Field ini juga ada di detail |
| `ada_jadwal_seleksi` | `true` kalau jadwal seleksi substansi sudah diisi, lewat [`PUT /api/v1/pendaftaran/jadwal-seleksi-substansi`](#put-apiv1pendaftaranjadwal-seleksi-substansi) |
| `ada_plotting_dpm` | `true` kalau plotting DPM sudah diisi, lewat [`PUT /api/v1/pendaftaran/plot-dpm`](#put-apiv1pendaftaranplot-dpm) |

Kedua field ini hanya ada di daftar. Isi lengkapnya ada di `jadwal_seleksi` dan `plotting_dpm` pada [`GET /api/v1/pendaftaran/{id}`](#get-apiv1pendaftaranid). Nilainya tetap `true` walaupun statusnya sudah berpindah, karena datanya tetap tersimpan.

| Status | Arti |
|---|---|
| `400` | Nilai query kosong, bukan bilangan bulat, atau di luar jangkauan, atau ada query lain yang dikirim. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan mahasiswa maupun Panitia Magang |
| `422` | `p` atau `r` kurang dari 1, dengan pesan `p dan r minimal 1.` |

### `GET /api/v1/pendaftaran/{id}`

Mengambil detail satu pendaftaran, dengan aturan akses yang sama: mahasiswa hanya miliknya sendiri, Panitia Magang semuanya.

**Respons `200`:** semua field setiap baris di [`GET /api/v1/pendaftaran`](#get-apiv1pendaftaran), kecuali `ada_jadwal_seleksi` dan `ada_plotting_dpm`, dengan `mahasiswa` yang lebih lengkap, ditambah data tahap pendaftaran.

```json
{
  "id_pendaftaran": 1,
  "status": "Diterima",
  "created_at": "2026-11-04T17:00:00+07:00",
  "updated_at": "2026-11-04T17:00:00+07:00",
  "id_posisi": 1,
  "posisi": "Backend Developer Intern",
  "id_periode": 2,
  "perusahaan": {
    "id_perusahaan": 1,
    "nama_perusahaan": "PT Nusantara Digital"
  },
  "batas_konfirmasi": "2026-11-05T05:00:00+07:00",
  "mahasiswa": {
    "id_pengguna": 12,
    "nim": "215150200111001",
    "email": "budi@student.ub.ac.id",
    "nama": "Budi",
    "program_studi": "Teknik Informatika",
    "ipk": 3.75,
    "no_hp": "081234567890",
    "jenis_kelamin": "L",
    "semester": 6
  },
  "berkas": [
    {
      "id_berkas": 1,
      "jenis_berkas": "Surat Pernyataan DPA",
      "nama_berkas": "12_surat-dpa_3f2a9c0d6b1e47a8905cd2e17f4b8a61.pdf",
      "ukuran_berkas": 182340
    }
  ],
  "log": [
    {
      "id_log": 1,
      "status_lama": null,
      "status_baru": "Menunggu Verifikasi",
      "created_at": "2026-11-04T17:00:00+07:00"
    }
  ],
  "jadwal_seleksi": {
    "tanggal": "2026-11-05",
    "jam": "13:30",
    "lokasi": "Daring (Zoom)"
  },
  "surat_keterangan": {
    "nama_berkas": "surat-keterangan_1_8b1d4e7f20a94c36b57e1f09d3c6a2e4.pdf",
    "ukuran_berkas": 5554,
    "nomor_surat": "00001/SKM/MB-FILKOM/2026"
  },
  "plotting_dpm": {
    "nama_dpm": "Dr. Rina Kusuma",
    "kontak_dpm": "rina.kusuma@ub.ac.id"
  }
}
```

- `log` diurutkan dari yang paling lama, dan `status_lama` bernilai `null` pada baris pertama.
- `jadwal_seleksi`, `surat_keterangan`, dan `plotting_dpm` bernilai `null` selama belum diisi. `surat_keterangan` terisi setelah surat pertama kali diunduh.
- `id_berkas` dipakai untuk [mengunduh berkas](#get-apiv1berkasid_berkas). Surat keterangan diunduh lewat [`GET /api/v1/pendaftaran/{id}/surat-keterangan`](#get-apiv1pendaftaranidsurat-keterangan).

| Status | Arti |
|---|---|
| `400` | `id` bukan angka. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | Pendaftaran milik mahasiswa lain, atau user bukan mahasiswa maupun Panitia Magang |
| `404` | Pendaftaran tidak ditemukan |

### `POST /api/v1/pendaftaran`

Mendaftarkan mahasiswa yang sedang login ke satu posisi magang. Dilindungi middleware `require_mahasiswa`, dan `id_pengguna` diambil dari access token.

**Header:** `Content-Type: multipart/form-data`. Kalau memakai `FormData` di browser atau axios, jangan set header ini secara manual, supaya `boundary`-nya terisi otomatis.

| Field | Wajib | Aturan |
|---|---|---|
| `id_posisi` | Ya | Bilangan bulat, posisi harus ada |
| `surat_dpa` | Ya | File PDF, maksimal 2 MB |
| `cv` | Ya | File PDF, maksimal 2 MB |
| `berkas_tambahan` | Tidak | Portfolio dan berkas tambahan. File PDF, maksimal 2 MB per file. Kirim field yang sama berulang kali untuk beberapa file, maksimal 5 file. Bagian yang kosong, seperti dari input file yang tidak diisi, dilewati |

Format PDF diperiksa dari isi file, yaitu harus diawali `%PDF-`, bukan dari nama atau tipe yang dikirim browser. Field lain ditolak, begitu juga `id_posisi`, `surat_dpa`, dan `cv` yang dikirim dua kali.

Setiap berkas tambahan disimpan sebagai satu baris `berkas_pendaftaran` dengan `jenis_berkas` `Portfolio dan Berkas Tambahan`, setelah Surat Pernyataan DPA dan CV, dalam urutan yang dikirim. Berkas tambahan ikut muncul di `berkas` pada respons dan di [`GET /api/v1/pendaftaran/{id}`](#get-apiv1pendaftaranid), dan bisa diunduh lewat [`GET /api/v1/berkas/{id_berkas}`](#get-apiv1berkasid_berkas) seperti berkas lain.

**Contoh:**
```bash
curl -X POST http://localhost:3000/api/v1/pendaftaran \
  -b "access_token=$ACCESS_TOKEN" \
  -F "id_posisi=1" \
  -F "surat_dpa=@surat.pdf" \
  -F "cv=@cv.pdf" \
  -F "berkas_tambahan=@portfolio.pdf" \
  -F "berkas_tambahan=@sertifikat.pdf"
```

**Aturan pendaftaran:**
- Data diri mahasiswa harus sudah lengkap.
- Periode posisi harus sedang berlangsung, menurut jam WIB.
- Kuota posisi tidak diperiksa. Kuota adalah jumlah yang akan diterima, bukan batas pendaftar, jadi pendaftar boleh lebih banyak dari kuotanya.
- Mahasiswa tidak boleh punya pendaftaran `Aktif`. Setelah pendaftaran itu `Selesai`, mahasiswa boleh mendaftar lagi.
- Mahasiswa tidak boleh punya pendaftaran yang masih berjalan di posisi yang sama.
- Dalam satu periode, mahasiswa maksimal mendaftar 2 kali. Semua pendaftaran di periode itu ikut dihitung apa pun statusnya, termasuk yang `Batal/Mengundurkan Diri`, `Tidak Lolos`, `Selesai`, atau yang posisinya sudah disembunyikan. Di periode berikutnya, jatahnya kembali 2.

Pendaftaran dianggap berjalan selama statusnya bukan `Selesai`, `Tidak Lolos`, atau `Batal/Mengundurkan Diri`. Semua aturan ini tetap berlaku untuk request yang masuk bersamaan.

**Yang dilakukan:** dalam satu transaksi, membuat baris `pendaftaran` berstatus `Menunggu Verifikasi`, `log_status` pertama, dan satu baris `berkas_pendaftaran` untuk setiap berkas. Kalau file gagal ditulis, semua perubahan itu dibatalkan.

**Penyimpanan berkas:** file disimpan di folder `uploads/`, atau di `UPLOAD_DIR` kalau env itu di-set. Folder ini sementara, tidak ikut git, dan tidak disajikan langsung oleh route mana pun. `nama_berkas` dibuat server dengan format `{id_pengguna}_{jenis}_{acak}.pdf`, dan nama asli dari mahasiswa tidak disimpan.

**Respons `201`:**
```json
{
  "id_pendaftaran": 1,
  "id_posisi": 1,
  "status": "Menunggu Verifikasi",
  "created_at": "2026-11-04T17:00:00+07:00",
  "updated_at": "2026-11-04T17:00:00+07:00",
  "berkas": [
    {
      "id_berkas": 1,
      "jenis_berkas": "Surat Pernyataan DPA",
      "nama_berkas": "12_surat-dpa_3f2a9c0d6b1e47a8905cd2e17f4b8a61.pdf",
      "ukuran_berkas": 182340
    },
    {
      "id_berkas": 2,
      "jenis_berkas": "CV",
      "nama_berkas": "12_cv_8b1d4e7f20a94c36b57e1f09d3c6a2e4.pdf",
      "ukuran_berkas": 356912
    }
  ]
}
```

| Status | Arti |
|---|---|
| `400` | Body bukan `multipart/form-data`. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User tidak punya baris di tabel `mahasiswa`, misalnya Panitia Magang |
| `409` | Periode tidak berlangsung, sudah punya pendaftaran `Aktif`, masih punya pendaftaran berjalan di posisi itu, atau sudah mendaftar 2 kali di periode posisi itu |
| `422` | Data diri belum lengkap, posisi tidak ditemukan, atau isi form ditolak |
| `500` | Terjadi kesalahan di server, dan tidak ada data yang tersimpan |

### `PATCH /api/v1/pendaftaran`

Mengubah status satu pendaftaran. Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`.

**Header:** `Content-Type: application/json`

**Query:**

| Parameter | Wajib | Aturan |
|---|---|---|
| `id` | Ya | `id_pendaftaran` yang diubah, bilangan bulat |

**Body:**

| Field | Wajib | Aturan |
|---|---|---|
| `status` | Ya | Nama status, persis seperti di tabel `status_pendaftaran` |

**Contoh:**
```bash
curl -X PATCH "http://localhost:3000/api/v1/pendaftaran?id=1" \
  -b "access_token=$ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"status":"Lolos Seleksi Administrasi"}'
```

**Perubahan yang diizinkan:**

| Dari | Ke |
|---|---|
| `Menunggu Verifikasi` | `Lolos Seleksi Administrasi` atau `Tidak Lolos` |
| `Lolos Seleksi Administrasi` | `Diterima` atau `Tidak Lolos` |
| `Aktif` | `Selesai` |

Perubahan lain ditolak `409`, termasuk ke status yang sama. `Diterima` ke `Aktif` bukan untuk Panitia Magang, karena dilakukan mahasiswa lewat [konfirmasi penerimaan](#patch-apiv1pendaftaranaccept). `Selesai`, `Tidak Lolos`, dan `Batal/Mengundurkan Diri` adalah status akhir.

**Yang dilakukan:** dalam satu transaksi, mengubah status dan `updated_at`, lalu menambah baris `log_status` yang mencatat status lama, status baru, dan Panitia Magang yang mengubah. Status tetap bisa diubah setelah periodenya selesai.

**Email pemberitahuan:** setelah status berhasil diubah, mahasiswa pemilik pendaftaran dikirimi email ke alamat akun Google-nya. Subjeknya `Status pendaftaran magang: <status baru>`. Isinya HTML bergaya UI frontend: logo GoMagang di atas, ikon centang hijau (Lolos Seleksi Administrasi, Diterima, Aktif, Selesai) atau silang merah (Tidak Lolos, Batal/Mengundurkan Diri), kartu posisi dan perusahaan dengan badge status baru serta status sebelumnya, langkah berikutnya sesuai status, dan tombol ke `FRONTEND_URL/dasbor/{id_pendaftaran}`. Logo dan ikon ikut di dalam email sebagai gambar inline PNG, dan versi teks biasa disertakan untuk klien yang tidak menampilkan HTML. Templatnya di `templates/email/perubahan_status.html`, dan gambarnya di `assets/email/`. Email dikirim di latar belakang, sehingga respons tidak menunggu pengiriman, dan kegagalan kirim hanya dicatat di log server, tanpa membatalkan perubahan status. Perubahan yang ditolak (`404`, `409`, dst.) tidak mengirim email. Konfirmasi penerimaan oleh mahasiswa sendiri juga tidak mengirim email.

Khusus perubahan ke `Diterima`, subjeknya `Status pendaftaran magang: Diterima (konfirmasi dalam 1 x 12 jam)`, tombolnya menjadi "Konfirmasi penerimaan", dan ada kotak peringatan merah bahwa konfirmasi wajib dilakukan paling lambat pada `batas_konfirmasi` (ditulis seperti "Rabu, 11 November 2026 pukul 20.15 WIB"), dan jika lewat, pendaftaran otomatis dinyatakan `Batal/Mengundurkan Diri`.

**Konfigurasi email:** semua lewat env. Tanpa `SMTP_HOST`, tidak ada email yang dikirim dan endpoint tetap berjalan seperti biasa.

| Env | Wajib | Keterangan |
|---|---|---|
| `SMTP_HOST` | Untuk mengaktifkan email | Host server SMTP, misalnya `smtp.gmail.com` |
| `SMTP_PORT` | Tidak | Default mengikuti `SMTP_TLS`: 587 untuk `starttls`, 465 untuk `tls`, 25 untuk `none` |
| `SMTP_TLS` | Tidak | `starttls` (default), `tls`, atau `none` (hanya untuk server uji lokal seperti Mailpit) |
| `SMTP_USERNAME` | Tidak | Username SMTP. Tanpa ini, server dihubungi tanpa login |
| `SMTP_PASSWORD` | Tidak | Password SMTP, misalnya App Password untuk Gmail |
| `SMTP_FROM` | Ya, kalau `SMTP_HOST` di-set | Pengirim, misalnya `Panitia Magang <magang@example.com>` |

Nilai `SMTP_FROM`, `SMTP_PORT`, atau `SMTP_TLS` yang formatnya salah membuat server berhenti saat start.

**Respons `200`:** detail pendaftaran terbaru, dengan bentuk yang sama seperti [`GET /api/v1/pendaftaran/{id}`](#get-apiv1pendaftaranid).

| Status | Arti |
|---|---|
| `400` | `id` tidak dikirim atau bukan angka, atau body bukan JSON yang valid. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang, termasuk mahasiswa pemilik pendaftaran |
| `404` | Pendaftaran tidak ditemukan |
| `409` | Perubahan status ini tidak diizinkan |
| `415` | Header `Content-Type: application/json` tidak dikirim |
| `422` | Isi body ditolak, termasuk nama status yang tidak dikenal |

### `PUT /api/v1/pendaftaran/jadwal-seleksi-substansi`

Menyimpan satu jadwal seleksi substansi yang sama untuk satu atau banyak pendaftaran, untuk aksi massal maupun satu pendaftaran. Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`.

**Header:** `Content-Type: application/json`

**Body:**

| Field | Wajib | Aturan |
|---|---|---|
| `id_pendaftaran` | Ya | Array bilangan bulat, minimal 1. Id yang sama cukup dihitung sekali |
| `tanggal` | Ya | Format `YYYY-MM-DD`, tidak boleh sebelum hari ini menurut WIB |
| `jam` | Ya | Format `HH:MM`, 24 jam, tanpa detik |
| `lokasi` | Ya | 1-255 karakter, spasi di awal dan akhir dibuang |

**Contoh:**
```bash
curl -X PUT http://localhost:3000/api/v1/pendaftaran/jadwal-seleksi-substansi \
  -b "access_token=$ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"id_pendaftaran":[1,2,3],"tanggal":"2026-11-10","jam":"13:30","lokasi":"Gedung F, Ruang F2.3"}'
```

Untuk satu pendaftaran, cukup kirim array berisi satu id, misalnya `"id_pendaftaran":[1]`.

**Aturan:**
- Semua pendaftaran harus ada dan berstatus `Lolos Seleksi Administrasi`. Kalau ada satu saja yang tidak memenuhi, seluruh permintaan ditolak dan tidak ada yang tersimpan.
- Jadwal yang sudah ada diganti, jadi endpoint yang sama dipakai untuk menambah dan mengubah jadwal. Dalam satu permintaan boleh ada pendaftaran yang sudah dan belum punya jadwal.
- Semua pendaftaran mendapat jadwal yang sama. Jadwal yang berbeda dikirim lewat permintaan terpisah.
- Hanya tanggal yang dibandingkan dengan hari ini. Jam yang sudah lewat pada tanggal hari ini tetap diterima.
- Periode pendaftaran tidak diperiksa, jadi jadwal tetap bisa disimpan setelah periodenya selesai.

**Yang dilakukan:** dalam satu transaksi, menyimpan jadwal dan memperbarui `updated_at` setiap pendaftaran. Status tidak berubah, jadi `log_status` tidak bertambah. Selama transaksi berjalan, perubahan status pada pendaftaran yang sama menunggu sampai selesai, sehingga pengecekan status tetap akurat untuk permintaan yang masuk bersamaan.

Setelah tersimpan, jadwal muncul di field `jadwal_seleksi` pada [`GET /api/v1/pendaftaran/{id}`](#get-apiv1pendaftaranid), baik untuk mahasiswa pemiliknya maupun Panitia Magang. Kalau statusnya berubah kemudian, misalnya menjadi `Diterima`, jadwalnya tetap tersimpan sebagai riwayat, tapi tidak bisa diubah lagi.

**Respons `200`:** data yang tersimpan, dengan `id_pendaftaran` terurut dan tanpa duplikat.
```json
{
  "id_pendaftaran": [1, 2, 3],
  "tanggal": "2026-11-10",
  "jam": "13:30",
  "lokasi": "Gedung F, Ruang F2.3"
}
```

| Status | Arti |
|---|---|
| `400` | Body bukan JSON yang valid. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang, termasuk mahasiswa pemilik pendaftaran |
| `404` | Ada pendaftaran yang tidak ditemukan |
| `409` | Ada pendaftaran yang tidak berstatus `Lolos Seleksi Administrasi` |
| `415` | Header `Content-Type: application/json` tidak dikirim |
| `422` | Isi body ditolak, lihat pesannya di bawah |

Respons `404` tidak punya body, dan respons `409` serta `422` tidak menyebut id mana yang bermasalah. Kalau ditolak, tidak ada data yang berubah.

**Pesan error `409` dan `422`**, dalam format `{ "message": "..." }`:

| Status | Pesan | Penyebab |
|---|---|---|
| `409` | `Semua pendaftaran harus berstatus Lolos Seleksi Administrasi.` | Ada pendaftaran dengan status lain |
| `422` | `Pilih minimal satu pendaftaran.` | `id_pendaftaran` berupa array kosong |
| `422` | `Tanggal harus berformat YYYY-MM-DD.` | Format `tanggal` salah, atau tanggalnya tidak ada, misalnya `2026-02-30` |
| `422` | `Tanggal tidak boleh sudah lewat.` | `tanggal` sebelum hari ini menurut WIB |
| `422` | `Jam harus berformat HH:MM.` | Format `jam` salah, misalnya `24:00`, `1:30`, atau `13:30:00` |
| `422` | `Lokasi harus berisi 1-255 karakter.` | `lokasi` kosong, hanya berisi spasi, atau terlalu panjang |

Field yang tidak dikirim, tipe data yang salah, dan field yang tidak dikenal juga menghasilkan `422`, tapi body-nya teks biasa dari framework.

### `PUT /api/v1/pendaftaran/plot-dpm`

Menyimpan satu plotting DPM (Dosen Pembimbing Magang) yang sama untuk satu atau banyak pendaftaran, untuk aksi massal maupun satu pendaftaran. Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`.

**Header:** `Content-Type: application/json`

**Body:**

| Field | Wajib | Aturan |
|---|---|---|
| `id_pendaftaran` | Ya | Array bilangan bulat, minimal 1. Id yang sama cukup dihitung sekali |
| `nama_dpm` | Ya | 1-255 karakter, spasi di awal dan akhir dibuang |
| `kontak_dpm` | Ya | 1-255 karakter, spasi di awal dan akhir dibuang. Formatnya bebas, misalnya email atau nomor HP |

**Contoh:**
```bash
curl -X PUT http://localhost:3000/api/v1/pendaftaran/plot-dpm \
  -b "access_token=$ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"id_pendaftaran":[1,2,3],"nama_dpm":"Dr. Budi Santoso","kontak_dpm":"budi@ub.ac.id"}'
```

Untuk satu pendaftaran, cukup kirim array berisi satu id, misalnya `"id_pendaftaran":[1]`.

**Aturan:**
- Semua pendaftaran harus ada dan berstatus `Aktif`. Kalau ada satu saja yang tidak memenuhi, seluruh permintaan ditolak dan tidak ada yang tersimpan.
- Plotting yang sudah ada diganti, jadi endpoint yang sama dipakai untuk menambah dan mengubah plotting. Dalam satu permintaan boleh ada pendaftaran yang sudah dan belum punya plotting.
- Semua pendaftaran mendapat DPM yang sama. DPM yang berbeda dikirim lewat permintaan terpisah.
- Periode pendaftaran tidak diperiksa.

**Yang dilakukan:** dalam satu transaksi, menyimpan plotting dan memperbarui `updated_at` setiap pendaftaran. Status tidak berubah, jadi `log_status` tidak bertambah. Selama transaksi berjalan, perubahan status pada pendaftaran yang sama menunggu sampai selesai.

Setelah tersimpan, plotting muncul di field `plotting_dpm` pada [`GET /api/v1/pendaftaran/{id}`](#get-apiv1pendaftaranid), dan `ada_plotting_dpm` di [`GET /api/v1/pendaftaran`](#get-apiv1pendaftaran) menjadi `true`, baik untuk mahasiswa pemiliknya maupun Panitia Magang. Kalau statusnya berubah menjadi `Selesai`, plotting-nya tetap tersimpan sebagai riwayat, tapi tidak bisa diubah lagi.

**Respons `200`:** data yang tersimpan, dengan `id_pendaftaran` terurut dan tanpa duplikat.
```json
{
  "id_pendaftaran": [1, 2, 3],
  "nama_dpm": "Dr. Budi Santoso",
  "kontak_dpm": "budi@ub.ac.id"
}
```

| Status | Arti |
|---|---|
| `400` | Body bukan JSON yang valid. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang, termasuk mahasiswa pemilik pendaftaran |
| `404` | Ada pendaftaran yang tidak ditemukan |
| `409` | Ada pendaftaran yang tidak berstatus `Aktif` |
| `415` | Header `Content-Type: application/json` tidak dikirim |
| `422` | Isi body ditolak, lihat pesannya di bawah |

Respons `404` tidak punya body, dan respons `409` serta `422` tidak menyebut id mana yang bermasalah. Kalau ditolak, tidak ada data yang berubah.

**Pesan error `409` dan `422`**, dalam format `{ "message": "..." }`:

| Status | Pesan | Penyebab |
|---|---|---|
| `409` | `Semua pendaftaran harus berstatus Aktif.` | Ada pendaftaran dengan status lain |
| `422` | `Pilih minimal satu pendaftaran.` | `id_pendaftaran` berupa array kosong |
| `422` | `Nama DPM harus berisi 1-255 karakter.` | `nama_dpm` kosong, hanya berisi spasi, atau terlalu panjang |
| `422` | `Kontak DPM harus berisi 1-255 karakter.` | `kontak_dpm` kosong, hanya berisi spasi, atau terlalu panjang |

Field yang tidak dikirim, tipe data yang salah, dan field yang tidak dikenal juga menghasilkan `422`, tapi body-nya teks biasa dari framework.

### `POST /api/v1/pendaftaran/ekspor`

Mengekspor pendaftaran yang dipilih Panitia Magang menjadi satu berkas. Untuk saat ini formatnya hanya Excel (`.xlsx`). Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`.

**Header:** `Content-Type: application/json`

**Body:**

| Field | Wajib | Aturan |
|---|---|---|
| `format` | Ya | `"xlsx"` |
| `id_pendaftaran` | Ya | Array bilangan bulat, 1 sampai 10000 id. Id yang sama cukup dihitung sekali |

**Contoh:**
```bash
curl -X POST http://localhost:3000/api/v1/pendaftaran/ekspor \
  -b "access_token=$ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"format":"xlsx","id_pendaftaran":[1,2,3]}' \
  -o pendaftar.xlsx
```

**Isi berkas:** satu sheet `Pendaftar`, satu baris per pendaftaran, urut dari yang paling baru seperti di [`GET /api/v1/pendaftaran`](#get-apiv1pendaftaran). Baris judul dibekukan dan diberi filter, dan lebar kolom menyesuaikan isinya.

| Kolom | Isi |
|---|---|
| No | Nomor urut |
| Nama, NIM, Email, Jenis Kelamin, Program Studi, Semester, IPK, No. HP | Data diri mahasiswa. Jenis kelamin ditulis `Laki-laki` atau `Perempuan` |
| Posisi, Perusahaan | Posisi yang didaftar |
| Periode | Catatan periode dan rentang tanggalnya, misalnya `Ganjil 2026/2027 (1 Okt 2026 – 30 Nov 2026)` |
| Status | Status saat ini |
| Tanggal Daftar, Terakhir Diubah | Tanggal dan jam (WIB), sebagai nilai tanggal Excel |
| Tanggal, Jam, dan Lokasi Seleksi Substansi | Kosong kalau jadwal belum diisi |
| Nama DPM, Kontak DPM | Kosong kalau plotting belum diisi |
| Nomor Surat Keterangan | Kosong kalau surat belum diterbitkan |
| URL Surat Pernyataan DPA, URL CV, URL Portfolio dan Berkas Tambahan | Link publik ke setiap berkas, lewat [`GET /api/v1/berkas/publik/{token}`](#get-apiv1berkaspubliktoken). Satu kolom per jenis berkas, urut seperti saat diunggah, jadi jenis berkas baru otomatis mendapat kolom sendiri. Kalau ada pendaftar yang punya lebih dari satu berkas sejenis, kolomnya diberi nomor: `URL Portfolio dan Berkas Tambahan 1`, `… 2`, dan seterusnya |

Sheet kedua, `Keterangan`, berisi waktu ekspor, jumlah pendaftar, dan batas waktu link berkas.

**Link berkas** bisa dibuka siapa pun yang memegang file Excel-nya, tanpa login, selama 30 hari sejak diekspor. Setelah itu, Panitia Magang perlu mengekspor ulang. Perlakukan file Excel-nya sebagai dokumen rahasia.

Semua teks ditulis sebagai teks, jadi isian seperti `=HYPERLINK(...)` tidak pernah dijalankan sebagai rumus. Ekspor tidak mengubah data apa pun.

**Respons `200`:** isi berkas.

| Header | Nilai |
|---|---|
| `Content-Type` | `application/vnd.openxmlformats-officedocument.spreadsheetml.sheet` |
| `Content-Disposition` | `attachment; filename="pendaftar-{YYYYMMDD}-{HHMM}.xlsx"`, dengan waktu WIB saat diekspor |
| `Cache-Control` | `private, no-store` |

| Status | Arti |
|---|---|
| `400` | Body bukan JSON yang valid. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang |
| `404` | Ada pendaftaran yang tidak ditemukan. Tidak ada berkas yang dibuat |
| `415` | Header `Content-Type: application/json` tidak dikirim |
| `422` | Isi body ditolak, lihat pesannya di bawah |

**Pesan error `422`**, dalam format `{ "message": "..." }`:

| Pesan | Penyebab |
|---|---|
| `Pilih minimal satu pendaftaran.` | `id_pendaftaran` berupa array kosong |
| `Maksimal 10000 pendaftaran dalam satu ekspor.` | Lebih dari 10000 id berbeda |

`format` selain `xlsx`, field yang tidak dikirim, tipe data yang salah, dan field yang tidak dikenal juga menghasilkan `422`, tapi body-nya teks biasa dari framework.

### `PATCH /api/v1/pendaftaran/accept`

Mengonfirmasi penerimaan: mengubah status pendaftaran dari `Diterima` menjadi `Aktif`, dan membatalkan pendaftaran lain milik mahasiswa itu yang masih berjalan. Hanya untuk mahasiswa pemilik pendaftaran, dilindungi middleware `require_mahasiswa`. Tanpa body.

**Query:**

| Parameter | Wajib | Aturan |
|---|---|---|
| `id` | Ya | `id_pendaftaran` yang dikonfirmasi, bilangan bulat |

**Contoh:**
```bash
curl -X PATCH "http://localhost:3000/api/v1/pendaftaran/accept?id=1" \
  -b "access_token=$ACCESS_TOKEN"
```

**Aturan:**
- Status pendaftaran yang dikonfirmasi harus `Diterima`.
- Mahasiswa tidak boleh sudah punya pendaftaran `Aktif`.
- Konfirmasi harus dilakukan sebelum `batas_konfirmasi`, yaitu dalam 12 jam setelah status menjadi `Diterima`. Lewat dari itu ditolak `409`, dan pendaftarannya dibatalkan otomatis.
- Konfirmasi tetap bisa dilakukan setelah periodenya selesai.

**Yang dilakukan:** dalam satu transaksi,
- pendaftaran yang dikonfirmasi menjadi `Aktif`;
- pendaftaran lain milik mahasiswa itu yang berstatus `Menunggu Verifikasi`, `Lolos Seleksi Administrasi`, atau `Diterima` menjadi `Batal/Mengundurkan Diri`, dari periode mana pun;
- pendaftaran yang sudah `Selesai`, `Tidak Lolos`, atau `Batal/Mengundurkan Diri` tidak berubah;
- setiap perubahan mengisi `updated_at` dan menambah baris `log_status`, dengan `id_pengubah` kosong karena dilakukan oleh mahasiswa sendiri.

**Respons `200`:** detail pendaftaran yang dikonfirmasi, dengan bentuk yang sama seperti [`GET /api/v1/pendaftaran/{id}`](#get-apiv1pendaftaranid).

| Status | Arti |
|---|---|
| `400` | `id` tidak dikirim atau bukan angka. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | Pendaftaran milik mahasiswa lain, atau user tidak punya baris di tabel `mahasiswa`, misalnya Panitia Magang |
| `404` | Pendaftaran tidak ditemukan |
| `409` | Status pendaftaran bukan `Diterima`, sudah punya pendaftaran `Aktif`, atau batas waktu 12 jam sudah lewat (`Batas waktu konfirmasi 1 x 12 jam sudah lewat, sehingga pendaftaran ini dibatalkan.`) |

### Pembatalan otomatis konfirmasi penerimaan

Bukan endpoint, tapi proses di server yang berjalan setiap menit sejak server start. Setiap pendaftaran berstatus `Diterima` yang `batas_konfirmasi`-nya sudah lewat diubah menjadi `Batal/Mengundurkan Diri`. Perubahannya mengisi `updated_at` dan menambah baris `log_status` dengan status lama `Diterima` dan `id_pengubah` kosong.

- Waktu mulainya diambil dari baris `log_status` saat status menjadi `Diterima`. Kalau baris itu tidak ada, misalnya data lama, `updated_at` yang dipakai.
- Karena prosesnya berjalan per menit, pembatalan bisa terjadi hingga sekitar satu menit setelah batas waktu. Dalam selang itu, konfirmasi lewat [`PATCH /api/v1/pendaftaran/accept`](#patch-apiv1pendaftaranaccept) sudah ditolak.
- Mahasiswa yang pendaftarannya dibatalkan dikirimi email (lihat email pemberitahuan di [`PATCH /api/v1/pendaftaran`](#patch-apiv1pendaftaran)) dengan keterangan bahwa pendaftaran dibatalkan otomatis karena penerimaan tidak dikonfirmasi dalam 1 x 12 jam.

### `GET /api/v1/berkas/{id_berkas}`

Mengunduh satu berkas pendaftaran. `id_berkas` didapat dari respons [`POST /api/v1/pendaftaran`](#post-apiv1pendaftaran) atau [`GET /api/v1/pendaftaran/{id}`](#get-apiv1pendaftaranid).

| User | Akses |
|---|---|
| Mahasiswa pemilik pendaftaran | Boleh |
| Mahasiswa lain | `403` |
| Panitia Magang | Boleh, untuk semua berkas |

**Respons `200`:** isi file dengan `Content-Type: application/pdf` dan `Content-Disposition: attachment`. Respons ini tidak boleh disimpan di cache.

| Status | Arti |
|---|---|
| `400` | `id_berkas` bukan angka. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | Berkas milik mahasiswa lain, atau user bukan mahasiswa maupun Panitia Magang |
| `404` | Berkas tidak ditemukan, baik barisnya maupun file-nya di folder upload |

### `GET /api/v1/berkas/publik/{token}`

Membuka satu berkas pendaftaran tanpa login, lewat link di file hasil [`POST /api/v1/pendaftaran/ekspor`](#post-apiv1pendaftaranekspor). Dipakai untuk perusahaan mitra dan kaprodi yang menyeleksi dari file Excel, karena mereka tidak punya akun.

**Link di Excel** mengarah ke halaman frontend `{FRONTEND_URL}/berkas/{token}`, yang mengambil berkasnya dari endpoint ini lalu menampilkannya dengan tombol unduh.

**Token:** `{id_berkas}.{kedaluwarsa}.{tag}`.
- `kedaluwarsa` adalah waktu Unix dalam detik, 30 hari setelah diekspor.
- `tag` adalah HMAC-SHA256 base64url atas `{id_berkas}.{kedaluwarsa}`. Kuncinya diturunkan dari `JWT_SECRET` khusus untuk link berkas, jadi tidak bisa dipakai sebagai token sesi atau sebaliknya.
- Mengubah bagian mana pun, misalnya `id_berkas` untuk membuka berkas lain atau `kedaluwarsa` untuk memperpanjang, membuat tag-nya tidak cocok.
- Link tidak bisa dicabut satu per satu sebelum kedaluwarsa. Mengganti `JWT_SECRET` membatalkan semua link sekaligus, dan semua sesi login.
- Link berhenti berfungsi kalau berkasnya dihapus.

**Respons `200`:** isi file dengan header berikut.

| Header | Nilai |
|---|---|
| `Content-Type` | `application/pdf` |
| `Content-Disposition` | `inline; filename="{jenis}-{nim}.pdf"`, misalnya `cv-225150200111001.pdf` |
| `Cache-Control` | `private, no-store` |
| `X-Robots-Tag` | `noindex` |

| Status | Arti |
|---|---|
| `404` | Token tidak valid, sudah diubah, sudah kedaluwarsa, dibuat dengan kunci lain, atau berkasnya tidak ditemukan. Semuanya sengaja dibuat sama, tanpa body. Frontend membaca `kedaluwarsa` dari token sendiri untuk menampilkan pesan bahwa link sudah kedaluwarsa |

## Endpoint surat keterangan

Surat keterangan kepesertaan magang tidak diunggah Panitia Magang, tapi dibuat backend dari template fakultas, lalu ditandatangani secara digital supaya salinan palsu atau yang diubah bisa dikenali.

**Cara kerjanya:**
- Surat diterbitkan saat pertama kali diunduh, oleh mahasiswa pemiliknya atau Panitia Magang, selama pendaftaran berstatus `Aktif`.
- Saat terbit, data yang tercetak dibekukan sebagai JSON di kolom `isi`, lalu ditandatangani dengan **Ed25519**. Perubahan data mahasiswa, posisi, atau template sesudahnya tidak mengubah surat yang sudah terbit.
- PDF dibuat sekali dan disimpan di folder upload. Hash SHA-256 file-nya disimpan di `hash_berkas`, dan unduhan berikutnya mengirim file yang sama persis.
- Di ruang tanda tangan ada kode QR berisi `{FRONTEND_URL}/verifikasi-surat?kode={id_surat}.{tanda_tangan}`. Mengklik QR di pembaca PDF membuka alamat yang sama.

**Yang dijamin:** siapa pun bisa membuat dokumen yang mirip, tapi dokumen itu pasti gagal diverifikasi. Kode QR tidak bisa dipalsukan tanpa kunci privat, PDF yang diubah satu byte pun tidak cocok dengan hash-nya, dan baris yang diubah langsung di database tidak lolos pemeriksaan tanda tangan.

**Konfigurasi:** env `SURAT_SIGNING_KEY` berisi seed Ed25519 32 byte dalam base64. Tanpa env ini, semua endpoint di bagian ini menjawab `503`. Kunci yang formatnya salah membuat server berhenti saat start. Kunci ini **jangan diganti** setelah ada surat yang terbit, karena surat lama tidak akan lolos verifikasi lagi.

```bash
# Membuat kunci baru
openssl rand -base64 32
```

Isi yang belum punya sumber data masih mockup. Daftarnya ada di [Yang masih perlu dibuat di backend](#yang-masih-perlu-dibuat-di-backend).

### `GET /api/v1/pendaftaran/{id}/surat-keterangan`

Mengunduh surat keterangan sebuah pendaftaran, dan menerbitkannya pada unduhan pertama.

| User | Akses |
|---|---|
| Mahasiswa pemilik pendaftaran | Boleh |
| Mahasiswa lain | `403` |
| Panitia Magang | Boleh, untuk semua pendaftaran |

**Aturan:**
- Surat baru hanya terbit untuk pendaftaran berstatus `Aktif`.
- Surat yang sudah terbit tetap bisa diunduh walaupun statusnya sudah berubah, misalnya menjadi `Selesai`.
- Unduhan pertama yang bersamaan tetap menghasilkan satu surat.

**Respons `200`:** file PDF dengan `Content-Type: application/pdf` dan `Content-Disposition: attachment; filename="surat-keterangan-magang-{nim}.pdf"`. Respons ini tidak boleh disimpan di cache.

| Status | Arti |
|---|---|
| `400` | `id` bukan angka. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | Pendaftaran milik mahasiswa lain, atau user bukan mahasiswa maupun Panitia Magang |
| `404` | Pendaftaran tidak ditemukan, atau file surat hilang dari folder upload |
| `409` | `Surat keterangan hanya tersedia untuk pendaftaran berstatus Aktif.` |
| `503` | `SURAT_SIGNING_KEY` tidak di-set |

### `GET /api/v1/surat-keterangan/verifikasi`

Memverifikasi surat dari kode di QR-nya. Publik, tanpa login, karena dipakai siapa pun yang memegang surat.

**Query:**

| Parameter | Wajib | Aturan |
|---|---|---|
| `kode` | Ya | `{id_surat}.{tanda_tangan}`, persis seperti di QR |

**Respons `200`:** isi surat yang tercatat saat terbit. Pemeriksa membandingkannya dengan surat yang dipegang.
```json
{
  "versi": 1,
  "nomor_surat": "00001/SKM/MB-FILKOM/2026",
  "tanggal_surat": "2026-11-06",
  "nama": "Budi Santoso",
  "nim": "215150200111001",
  "program_studi": "S1 Sistem Informasi",
  "posisi": "Intern Business Analyst",
  "perusahaan": "PT Nusantara Digital",
  "alamat_perusahaan": "Jl. Contoh Alamat Instansi No. 1, Kota Malang",
  "program": "Magang Mandiri",
  "tanggal_mulai": "2026-11-06",
  "tanggal_selesai": "2027-03-06",
  "nama_pejabat": "[Nama Pejabat]",
  "nip_pejabat": "[NIP]",
  "jabatan_pejabat": "Wakil Dekan Bidang Akademik"
}
```

| Status | Arti |
|---|---|
| `400` | `kode` tidak dikirim. Body-nya teks biasa dari framework |
| `404` | Kode salah format, tidak dikenal, atau tanda tangannya tidak cocok dengan isi surat. Tanpa body |
| `503` | `SURAT_SIGNING_KEY` tidak di-set |

### `POST /api/v1/surat-keterangan/verifikasi`

Memverifikasi surat dari berkas PDF-nya. Publik, tanpa login. Berkas dianggap asli hanya kalau sama persis, byte per byte, dengan yang diterbitkan, dan tanda tangan isinya masih cocok.

**Header:** `Content-Type: multipart/form-data`

| Field | Wajib | Aturan |
|---|---|---|
| `berkas` | Ya | File PDF, maksimal 2 MB |

**Respons `200`:** isi surat, sama seperti [`GET /api/v1/surat-keterangan/verifikasi`](#get-apiv1surat-keteranganverifikasi).

| Status | Arti |
|---|---|
| `404` | Berkas tidak sama dengan surat mana pun yang diterbitkan, atau tanda tangannya tidak cocok. Tanpa body |
| `422` | `Pilih berkas PDF surat keterangan.` atau `Berkas terlalu besar untuk sebuah surat keterangan.` |
| `503` | `SURAT_SIGNING_KEY` tidak di-set |

### `GET /api/v1/surat-keterangan/kunci-publik`

Kunci publik penanda tangan surat, untuk pihak yang ingin memeriksa tanda tangan sendiri tanpa bergantung pada jawaban server. Publik, tanpa login.

Yang ditandatangani adalah `gomagang/surat-keterangan/v1\n` diikuti isi surat dalam JSON, persis seperti yang tersimpan saat terbit.

**Respons `200`:**
```json
{
  "algoritma": "Ed25519",
  "kunci_publik": "A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg="
}
```

| Status | Arti |
|---|---|
| `503` | `SURAT_SIGNING_KEY` tidak di-set |

## Endpoint Panitia Magang

### `POST /api/v1/panitia-magang` (sementara)

Mendaftarkan anggota Panitia Magang baru, sehingga tidak perlu menulis SQL manual. Endpoint ini sementara, sampai ada alur admin yang sebenarnya.

Route ini **hanya ada kalau env `ADMIN_SECRET` di-set**, minimal 32 byte. Tanpa env itu, responsnya `404`. Header secret-nya tidak diizinkan CORS, jadi endpoint ini dipanggil lewat `curl`, bukan dari browser.

**Header:** `X-Admin-Secret` dan `Content-Type: application/json`

| Field | Wajib | Aturan |
|---|---|---|
| `email` | Ya | Alamat `@gmail.com`, disimpan dalam huruf kecil. Tulis persis seperti akun Google-nya, termasuk titik |
| `nama` | Tidak | 1-255 karakter, spasi di awal dan akhir dibuang |
| `no_hp` | Tidak | 9-15 digit angka, boleh diawali `+` |

**Contoh:**
```bash
curl -X POST http://localhost:3000/api/v1/panitia-magang \
  -H "X-Admin-Secret: $ADMIN_SECRET" \
  -H "Content-Type: application/json" \
  -d '{"email":"nama@gmail.com","nama":"Nama Panitia Magang","no_hp":"081234567890"}'
```

**Yang dilakukan:** membuat baris `pengguna` dan `panitia_magang` dalam satu statement. `google_sub` masih kosong, dan terisi saat anggota itu login pertama kali.

**Respons `201`:**
```json
{
  "id_pengguna": 2,
  "email": "nama@gmail.com",
  "nama": "Nama Panitia Magang",
  "no_hp": "081234567890"
}
```

| Status | Arti |
|---|---|
| `403` | Header `X-Admin-Secret` tidak ada atau salah |
| `404` | `ADMIN_SECRET` tidak di-set di server |
| `409` | Email sudah terdaftar, tanpa membedakan huruf besar dan kecil |
| `422` | Isi body ditolak |

**Cara menghapus nanti:** hapus folder `src/features/panitia_magang`, barisnya di `src/features/mod.rs`, dan `admin_secret` di `Config` serta `AppState`.

## Endpoint templat SP DPA

Hanya ada satu templat surat pernyataan DPA yang berlaku, yaitu yang terakhir disimpan. Setiap penggantian menambah baris baru di `templat_sp_dpa`, jadi tautan lama tetap tersimpan sebagai riwayat.

### `GET /api/v1/templat-sp-dpa`

Mengambil templat yang berlaku. Bisa dipakai mahasiswa dan Panitia Magang.

**Respons `200`:**
```json
{
  "id_templat": 3,
  "tautan_templat": "https://drive.google.com/file/d/abc/view",
  "updated_at": "2026-11-10T14:00:00+07:00"
}
```

Body-nya `null` kalau Panitia Magang belum pernah mengatur tautan.

| Status | Arti |
|---|---|
| `401` | Access token tidak valid |
| `403` | User bukan mahasiswa maupun Panitia Magang |

### `PUT /api/v1/templat-sp-dpa`

Mengganti tautan templat yang berlaku. Hanya untuk Panitia Magang, dilindungi middleware `require_panitia_magang`.

**Header:** `Content-Type: application/json`

| Field | Wajib | Aturan |
|---|---|---|
| `tautan_templat` | Ya | URL lengkap dengan skema `http` atau `https` dan nama host, tanpa spasi, 1-255 karakter. Spasi di awal dan akhir dibuang |

**Contoh body:**
```json
{
  "tautan_templat": "https://drive.google.com/file/d/abc/view"
}
```

**Respons `200`:** templat yang baru berlaku, dengan bentuk yang sama seperti [`GET /api/v1/templat-sp-dpa`](#get-apiv1templat-sp-dpa).

| Status | Arti |
|---|---|
| `401` | Access token tidak valid |
| `403` | User bukan Panitia Magang |
| `415` | Header `Content-Type: application/json` tidak dikirim |
| `422` | Isi body ditolak |

## Riwayat perubahan

### 12 November 2026

- `DELETE /api/v1/mahasiswa` dan `POST /api/v1/mahasiswa/seed` (endpoint sementara untuk uji coba) dihapus, beserta folder `src/features/data_uji_coba`. `ADMIN_SECRET` kini hanya dipakai `POST /api/v1/panitia-magang`.

### 11 November 2026

- Konfirmasi penerimaan kini dibatasi 1 x 12 jam sejak status menjadi `Diterima`. `GET /api/v1/pendaftaran` dan `GET /api/v1/pendaftaran/{id}` mengirim field baru `batas_konfirmasi`. `PATCH /api/v1/pendaftaran/accept` menolak konfirmasi yang terlambat dengan `409`, dan server membatalkan otomatis pendaftaran yang tidak dikonfirmasi menjadi `Batal/Mengundurkan Diri`, disertai email pemberitahuan. Email untuk status `Diterima` kini berisi peringatan tegas tentang batas waktu tersebut.
- Email pemberitahuan perubahan status kini berupa HTML dengan logo, ikon status, dan tombol ke detail pendaftaran, dengan versi teks biasa sebagai cadangan.
- `PATCH /api/v1/pendaftaran` kini mengirim email pemberitahuan ke mahasiswa setiap kali Panitia Magang berhasil mengubah status pendaftarannya. Env baru: `SMTP_HOST`, `SMTP_PORT`, `SMTP_TLS`, `SMTP_USERNAME`, `SMTP_PASSWORD`, `SMTP_FROM`. Tanpa `SMTP_HOST`, email tidak dikirim.
- `POST /api/v1/pendaftaran`: batas pendaftaran kini 2 kali per periode, dihitung dari semua pendaftaran di periode posisi yang dituju apa pun statusnya. Sebelumnya batasnya 2 pendaftaran yang masih berjalan, sehingga pendaftaran yang batal, tidak lolos, atau selesai membuka jatah baru. Pesan `409`-nya kini `Kamu sudah mendaftar 2 kali di periode ini.`
- Penamaan **Tim MB** diganti menjadi **Panitia Magang** di seluruh backend, tanpa perubahan perilaku. Tabel `tim_mb` menjadi `panitia_magang` beserta nama constraint-nya, lewat migration `20261111000000_rename_tim_mb_menjadi_panitia_magang.sql`. `POST /api/v1/tim-mb` menjadi [`POST /api/v1/panitia-magang`](#post-apiv1panitia-magang-sementara), middleware `require_tim_mb` menjadi `require_panitia_magang`, dan setelah login akun Panitia Magang kini diarahkan ke `/panitia-magang`, bukan `/tim-mb`. Catatan riwayat di bawah ini tetap memakai nama lama.

### 10 November 2026

- Placeholder `GET /api/v1/templat-sp-dpa` diganti endpoint sungguhan yang mengembalikan tautan templat surat pernyataan DPA yang berlaku (atau `null`), kini dilindungi autentikasi untuk mahasiswa dan Tim MB. `PUT /api/v1/templat-sp-dpa` ditambahkan untuk Tim MB mengganti tautannya. Tautan lama tetap tersimpan sebagai riwayat.
- Konversi mata kuliah ditambahkan. `GET`, `POST /api/v1/mata-kuliah` dan `DELETE /api/v1/mata-kuliah/{id}` untuk katalog (tambah dan hapus hanya Tim MB, hapus berupa soft delete). `GET`, `POST /api/v1/pendaftaran/{id}/penyetaraan-mk` dan `DELETE /api/v1/penyetaraan-mk/{id}` untuk mahasiswa memilih mata kuliah pada pendaftaran `Aktif`. Migration `20261110000002_mata_kuliah_soft_delete.sql` menambah kolom `deleted_at` di `mata_kuliah_konversi`, serta constraint unik `(id_pendaftaran, id_mk_konversi)` di `penyetaraan_mk` setelah membuang pilihan ganda yang mungkin sudah ada.
- `PATCH /api/v1/posisi/{id}` dan `DELETE /api/v1/posisi/{id}` ditambahkan, untuk Tim MB mengubah nama, kuota, dan deskripsi posisi magang, serta menyembunyikannya (soft delete). `GET /api/v1/posisi` kini mengirim `deleted_at` dan menerima `termasuk_dihapus`. Posisi yang disembunyikan tidak bisa didaftar.
- Perbaikan: `POST /api/v1/mahasiswa/seed` tidak lagi memakai posisi dari perusahaan yang sudah disembunyikan.
- `PATCH /api/v1/perusahaan/{id}` dan `DELETE /api/v1/perusahaan/{id}` ditambahkan, untuk Tim MB mengubah nama dan menyembunyikan perusahaan mitra (soft delete). `GET /api/v1/perusahaan` kini mengirim `deleted_at` dan menerima `termasuk_dihapus`. Posisi dari perusahaan yang disembunyikan hilang dari katalog dan tidak bisa didaftar. CORS kini mengizinkan method `DELETE`.

### 9 November 2026

- `POST /api/v1/mahasiswa/seed` kini menambahkan 50 mahasiswa, bukan 200. Setiap pendaftaran punya file PDF contoh sungguhan untuk Surat Pernyataan DPA dan CV, dan sebagian juga punya 1 sampai 3 berkas tambahan. Respons kini juga berisi `jumlah_berkas`.
- `POST /api/v1/pendaftaran/ekspor` ditambahkan, untuk Tim MB mengekspor pendaftaran terpilih ke Excel (.xlsx), lengkap dengan link publik ke setiap berkas yang berlaku 30 hari. Dependency baru: `rust_xlsxwriter`, dan `zip` untuk pengujian.
- `GET /api/v1/berkas/publik/{token}` ditambahkan, untuk membuka berkas dari link di file ekspor tanpa login.
- `POST /api/v1/pendaftaran` kini menerima field opsional `berkas_tambahan` (portfolio dan berkas tambahan), maksimal 5 file PDF, disimpan di `berkas_pendaftaran` dengan jenis `Portfolio dan Berkas Tambahan`. Ekspor Excel memberi kolom URL bernomor untuk berkas sejenis yang lebih dari satu.
- `GET /api/v1/pendaftaran` kini menerima query `posisi` (`id_posisi`), untuk menyaring pendaftaran per posisi magang.
- `POST /api/v1/mahasiswa/seed` kini membagi pendaftaran rata ke semua posisi di periode yang sedang berlangsung, bukan acak murni, sehingga setiap posisi pasti mendapat pendaftar.

### 6 November 2026

- Kuota posisi kini kuota penerimaan dan hanya sebagai informasi. Kolom `kuota_total` diganti nama menjadi `kuota` dan `sisa_kuota` dihapus, lewat migration `20261106000001_kuota_posisi_sebagai_informasi.sql`. Di API, field `kuota_total` menjadi `kuota`, dan `sisa_kuota` dihapus dari respons `GET` dan `POST /api/v1/posisi`.
- `POST /api/v1/pendaftaran` tidak lagi menolak pendaftaran karena kuota penuh. Query `is_full` di `GET /api/v1/posisi` dihapus, dan `DELETE /api/v1/mahasiswa` tidak lagi mengubah kuota.
- Surat keterangan magang kini dibuat backend dari template, ditandatangani dengan Ed25519, dan diverifikasi lewat QR atau berkas PDF: `GET /api/v1/pendaftaran/{id}/surat-keterangan`, `GET` dan `POST /api/v1/surat-keterangan/verifikasi`, serta `GET /api/v1/surat-keterangan/kunci-publik`. Env baru: `SURAT_SIGNING_KEY`.
- Tabel `surat_keterangan` mendapat kolom `nomor_surat`, `isi`, `tanda_tangan`, dan `hash_berkas`, lewat migration `20261106000000_surat_keterangan_bertanda_tangan.sql`.
- `GET /api/v1/pendaftaran/{id}` kini mengirim `nomor_surat` di `surat_keterangan`.
- `DELETE /api/v1/mahasiswa` kini ikut menghapus file surat keterangan, dan menghitungnya di `jumlah_berkas`.
- CORS kini membuka header `Content-Disposition`.
- Status frontend diperbarui: plotting DPM, filter dan pencarian daftar pendaftar, serta surat keterangan kini tersambung ke API.

### 5 November 2026

- `PUT /api/v1/pendaftaran/plot-dpm` ditambahkan, untuk Tim MB menyimpan plotting DPM pada satu atau banyak pendaftaran.
- Default `r` di `GET /api/v1/pendaftaran` diubah dari 10 menjadi 25.
- `GET /api/v1/pendaftaran` kini menerima `s` untuk mencari pendaftaran menurut nama mahasiswa.
- Status frontend diperbarui: menyimpan jadwal seleksi substansi kini tersambung ke API.
- `GET /api/v1/pendaftaran` kini menerima filter `periode`, `status`, dan `perusahaan`. **Tanpa `p` dan `r`, responsnya kini halaman 1 dengan 10 baris**, bukan semua baris, dan `p` boleh dikirim tanpa `r`.
- `GET /api/v1/pendaftaran` kini menerima query `p` dan `r`, dan **responsnya berubah dari array menjadi objek** berisi `data`, `page`, `rows`, `total`, dan `total_pages`.
- `DELETE /api/v1/mahasiswa` kini menghapus semua mahasiswa dan tidak lagi menerima `email`. Kodenya pindah ke folder `src/features/data_uji_coba`.
- `POST /api/v1/mahasiswa/seed` ditambahkan sebagai endpoint sementara untuk uji coba, dilindungi `ADMIN_SECRET`. Menambahkan 200 mahasiswa acak, masing-masing dengan 1 pendaftaran.
- `GET /api/v1/pendaftaran` kini mengirim `ada_jadwal_seleksi` dan `ada_plotting_dpm` pada setiap pendaftaran.
- Perbaikan: `PATCH /api/v1/pendaftaran` dan `PUT /api/v1/pendaftaran/jadwal-seleksi-substansi` bisa keliru menjawab `404` kalau status pendaftaran berubah saat request sedang menunggu. Kini keduanya memeriksa status terbaru.
- Status frontend diperbarui: mengubah status pendaftaran dan konfirmasi penerimaan kini tersambung ke API.
- `PUT /api/v1/pendaftaran/jadwal-seleksi-substansi` ditambahkan, untuk Tim MB menyimpan jadwal seleksi substansi pada satu atau banyak pendaftaran, dan CORS kini mengizinkan method `PUT`.
- `DELETE /api/v1/mahasiswa` ditambahkan sebagai endpoint sementara untuk uji coba, dilindungi `ADMIN_SECRET`. Menghapus mahasiswa beserta semua datanya dan filenya, serta mengembalikan kuota.
- `PATCH /api/v1/pendaftaran/accept` ditambahkan, untuk mahasiswa mengonfirmasi penerimaan. Pendaftaran lain yang masih berjalan otomatis menjadi `Batal/Mengundurkan Diri`, dan kuotanya tidak kembali.
- `POST /api/v1/pendaftaran` kini menolak mahasiswa yang punya pendaftaran `Aktif`.
- `PATCH /api/v1/pendaftaran` ditambahkan, untuk Tim MB mengubah status pendaftaran.
- Status frontend diperbarui: pendaftaran, unduh berkas, serta memulai dan menghentikan periode kini tersambung ke API.
- `PATCH /api/v1/periode/{id}/stop` ditambahkan, untuk menghentikan periode saat itu juga.
- `PATCH /api/v1/periode/{id}/start` ditambahkan, untuk memulai periode saat itu juga.

### 4 November 2026

- `GET /api/v1/pendaftaran` dan `GET /api/v1/pendaftaran/{id}` diimplementasikan, menggantikan placeholder, dan kini dilindungi autentikasi.
- Status frontend diperbarui: perusahaan, tambah posisi, dan dropdown periode di halaman Pendaftar kini tersambung ke API.
- `POST /api/v1/pendaftaran` ditambahkan, menerima unggahan dua berkas PDF dan menyimpannya di folder `uploads/`.
- `GET /api/v1/berkas/{id_berkas}` ditambahkan, hanya untuk pemilik berkas dan Tim MB.
- Tabel `status_pendaftaran` kini terisi tujuh status lewat migration.
- `GET` dan `POST /api/v1/perusahaan` ditambahkan.
- `POST /api/v1/posisi` ditambahkan.
- `POST /api/v1/periode` ditambahkan, bersama middleware `require_tim_mb`.
- `POST /api/v1/tim-mb` ditambahkan sebagai endpoint sementara, dilindungi `ADMIN_SECRET`.
- `GET /api/v1/periode` diimplementasikan, menggantikan placeholder, dengan filter `id` dan `status`.
- `GET /api/v1/posisi` menggantikan placeholder `GET /api/v1/lowongan`, yang sudah dihapus.
- `PATCH /api/v1/mahasiswa/data` ditambahkan, dan CORS kini mengizinkan method `PATCH`.
- `GET /auth/google` menerima akun `@gmail.com` yang terdaftar di `tim_mb`, mengarahkannya ke `/tim-mb`, dan menambah error `not_registered`.
- `GET /auth/google/login` tidak lagi mengirim `hd` ke Google, supaya akun Gmail bisa dipilih.
- `GET /auth/google` otomatis membuat baris `mahasiswa` untuk akun student.
- `GET /api/v1/mahasiswa/data` menambah `jenis_kelamin` dan `semester`, dan `nim` boleh `null`.
- `GET /auth/me` mengembalikan `id_pengguna` dan `email`, tanpa nama.
- Dokumentasi ini dibuat, lalu ditambah bagian progress dan status frontend.
- Status frontend diperbarui: periode dan katalog posisi kini tersambung ke API.

### 3 November 2026

- `GET /api/v1/mahasiswa/data` ditambahkan dengan middleware `require_mahasiswa`, menggantikan placeholder `GET /api/v1/data-diri`.
- `POST /auth/refresh` ditambahkan. Session kini memakai access token 15 menit dan refresh token 7 hari.
- `POST /auth/logout` kini mencabut refresh token di database.

### 2 November 2026

- Struktur fitur, konfigurasi CORS, dan route placeholder dibuat.
- Login Google dibuat dengan authorization code flow: `GET /auth/google/login`, `GET /auth/google`, `GET /auth/me`, dan `POST /auth/logout`.
