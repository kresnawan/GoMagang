# Dokumentasi API GoMagang

Daftar endpoint yang tersedia di backend saat ini. File ini diperbarui setiap kali ada endpoint yang ditambah atau diubah, dan riwayatnya ada di [bagian paling bawah](#riwayat-perubahan).

**Terakhir diperbarui:** 4 November 2026

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
| `GET` | [`/api/v1/posisi`](#get-apiv1posisi) | Daftar posisi magang | Mahasiswa, Tim MB | ✅ Jadi | Tersambung: halaman Pendaftaran dan Katalog |
| `POST` | [`/api/v1/posisi`](#post-apiv1posisi) | Membuat posisi magang | Tim MB | ✅ Jadi | Belum dipanggil |
| `GET` | [`/api/v1/periode`](#get-apiv1periode) | Daftar periode pendaftaran | Mahasiswa, Tim MB | ✅ Jadi | Tersambung: halaman Periode dan Katalog |
| `POST` | [`/api/v1/periode`](#post-apiv1periode) | Membuat periode pendaftaran | Tim MB | ✅ Jadi | Tersambung |
| `GET` | [`/api/v1/perusahaan`](#get-apiv1perusahaan) | Daftar perusahaan mitra | Mahasiswa, Tim MB | ✅ Jadi | Masih mock |
| `POST` | [`/api/v1/perusahaan`](#post-apiv1perusahaan) | Membuat perusahaan mitra | Tim MB | ✅ Jadi | Masih mock |
| `POST` | [`/api/v1/tim-mb`](#post-apiv1tim-mb-sementara) | Mendaftarkan anggota Tim MB baru (sementara) | `ADMIN_SECRET` | ✅ Jadi | Tidak dipakai, lewat `curl` |
| `GET` | [`/api/v1/pendaftaran`](#endpoint-placeholder) | Daftar pendaftaran | Belum dilindungi | 🚧 Placeholder | Masih mock |
| `GET` | [`/api/v1/pendaftaran/{id}`](#endpoint-placeholder) | Detail pendaftaran | Belum dilindungi | 🚧 Placeholder | Masih mock |
| `GET` | [`/api/v1/templat-sp-dpa`](#endpoint-placeholder) | Daftar templat SP DPA | Belum dilindungi | 🚧 Placeholder | Belum dipakai |

## Progress

**14 dari 17 endpoint sudah jadi**, dan 3 sisanya masih placeholder.

| Area | Backend | Frontend |
|---|---|---|
| Login Google, access token, refresh token, logout | ✅ Jadi | Tersambung |
| Data diri mahasiswa | ✅ Baca dan ubah | Tersambung |
| Posisi magang | ✅ Baca dan tambah | Baca tersambung, tambah belum dipanggil |
| Periode | ✅ Baca dan tambah | Tersambung, kecuali dropdown di halaman Pendaftar |
| Perusahaan mitra | ✅ Baca dan tambah | Masih mock |
| Pendaftaran Tim MB (`POST /api/v1/tim-mb`) | ✅ Sementara, lewat `curl` | Tidak dipakai |
| Pendaftaran mahasiswa | ⬜ Masih placeholder | Masih mock |
| Pengelolaan pendaftar oleh Tim MB | ⬜ Belum ada | Masih mock |
| Konversi mata kuliah | ⬜ Belum ada | Masih mock |
| Templat SP DPA | ⬜ Masih placeholder | Belum dipakai |

### Yang masih perlu dibuat di backend

Daftar ini diturunkan dari fungsi yang masih mock di `gomagang-web/src/services`. Rancangan endpoint-nya belum ditentukan.

**Mahasiswa:**
- Detail satu posisi, untuk form pendaftaran
- Mengirim pendaftaran, dan mengimplementasikan daftar serta detail pendaftaran
- Konfirmasi penerimaan
- Konversi mata kuliah: daftar mata kuliah, serta melihat, menambah, dan menghapus penyetaraan

**Tim MB:**
- Daftar dan detail pendaftar, serta mengubah statusnya
- Menyimpan jadwal seleksi, plotting DPM, dan surat keterangan

**Keamanan dan peran:**
- `GET /auth/me` belum mengirim peran, jadi frontend menebaknya dari domain email
- Tiga endpoint placeholder belum dilindungi autentikasi

**Data yang belum disediakan:**
- `GET /api/v1/perusahaan` tidak mengirim jumlah posisi, padahal halaman Perusahaan menampilkannya

### Yang sudah ada di backend, tapi belum dipakai frontend

- `POST /api/v1/posisi`: frontend belum punya fungsi untuk menambah posisi
- `GET` dan `POST /api/v1/perusahaan`: halaman Perusahaan masih memakai mock
- `GET /api/v1/periode`: dropdown periode di halaman Pendaftar masih memakai mock

## Autentikasi

Backend memakai dua cookie httpOnly, jadi JavaScript tidak bisa membacanya dan tidak perlu mengirimnya secara manual. Header `Authorization` tidak dibaca. Kedua cookie memakai `SameSite=Lax`, dan otomatis `Secure` kalau `GOOGLE_REDIRECT_URI` memakai `https://`.

| Cookie | Masa berlaku | Path | Isi |
|---|---|---|---|
| `access_token` | 15 menit | `/` | JWT berisi `id_pengguna` dan `email` |
| `refresh_token` | 7 hari sejak login, tidak diperpanjang saat refresh | `/auth` | String acak, hanya hash-nya yang disimpan di tabel `token_refresh` |

Semua request dari frontend harus mengirim cookie, misalnya `withCredentials: true` di axios. Origin frontend juga harus terdaftar di `CORS_ALLOWED_ORIGINS`, dan CORS hanya mengizinkan method `GET`, `POST`, dan `PATCH`.

### Siapa yang boleh login

| Akun Google | Syarat | Setelah login |
|---|---|---|
| `@student.ub.ac.id` | Email terverifikasi | Baris `pengguna` dan `mahasiswa` dibuat otomatis, lalu diarahkan ke `/dasbor` |
| `@gmail.com` | Email terverifikasi dan terdaftar di tabel `tim_mb`, misalnya lewat [`POST /api/v1/tim-mb`](#post-apiv1tim-mb-sementara) | Diarahkan ke `/tim-mb` |

### Format error

Error `409` dan error dari aturan validasi `422` membawa pesan yang bisa langsung ditampilkan ke user. Error `401`, `403`, `404`, dan `500` hanya berisi status HTTP, tanpa body.

```json
{ "message": "IPK harus di antara 0 dan 4." }
```

| Status | Arti |
|---|---|
| `400` | Body bukan JSON yang valid, atau nilai query tidak valid. Body-nya teks biasa dari framework, bukan format di atas |
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
| Tim MB berhasil login | `/tim-mb` |
| Login dibatalkan user | `/login?error=cancelled` |
| Domain email tidak diizinkan atau email belum terverifikasi | `/login?error=domain` |
| Gmail tidak terdaftar di `tim_mb`, atau baris itu sudah terikat ke akun Google lain | `/login?error=not_registered` |
| `state` tidak cocok, `code` ditolak, email sudah terikat ke akun Google lain, atau server error | `/login?error=failed` |

### `GET /auth/me`

Mengambil data user dari access token, tanpa query ke database. Berlaku untuk mahasiswa dan Tim MB.

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
| `403` | User tidak punya baris di tabel `mahasiswa`, misalnya Tim MB |

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
| `403` | User tidak punya baris di tabel `mahasiswa`, misalnya Tim MB |
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
| `is_full` | `1` atau `0` | `1`: hanya yang penuh. `0`: hanya yang belum penuh. Tidak dikirim: semuanya |
| `id_periode` | bilangan bulat | Periode yang ditampilkan, dengan aturan di bawah |

**Aturan periode:** waktu periode dibandingkan dengan jam WIB. Kalau ada beberapa periode yang berjalan bersamaan, yang dipakai sebagai default adalah yang mulai paling akhir.

| Kondisi | Mahasiswa | Tim MB |
|---|---|---|
| Tanpa `id_periode`, ada periode berjalan | Periode berjalan | Periode berjalan |
| Tanpa `id_periode`, tidak ada periode berjalan | Daftar kosong `[]` | Periode yang mulai paling akhir |
| `id_periode` milik periode berjalan | Periode itu | Periode itu |
| `id_periode` lain | `403` | Periode itu, atau `[]` kalau tidak ditemukan |

**Respons `200`:**
```json
[
  {
    "id_posisi": 1,
    "posisi": "Backend Developer Intern",
    "deskripsi_persyaratan": "Memahami REST API dan basis data.",
    "kuota_total": 5,
    "sisa_kuota": 3,
    "id_periode": 2,
    "perusahaan": {
      "id_perusahaan": 1,
      "nama_perusahaan": "PT Nusantara Digital"
    }
  }
]
```

| Status | Arti |
|---|---|
| `400` | Nilai query tidak valid, misalnya `is_full=2` atau `p=abc`. Body-nya teks biasa dari framework |
| `401` | Access token tidak valid |
| `403` | Mahasiswa meminta periode yang tidak berjalan, atau user bukan mahasiswa maupun Tim MB |

### `POST /api/v1/posisi`

Membuat posisi magang baru. Hanya untuk Tim MB, dilindungi middleware `require_tim_mb`.

**Header:** `Content-Type: application/json`

| Field | Wajib | Aturan |
|---|---|---|
| `id_perusahaan` | Ya | Harus ada di tabel `perusahaan_mitra` |
| `id_periode` | Ya | Harus ada di tabel `periode_pendaftaran`, boleh periode apa saja |
| `posisi` | Ya | 1-255 karakter, spasi di awal dan akhir dibuang |
| `deskripsi_persyaratan` | Ya | Tidak boleh kosong, spasi di awal dan akhir dibuang |
| `kuota_total` | Ya | Bilangan bulat, minimal 1 |

`sisa_kuota` tidak dikirim, karena otomatis diisi sama dengan `kuota_total`.

**Contoh body:**
```json
{
  "id_perusahaan": 1,
  "id_periode": 2,
  "posisi": "Backend Developer Intern",
  "deskripsi_persyaratan": "Memahami REST API dan basis data.",
  "kuota_total": 5
}
```

**Respons `201`:** posisi yang dibuat, dengan bentuk yang sama seperti satu item di [`GET /api/v1/posisi`](#get-apiv1posisi).

| Status | Arti |
|---|---|
| `401` | Access token tidak valid |
| `403` | User bukan Tim MB |
| `422` | Isi body ditolak, termasuk `id_perusahaan` atau `id_periode` yang tidak ditemukan |

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

| Kondisi | Mahasiswa | Tim MB |
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
| `403` | Mahasiswa meminta status selain `berlangsung`, atau user bukan mahasiswa maupun Tim MB |

### `POST /api/v1/periode`

Membuat periode pendaftaran baru. Hanya untuk Tim MB, dilindungi middleware `require_tim_mb`.

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
| `403` | User bukan Tim MB |
| `409` | Rentang waktu beririsan dengan periode lain |
| `422` | Isi body ditolak |

## Endpoint perusahaan

### `GET /api/v1/perusahaan`

Mengambil semua perusahaan mitra, diurutkan berdasarkan nama. Bisa dipakai mahasiswa dan Tim MB, misalnya untuk mengisi filter `p` di [`GET /api/v1/posisi`](#get-apiv1posisi).

**Respons `200`:**
```json
[
  {
    "id_perusahaan": 1,
    "nama_perusahaan": "PT Nusantara Digital"
  }
]
```

| Status | Arti |
|---|---|
| `401` | Access token tidak valid |
| `403` | User bukan mahasiswa maupun Tim MB |

### `POST /api/v1/perusahaan`

Membuat perusahaan mitra baru. Hanya untuk Tim MB, dilindungi middleware `require_tim_mb`.

**Header:** `Content-Type: application/json`

| Field | Wajib | Aturan |
|---|---|---|
| `nama_perusahaan` | Ya | 1-255 karakter, spasi di awal dan akhir dibuang. Tidak boleh sama dengan perusahaan lain, tanpa membedakan huruf besar dan kecil |

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
| `403` | User bukan Tim MB |
| `409` | Nama perusahaan sudah ada |
| `422` | Isi body ditolak |

## Endpoint Tim MB

### `POST /api/v1/tim-mb` (sementara)

Mendaftarkan anggota Tim MB baru, sehingga tidak perlu menulis SQL manual. Endpoint ini sementara, sampai ada alur admin yang sebenarnya.

Route ini **hanya ada kalau env `ADMIN_SECRET` di-set**, minimal 32 byte. Tanpa env itu, responsnya `404`. Header secret-nya tidak diizinkan CORS, jadi endpoint ini dipanggil lewat `curl`, bukan dari browser.

**Header:** `X-Admin-Secret` dan `Content-Type: application/json`

| Field | Wajib | Aturan |
|---|---|---|
| `email` | Ya | Alamat `@gmail.com`, disimpan dalam huruf kecil. Tulis persis seperti akun Google-nya, termasuk titik |
| `nama` | Tidak | 1-255 karakter, spasi di awal dan akhir dibuang |
| `no_hp` | Tidak | 9-15 digit angka, boleh diawali `+` |

**Contoh:**
```bash
curl -X POST http://localhost:3000/api/v1/tim-mb \
  -H "X-Admin-Secret: $ADMIN_SECRET" \
  -H "Content-Type: application/json" \
  -d '{"email":"nama@gmail.com","nama":"Nama Tim MB","no_hp":"081234567890"}'
```

**Yang dilakukan:** membuat baris `pengguna` dan `tim_mb` dalam satu statement. `google_sub` masih kosong, dan terisi saat anggota itu login pertama kali.

**Respons `201`:**
```json
{
  "id_pengguna": 2,
  "email": "nama@gmail.com",
  "nama": "Nama Tim MB",
  "no_hp": "081234567890"
}
```

| Status | Arti |
|---|---|
| `403` | Header `X-Admin-Secret` tidak ada atau salah |
| `404` | `ADMIN_SECRET` tidak di-set di server |
| `409` | Email sudah terdaftar, tanpa membedakan huruf besar dan kecil |
| `422` | Isi body ditolak |

**Cara menghapus nanti:** hapus folder `src/features/tim_mb`, barisnya di `src/features/mod.rs`, dan `admin_secret` di `Config` serta `AppState`.

## Endpoint placeholder

Route berikut hanya mengembalikan teks `TODO` dengan status `200`, belum memakai database, dan **belum dilindungi autentikasi**.

| Path | Respons saat ini |
|---|---|
| `GET /api/v1/pendaftaran` | `TODO: list pendaftaran` |
| `GET /api/v1/pendaftaran/{id}` | `TODO: detail pendaftaran {id}` |
| `GET /api/v1/templat-sp-dpa` | `TODO: list templat SP DPA` |

## Riwayat perubahan

### 4 November 2026

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
