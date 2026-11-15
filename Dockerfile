# ---- Tahap 1: build ---------------------------------------------------------------------------
# Versi Rust disamakan dengan toolchain pengembangan. Builder dan runtime sama-sama Debian trixie,
# sehingga binary yang dihasilkan cocok dengan glibc di image runtime.
FROM rust:1.98-slim-trixie AS build
WORKDIR /src

# Dependency di-compile lebih dulu dengan kode dummy. Layer ini hanya dibuat ulang bila
# Cargo.toml atau Cargo.lock berubah, sehingga mengubah kode aplikasi tidak meng-compile ulang
# seluruh dependency. Tidak memakai fitur BuildKit, jadi bisa dibangun dengan builder Docker mana
# pun.
COPY Cargo.toml Cargo.lock build.rs ./
RUN mkdir src \
    && echo "fn main() {}" > src/main.rs \
    && touch src/lib.rs \
    && cargo build --release --locked \
    && rm -rf src

# Semua yang dibaca saat compile: kode, migration (ditanam oleh sqlx::migrate!), serta templat
# dan gambar email (ditanam oleh include_str!/include_bytes!). Tidak ada query yang dicek ke
# database saat compile, sehingga build tidak membutuhkan DATABASE_URL.
COPY src ./src
COPY migrations ./migrations
COPY templates ./templates
COPY assets ./assets

# touch: COPY mempertahankan waktu ubah file asli, yang bisa lebih tua dari hasil build dummy,
# sehingga cargo bisa mengira kode aplikasi tidak berubah.
RUN touch src/main.rs src/lib.rs build.rs \
    && cargo build --release --locked \
    && cp target/release/gomagang /gomagang

# ---- Tahap 2: runtime -------------------------------------------------------------------------
FROM debian:trixie-slim

# Sertifikat CA untuk koneksi TLS keluar (Google OAuth, SMTP, dan database bila memakai TLS).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Aplikasi berjalan sebagai user non-root dengan UID tetap, agar permission folder upload di host
# bisa disiapkan untuk UID yang sama.
ARG APP_UID=10001
RUN useradd --system --uid ${APP_UID} --no-create-home --shell /usr/sbin/nologin gomagang \
    && mkdir -p /data/uploads \
    && chown -R gomagang:gomagang /data

COPY --from=build /gomagang /usr/local/bin/gomagang

# Berkas unggahan disimpan di path tetap ini, yang di-mount ke folder di host saat dijalankan.
ENV UPLOAD_DIR=/data/uploads
VOLUME ["/data/uploads"]

USER gomagang
WORKDIR /data
EXPOSE 3000

CMD ["gomagang"]
