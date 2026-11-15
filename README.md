# GoMagang

Backend sistem informasi pendaftaran magang, ditulis dengan Rust (Axum + PostgreSQL). Daftar
endpoint dan aturannya ada di [DOCUMENTATION.md](DOCUMENTATION.md).

## Menjalankan saat development

```bash
cargo run
```

Konfigurasi dibaca dari file `.env` di folder proyek. Server berjalan di `http://localhost:3000`
dan tidak mencetak apa pun saat berhasil start; log hanya muncul bila ada error.

## Docker

Image hanya berisi backend. PostgreSQL berjalan terpisah, dan berkas unggahan disimpan di folder
host yang di-mount ke `/data/uploads` di dalam container. Semua perintah di bawah dijalankan dari
folder proyek ini.

### 1. Membuat image setelah ada perubahan kode

```bash
docker build -t gomagang:latest .
```

- Titik (`.`) di akhir wajib: artinya "bangun dari folder ini".
- Image lama dengan nama yang sama otomatis digantikan. Image yang sedang dipakai container tidak
  ikut berubah sampai container itu dijalankan ulang (lihat langkah 3).
- Build pertama memakan beberapa menit. Build berikutnya lebih cepat selama `Cargo.toml` dan
  `Cargo.lock` tidak berubah, karena dependency sudah ter-cache.
- Migration, templat email, dan logo ikut tertanam di image. Folder `uploads/` dan file `.env`
  tidak ikut masuk (lihat `.dockerignore`).

Opsional, beri juga tag versi supaya bisa kembali ke versi sebelumnya bila ada masalah:

```bash
docker build -t gomagang:latest -t gomagang:2026-11-13 .
```

### 2. Menjalankan image sebagai container (tes di laptop)

Siapkan sekali saja:

```bash
# Folder unggahan untuk container. Aplikasi di dalam container berjalan sebagai UID 10001.
mkdir -p ~/gomagang-docker-uploads && chmod 777 ~/gomagang-docker-uploads

# File env khusus Docker
cp .env .env.docker
```

Di `.env.docker`, **hapus tanda kutip** pada nilai, misalnya
`SMTP_FROM="Panitia Magang <...>"` menjadi `SMTP_FROM=Panitia Magang <...>`. `docker run` tidak
menghapus tanda kutip, sehingga nilainya akan rusak. `.env` untuk `cargo run` justru tetap
memakai tanda kutip.

Jalankan:

```bash
docker run --rm --name gomagang --network host \
  --env-file .env.docker \
  -v ~/gomagang-docker-uploads:/data/uploads \
  gomagang:latest
```

- `--network host` membuat `localhost` di dalam container sama dengan `localhost` laptop, jadi
  `DATABASE_URL=...@localhost:5432/...` bisa dipakai apa adanya.
- Hentikan `cargo run` lebih dulu, karena keduanya memakai port 3000.
- Terminal akan tampak diam; itu tanda server sedang berjalan. Cek dengan membuka
  `http://localhost:3000/auth/me` (jawaban `401` berarti server hidup).
- Hentikan dengan `Ctrl+C`. Karena memakai `--rm`, container langsung terhapus setelah berhenti.

Untuk menjalankannya di latar belakang, tambahkan `-d`:

```bash
docker run -d --rm --name gomagang --network host \
  --env-file .env.docker \
  -v ~/gomagang-docker-uploads:/data/uploads \
  gomagang:latest

docker logs -f gomagang   # melihat log
docker stop gomagang      # menghentikan
```

### 3. Memperbarui container ke image terbaru

Container yang sudah jalan tidak otomatis memakai image baru. Setelah build ulang:

```bash
docker build -t gomagang:latest .
docker rm -f gomagang      # hentikan dan hapus container lama
# lalu jalankan lagi perintah `docker run` di langkah 2
```

Data aman: berkas unggahan ada di folder host, dan database ada di PostgreSQL. Migration baru
dijalankan otomatis saat container start.

Image lama yang sudah tidak terpakai bisa dibersihkan dengan:

```bash
docker image prune
```

### Menjalankan di VPS

Di server produksi, jangan pakai `--network host`. Batasi port ke `127.0.0.1` dan buka ke internet
lewat reverse proxy (Nginx/Caddy) dengan HTTPS:

```bash
docker run -d --name gomagang --restart unless-stopped \
  --env-file /srv/gomagang/.env.production \
  -v /srv/gomagang/uploads:/data/uploads \
  -p 127.0.0.1:3000:3000 \
  --add-host host.docker.internal:host-gateway \
  gomagang:latest
```

- `/srv/gomagang/uploads` harus bisa ditulis UID 10001:
  `sudo chown -R 10001:10001 /srv/gomagang/uploads`.
- Di `.env.production`, `DATABASE_URL` memakai `host.docker.internal`, bukan `localhost`, dan
  PostgreSQL di host harus menerima koneksi dari jaringan Docker (`listen_addresses` dan
  `pg_hba.conf`).
- `--restart unless-stopped` membuat container hidup lagi setelah VPS reboot.

### Masalah yang sering muncul

| Gejala | Penyebab | Solusi |
|---|---|---|
| `failed to connect to the database: PoolTimedOut` | Container tidak bisa menjangkau PostgreSQL, misalnya jalan tanpa `--network host` tetapi `DATABASE_URL` memakai `localhost` | Di laptop pakai `--network host`; di VPS pakai `host.docker.internal` dan atur PostgreSQL |
| `Address already in use` | Port 3000 sudah dipakai, biasanya oleh `cargo run` atau container lama | Hentikan `cargo run`, atau `docker rm -f gomagang` |
| `Conflict. The container name "/gomagang" is already in use` | Container lama dengan nama yang sama masih ada | `docker rm -f gomagang`, lalu jalankan lagi |
| Server berhenti saat start karena `SMTP_FROM` tidak valid | Nilai di file env masih memakai tanda kutip | Hapus tanda kutip di `.env.docker` / `.env.production` |
| Gagal menyimpan berkas unggahan | Folder upload di host tidak bisa ditulis UID 10001 | `chmod 777` (laptop) atau `chown 10001:10001` (VPS) |
