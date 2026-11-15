#!/usr/bin/env bash
#
# Membangun image Docker backend GoMagang secara konsisten dan bisa ditelusuri.
#
# Alur:
#   1. Prasyarat   : docker dan git tersedia, daemon Docker bisa dihubungi.
#   2. Kebersihan  : working tree harus bersih, agar image selalu sama dengan satu commit.
#   3. Quality gate: cargo fmt, clippy, dan test harus lolos sebelum image dibuat.
#   4. Build       : base image ditarik ulang (patch keamanan), image diberi label OCI.
#   5. Tag         : <commit>, <versi>, dan latest.
#   6. Smoke test  : image bisa dijalankan, binary ada, dan berjalan sebagai user non-root.
#   7. Push        : opsional, ke registry yang diberikan.
#
# Pemakaian: scripts/build-image.sh [opsi]   (jalankan dengan -h untuk daftar opsi)

set -Eeuo pipefail

# ---- Konfigurasi bawaan ---------------------------------------------------------------------
IMAGE_NAME="${IMAGE_NAME:-gomagang}"
EXPECTED_UID=10001

# ---- Opsi -----------------------------------------------------------------------------------
REGISTRY=""
SKIP_CHECKS=false
ALLOW_DIRTY=false
NO_CACHE=false
EXTRA_TAGS=()

usage() {
    cat <<EOF
Pemakaian: $(basename "$0") [opsi]

Opsi:
  --push REGISTRY   Setelah build, push image ke REGISTRY (mis. ghcr.io/kresnawan).
  --tag TAG         Tambahkan tag lain (boleh diulang).
  --skip-checks     Lewati cargo fmt/clippy/test. Hanya untuk keadaan darurat.
  --allow-dirty     Izinkan build dari working tree yang belum di-commit (tag diberi -dirty).
  --no-cache        Build tanpa cache Docker.
  -h, --help        Tampilkan bantuan ini.

Variabel environment:
  IMAGE_NAME        Nama image (bawaan: gomagang).

Tag yang dihasilkan:
  ${IMAGE_NAME}:<commit pendek>, ${IMAGE_NAME}:<versi git describe>, ${IMAGE_NAME}:latest
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --push)
            [[ $# -ge 2 ]] || { echo "--push butuh nama registry" >&2; exit 2; }
            REGISTRY="${2%/}"; shift 2 ;;
        --tag)
            [[ $# -ge 2 ]] || { echo "--tag butuh nilai" >&2; exit 2; }
            EXTRA_TAGS+=("$2"); shift 2 ;;
        --skip-checks) SKIP_CHECKS=true; shift ;;
        --allow-dirty) ALLOW_DIRTY=true; shift ;;
        --no-cache) NO_CACHE=true; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Opsi tidak dikenal: $1" >&2; usage >&2; exit 2 ;;
    esac
done

# ---- Utilitas log ---------------------------------------------------------------------------
if [[ -t 1 ]]; then
    BOLD=$'\e[1m'; GREEN=$'\e[32m'; YELLOW=$'\e[33m'; RED=$'\e[31m'; RESET=$'\e[0m'
else
    BOLD=""; GREEN=""; YELLOW=""; RED=""; RESET=""
fi
step() { echo; echo "${BOLD}==> $*${RESET}"; }
ok()   { echo "${GREEN}✔ $*${RESET}"; }
warn() { echo "${YELLOW}! $*${RESET}" >&2; }
die()  { echo "${RED}✘ $*${RESET}" >&2; exit 1; }

STARTED_AT=$(date +%s)
trap 'die "Gagal di baris $LINENO: $BASH_COMMAND"' ERR

# Selalu bekerja dari root proyek, dari mana pun script dipanggil.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# ---- 1. Prasyarat ---------------------------------------------------------------------------
step "Memeriksa prasyarat"
command -v docker >/dev/null || die "docker tidak ditemukan."
command -v git >/dev/null || die "git tidak ditemukan."
docker info >/dev/null 2>&1 || die "Daemon Docker tidak bisa dihubungi. Apakah service docker berjalan?"
[[ -f Dockerfile ]] || die "Dockerfile tidak ditemukan di $ROOT."
git rev-parse --git-dir >/dev/null 2>&1 || die "$ROOT bukan repository git."
ok "docker dan git siap"

# ---- 2. Kebersihan working tree dan versi ---------------------------------------------------
step "Menentukan versi"
DIRTY=false
if [[ -n "$(git status --porcelain)" ]]; then
    DIRTY=true
    if [[ "$ALLOW_DIRTY" != true ]]; then
        git status --short >&2
        die "Ada perubahan yang belum di-commit. Commit dulu, atau pakai --allow-dirty untuk build uji."
    fi
    warn "Build dari working tree yang belum di-commit; image tidak bisa ditelusuri ke satu commit."
fi

GIT_SHA="$(git rev-parse --short=12 HEAD)"
GIT_SHA_FULL="$(git rev-parse HEAD)"
# Tag git terdekat bila ada (mis. v1.2.0), selain itu tanggal + commit.
VERSION="$(git describe --tags --exact-match 2>/dev/null || echo "$(date -u +%Y%m%d)-${GIT_SHA}")"
if [[ "$DIRTY" == true ]]; then
    GIT_SHA="${GIT_SHA}-dirty"
    VERSION="${VERSION}-dirty"
fi
CREATED="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
SOURCE_URL="$(git config --get remote.origin.url 2>/dev/null || true)"

TAGS=("${IMAGE_NAME}:${GIT_SHA}" "${IMAGE_NAME}:${VERSION}" "${IMAGE_NAME}:latest")
for tag in "${EXTRA_TAGS[@]}"; do
    TAGS+=("${IMAGE_NAME}:${tag}")
done
ok "versi ${VERSION} (commit ${GIT_SHA})"

# ---- 3. Quality gate ------------------------------------------------------------------------
if [[ "$SKIP_CHECKS" == true ]]; then
    step "Quality gate DILEWATI (--skip-checks)"
    warn "Image dibuat tanpa memeriksa format, lint, dan test."
else
    command -v cargo >/dev/null || die "cargo tidak ditemukan; dibutuhkan untuk quality gate."

    step "cargo fmt --check"
    cargo fmt --check || die "Format kode belum rapi. Jalankan: cargo fmt"
    ok "format rapi"

    step "cargo clippy (warning dianggap error)"
    cargo clippy --all-targets --locked -- -D warnings
    ok "clippy bersih"

    # Test memakai PostgreSQL dari DATABASE_URL (dibaca dari .env oleh sqlx::test).
    step "cargo test"
    cargo test --locked || die "Ada test yang gagal; image tidak dibuat."
    ok "semua test lulus"
fi

# ---- 4. Build -------------------------------------------------------------------------------
step "Membangun image"
BUILD_ARGS=(
    --pull
    --label "org.opencontainers.image.title=gomagang"
    --label "org.opencontainers.image.description=Backend sistem informasi pendaftaran magang"
    --label "org.opencontainers.image.version=${VERSION}"
    --label "org.opencontainers.image.revision=${GIT_SHA_FULL}"
    --label "org.opencontainers.image.created=${CREATED}"
)
[[ -n "$SOURCE_URL" ]] && BUILD_ARGS+=(--label "org.opencontainers.image.source=${SOURCE_URL}")
[[ "$NO_CACHE" == true ]] && BUILD_ARGS+=(--no-cache)
for tag in "${TAGS[@]}"; do
    BUILD_ARGS+=(-t "$tag")
done

docker build "${BUILD_ARGS[@]}" .
ok "image dibuat"

# ---- 5. Smoke test --------------------------------------------------------------------------
# Tidak menjalankan server (butuh database dan secret); cukup memastikan isi image benar.
step "Smoke test image"
PRIMARY="${IMAGE_NAME}:${GIT_SHA}"
RUN_UID="$(docker run --rm --entrypoint sh "$PRIMARY" -c 'test -x /usr/local/bin/gomagang && id -u')" \
    || die "Binary /usr/local/bin/gomagang tidak ada atau tidak bisa dijalankan di image."
[[ "$RUN_UID" == "$EXPECTED_UID" ]] \
    || die "Image berjalan sebagai UID ${RUN_UID}, seharusnya ${EXPECTED_UID} (non-root)."
UPLOAD_DIR="$(docker image inspect "$PRIMARY" \
    --format '{{range .Config.Env}}{{println .}}{{end}}' | sed -n 's/^UPLOAD_DIR=//p')"
[[ "$UPLOAD_DIR" == "/data/uploads" ]] || die "UPLOAD_DIR di image bukan /data/uploads."
ok "binary ada, berjalan sebagai UID ${RUN_UID}, UPLOAD_DIR=${UPLOAD_DIR}"

# ---- 6. Push (opsional) ---------------------------------------------------------------------
if [[ -n "$REGISTRY" ]]; then
    [[ "$DIRTY" == true ]] && die "Menolak push image -dirty ke registry. Commit dulu."
    step "Push ke ${REGISTRY}"
    for tag in "${TAGS[@]}"; do
        remote="${REGISTRY}/${tag}"
        docker tag "$tag" "$remote"
        docker push "$remote"
    done
    ok "push selesai"
fi

# ---- Ringkasan ------------------------------------------------------------------------------
SIZE="$(docker image inspect "$PRIMARY" --format '{{.Size}}' | awk '{printf "%.0f MB", $1/1000000}')"
ELAPSED=$(( $(date +%s) - STARTED_AT ))
step "Selesai dalam ${ELAPSED} detik"
echo "  Versi : ${VERSION}"
echo "  Commit: ${GIT_SHA_FULL}"
echo "  Ukuran: ${SIZE}"
echo "  Tag   :"
for tag in "${TAGS[@]}"; do
    echo "    - ${tag}${REGISTRY:+  (juga ${REGISTRY}/${tag})}"
done
