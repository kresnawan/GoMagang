#!/usr/bin/env bash
#
# Membangun image backend GoMagang di laptop (Docker) lalu men-deploy-nya ke VPS sebagai container
# Podman rootless lewat SSH.
#
# Alur:
#   1. Koneksi    : satu koneksi SSH dipakai bersama (passphrase kunci hanya ditanya sekali).
#   2. Preflight  : VPS diperiksa lebih dulu (Podman, .env.production, container/network Postgres,
#                   role dan database) sebelum build yang lama dimulai.
#   3. Persiapan  : bila perlu dan disetujui, backend dan Postgres disambungkan ke satu network,
#                   serta role dan database dibuat dari DATABASE_URL di .env.production.
#   4. Build      : scripts/build-image.sh (fmt, clippy, test, build, smoke test).
#   5. Kirim      : docker save | gzip | ssh | podman load, dilewati bila image sudah ada di VPS.
#   6. Jalankan   : container gomagang-backend diganti (podman run --restart unless-stopped), tunggu
#                   GET /health 200; bila gagal, rollback otomatis ke versi sebelumnya.
#   7. Bersihkan  : image lama di VPS dihapus, menyisakan beberapa yang terbaru.
#
# Secret tidak pernah dikirim dari laptop: .env.production hanya ada di folder deploy di VPS.
#
# Pemakaian: scripts/deploy.sh [opsi]   (jalankan dengan -h untuk daftar opsi)

set -Eeuo pipefail

IMAGE_NAME=gomagang
# Nama lengkap di VPS. Podman menamai image tanpa registry sebagai docker.io/library/..., jadi
# image dikirim dengan nama localhost/... supaya tidak pernah dicari di Docker Hub.
REMOTE_IMAGE=localhost/gomagang
KEEP_IMAGES=3

DEPLOY_HOST="${DEPLOY_HOST:-kresnawan@ssh.kresnawan.com}"
DEPLOY_DIR="${DEPLOY_DIR:-gomagang}"
PG_CONTAINER="${PG_CONTAINER:-}"
PG_NETWORK="${PG_NETWORK:-}"
GOMAGANG_BIND="${GOMAGANG_BIND:-127.0.0.1:3000}"

TAG=""
SKIP_BUILD=false
ASSUME_YES=false
BUILD_FLAGS=()

usage() {
    cat <<EOF
Pemakaian: $(basename "$0") [opsi]

Opsi:
  --host USER@HOST      Tujuan SSH (bawaan: ${DEPLOY_HOST}).
  --dir PATH            Folder deploy di VPS, relatif dari home (bawaan: ${DEPLOY_DIR}).
  --pg-container NAMA   Container PostgreSQL di VPS (bawaan: dideteksi otomatis).
  --pg-network NAMA     Network Podman Postgres (bawaan: network buatan user milik container itu).
  --bind ALAMAT:PORT    Port backend di VPS (bawaan: ${GOMAGANG_BIND}).
  --tag TAG             Deploy image gomagang:TAG yang sudah ada tanpa build, mis. untuk rollback.
  --skip-build          Pakai image commit saat ini yang sudah di-build sebelumnya.
  --skip-checks         Teruskan ke build-image.sh: lewati fmt/clippy/test (darurat saja).
  --no-cache            Teruskan ke build-image.sh: build tanpa cache Docker.
  -y, --yes             Jangan minta konfirmasi.
  -h, --help            Tampilkan bantuan ini.

Semua opsi juga bisa diatur lewat environment: DEPLOY_HOST, DEPLOY_DIR, PG_CONTAINER,
PG_NETWORK, GOMAGANG_BIND, serta PG_SUPERUSER (bawaan: POSTGRES_USER container, atau postgres).
EOF
}

need_value() { [[ $# -ge 2 && -n "$2" ]] || { echo "$1 butuh nilai" >&2; exit 2; }; }
while [[ $# -gt 0 ]]; do
    case "$1" in
        --host) need_value "$@"; DEPLOY_HOST="$2"; shift 2 ;;
        --dir) need_value "$@"; DEPLOY_DIR="$2"; shift 2 ;;
        --pg-container) need_value "$@"; PG_CONTAINER="$2"; shift 2 ;;
        --pg-network) need_value "$@"; PG_NETWORK="$2"; shift 2 ;;
        --bind) need_value "$@"; GOMAGANG_BIND="$2"; shift 2 ;;
        --tag) need_value "$@"; TAG="$2"; SKIP_BUILD=true; shift 2 ;;
        --skip-build) SKIP_BUILD=true; shift ;;
        --skip-checks|--no-cache) BUILD_FLAGS+=("$1"); shift ;;
        -y|--yes) ASSUME_YES=true; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Opsi tidak dikenal: $1" >&2; usage >&2; exit 2 ;;
    esac
done

# ---- Utilitas -------------------------------------------------------------------------------
if [[ -t 1 ]]; then
    BOLD=$'\e[1m'; GREEN=$'\e[32m'; YELLOW=$'\e[33m'; RED=$'\e[31m'; RESET=$'\e[0m'
else
    BOLD=""; GREEN=""; YELLOW=""; RED=""; RESET=""
fi
step() { echo; echo "${BOLD}==> $*${RESET}"; }
ok()   { echo "${GREEN}✔ $*${RESET}"; }
warn() { echo "${YELLOW}! $*${RESET}" >&2; }
die()  { echo "${RED}✘ $*${RESET}" >&2; exit 1; }

confirm() {
    [[ "$ASSUME_YES" == true ]] && return 0
    [[ -r /dev/tty ]] || die "Butuh konfirmasi, tetapi tidak ada terminal. Pakai --yes."
    local answer
    read -r -p "$1 [y/N] " answer </dev/tty
    [[ "$answer" == [yY]* ]]
}

STARTED_AT=$(date +%s)
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
REMOTE_SCRIPT="$ROOT/scripts/deploy-remote.sh"

[[ "$DEPLOY_HOST" == *@* ]] || die "--host harus berbentuk user@host."

# ---- 1. Koneksi SSH bersama -----------------------------------------------------------------
CTRL_DIR="$(mktemp -d "${TMPDIR:-/tmp}/gomagang-deploy.XXXXXX")"
SSH_OPTS=(-o ControlPath="$CTRL_DIR/cm" -o ServerAliveInterval=30 -o ServerAliveCountMax=4)

cleanup() {
    ssh "${SSH_OPTS[@]}" -O exit "$DEPLOY_HOST" >/dev/null 2>&1 || true
    rm -rf "$CTRL_DIR"
}
trap cleanup EXIT
trap 'die "Gagal di baris $LINENO: $BASH_COMMAND"' ERR

# ssh ke VPS lewat koneksi bersama. Argumen di-quote supaya tidak dipecah ulang oleh shell VPS.
vps() { ssh "${SSH_OPTS[@]}" -o ControlMaster=no "$DEPLOY_HOST" "$@"; }
remote() { vps "bash -s -- $(printf '%q ' "$@")" <"$REMOTE_SCRIPT"; }

step "Menghubungkan ke ${DEPLOY_HOST}"
command -v docker >/dev/null || die "docker tidak ditemukan."
command -v git >/dev/null || die "git tidak ditemukan."
ssh "${SSH_OPTS[@]}" -o ControlMaster=yes -o ControlPersist=yes -o ConnectTimeout=30 -fN "$DEPLOY_HOST" \
    || die "SSH ke ${DEPLOY_HOST} gagal. Coba manual: ssh ${DEPLOY_HOST}"
ok "terhubung"

# ---- 2. Preflight VPS -----------------------------------------------------------------------
step "Memeriksa VPS"
DIR_Q="$(printf '%q' "$DEPLOY_DIR")"
# Templat env selalu dikirim, supaya selalu cocok dengan commit ini.
vps "mkdir -p $DIR_Q && cat > $DIR_Q/.env.production.example" <"$ROOT/scripts/env.production.example"

declare -A PF=()
PREFLIGHT="$(remote preflight "$DEPLOY_DIR" "$PG_CONTAINER" "$PG_NETWORK" "$GOMAGANG_BIND")" \
    || die "Preflight VPS gagal (lihat pesan [vps] di atas)."
while IFS='=' read -r key value; do
    [[ -n "$key" ]] && PF["$key"]="$value"
done <<<"$PREFLIGHT"

REMOTE_ARCH="${PF[ARCH]}"
LOCAL_ARCH="$(docker info -f '{{.Architecture}}')"
[[ "$REMOTE_ARCH" == "$LOCAL_ARCH" ]] \
    || die "Arsitektur VPS (${REMOTE_ARCH}) berbeda dengan laptop (${LOCAL_ARCH}); image tidak akan berjalan."
PG_CONTAINER="${PF[PG_CONTAINER]}"
PG_NETWORK="${PF[PG_NETWORK]}"
CURRENT_TAG="${PF[CURRENT_TAG]:-}"
ok "Podman rootless ${REMOTE_ARCH}, Postgres '${PG_CONTAINER}' di network '${PG_NETWORK}', folder ${PF[DEPLOY_DIR]}"

# ---- 3. Persiapan network dan database ------------------------------------------------------
if [[ "${PF[NEED_NETWORK]:-0}" == 1 ]]; then
    step "Network Postgres"
    echo "Container '${PG_CONTAINER}' belum ada di network '${PG_NETWORK}'. Network itu akan dibuat bila"
    echo "belum ada, lalu '${PG_CONTAINER}' disambungkan ke sana (podman network connect, tanpa restart)."
    confirm "Lanjutkan?" || die "Dibatalkan. Atau pilih network yang sudah ada dengan --pg-network."
    remote ensure-network "$PG_CONTAINER" "$PG_NETWORK"
    ok "network siap"
fi

if [[ "${PF[DB_ROLE_EXISTS]}" != 1 || "${PF[DB_EXISTS]}" != 1 ]]; then
    step "Database"
    [[ "${PF[DB_ROLE_EXISTS]}" != 1 ]] && echo "Role '${PF[DB_USER]}' belum ada."
    [[ "${PF[DB_EXISTS]}" != 1 ]] && echo "Database '${PF[DB_NAME]}' belum ada."
    echo "Keduanya akan dibuat di '${PG_CONTAINER}' dengan password dari DATABASE_URL di .env.production."
    confirm "Buat sekarang?" || die "Dibatalkan. Buat role dan database dulu, lalu deploy ulang."
    remote ensure-db "$DEPLOY_DIR" "$PG_CONTAINER"
    ok "role dan database siap"
fi

# ---- 4. Build -------------------------------------------------------------------------------
if [[ -z "$TAG" ]]; then
    [[ -z "$(git status --porcelain)" ]] \
        || die "Ada perubahan yang belum di-commit. Deploy hanya dari commit, supaya versi di VPS bisa ditelusuri."
    TAG="$(git rev-parse --short=12 HEAD)"
fi

if [[ "$SKIP_BUILD" == true ]]; then
    step "Build dilewati, memakai ${IMAGE_NAME}:${TAG}"
else
    step "Membangun image ${IMAGE_NAME}:${TAG}"
    IMAGE_NAME="$IMAGE_NAME" "$ROOT/scripts/build-image.sh" "${BUILD_FLAGS[@]}"
fi

if [[ "$TAG" == "$CURRENT_TAG" ]]; then
    warn "${IMAGE_NAME}:${TAG} sudah berjalan di VPS; container-nya akan dibuat ulang dengan versi yang sama."
fi

echo
echo "  Host       : ${DEPLOY_HOST}"
echo "  Versi kini : ${CURRENT_TAG:-(belum pernah deploy)}"
echo "  Versi baru : ${TAG}"
echo "  Port       : ${GOMAGANG_BIND}"
confirm "Deploy ${IMAGE_NAME}:${TAG} ke VPS?" || die "Dibatalkan."

# ---- 5. Kirim image -------------------------------------------------------------------------
step "Mengirim image"
if remote has-image "$TAG"; then
    ok "${IMAGE_NAME}:${TAG} sudah ada di VPS, tidak dikirim ulang"
else
    docker image inspect "${IMAGE_NAME}:${TAG}" >/dev/null 2>&1 \
        || die "Image ${IMAGE_NAME}:${TAG} tidak ada di laptop maupun VPS."
    SIZE="$(docker image inspect "${IMAGE_NAME}:${TAG}" -f '{{.Size}}' | awk '{printf "%.0f MB", $1/1000000}')"
    echo "Ukuran image ${SIZE} (sebelum kompresi)."
    # Tag sementara, supaya arsip membawa nama localhost/gomagang:TAG ke Podman.
    docker tag "${IMAGE_NAME}:${TAG}" "${REMOTE_IMAGE}:${TAG}"
    if command -v pv >/dev/null; then
        docker save "${REMOTE_IMAGE}:${TAG}" | pv | gzip -1 | vps "gunzip | podman load"
    else
        docker save "${REMOTE_IMAGE}:${TAG}" | gzip -1 | vps "gunzip | podman load"
    fi
    docker image rm "${REMOTE_IMAGE}:${TAG}" >/dev/null
    remote has-image "$TAG" || die "Image ${REMOTE_IMAGE}:${TAG} tidak muncul di VPS setelah dikirim."
    ok "terkirim"
fi

# ---- 6. Jalankan ----------------------------------------------------------------------------
step "Menjalankan di VPS"
remote up "$DEPLOY_DIR" "$TAG" "$PG_NETWORK" "$GOMAGANG_BIND" >/dev/null \
    || die "Deploy gagal. Versi sebelumnya dijalankan lagi bila ada (lihat pesan [vps] di atas)."
ok "${IMAGE_NAME}:${TAG} berjalan dan sehat"

# ---- 7. Bersihkan ---------------------------------------------------------------------------
step "Membersihkan image lama di VPS"
remote prune "$DEPLOY_DIR" "$KEEP_IMAGES" || warn "Gagal membersihkan image lama; deploy tetap berhasil."

ELAPSED=$(( $(date +%s) - STARTED_AT ))
step "Selesai dalam ${ELAPSED} detik"
echo "  Versi   : ${TAG}${CURRENT_TAG:+ (sebelumnya ${CURRENT_TAG})}"
echo "  Rollback: scripts/deploy.sh --tag ${CURRENT_TAG:-<tag>}"
echo "  Log     : ssh ${DEPLOY_HOST} podman logs -f gomagang-backend"
echo "  Status  : ssh ${DEPLOY_HOST} podman ps --filter name=gomagang-backend"
