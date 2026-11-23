#!/usr/bin/env bash
#
# Bagian deploy yang berjalan di VPS (Podman rootless + Quadlet). Tidak dijalankan langsung, tetapi
# dikirim oleh scripts/deploy.sh lewat SSH:  ssh host bash -s -- <perintah> [argumen...] < deploy-remote.sh
#
# Semua yang membaca secret (.env.production, password database) terjadi di sini, sehingga secret
# tidak pernah keluar dari VPS. Hasil yang perlu dibaca laptop ditulis ke stdout sebagai KUNCI=nilai;
# pesan untuk manusia ditulis ke stderr.
#
# Perintah:
#   preflight      DIR PG_CONTAINER PG_NETWORK BIND   Memeriksa VPS dan mencari container/network Postgres
#   ensure-network PG_CONTAINER NETWORK               Membuat network dan menyambungkan Postgres ke sana
#   ensure-db      DIR PG_CONTAINER                   Membuat role dan database dari DATABASE_URL
#   enable-linger                                     Menyalakan linger agar service jalan setelah reboot
#   has-image      TAG                                Keluar 0 bila image sudah ada
#   up             DIR TAG PG_NETWORK BIND            Menjalankan TAG, menunggu sehat, rollback bila gagal
#   prune          DIR KEEP                           Menghapus image lama, menyisakan KEEP terbaru

set -Eeuo pipefail

IMAGE_REPO=localhost/gomagang
CONTAINER=gomagang-backend
SERVICE=gomagang-backend.service
QUADLET_DIR="$HOME/.config/containers/systemd"
QUADLET_FILE="$QUADLET_DIR/gomagang-backend.container"
TEMPLATE_NAME=gomagang-backend.container.in
STATE_NAME=deploy-state
HEALTH_TIMEOUT=90

# Sesi SSH non-interaktif kadang tidak membawa variabel ini, padahal systemctl --user
# membutuhkannya. Hanya diisi bila foldernya memang ada; folder yang tidak ada membuat podman gagal.
if [[ -z "${XDG_RUNTIME_DIR:-}" && -d "/run/user/$(id -u)" ]]; then
    export XDG_RUNTIME_DIR="/run/user/$(id -u)"
fi
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" && -S "${XDG_RUNTIME_DIR:-/nonexistent}/bus" ]]; then
    export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus"
fi

say() { echo "  [vps] $*" >&2; }
die() { echo "  [vps] GAGAL: $*" >&2; exit 1; }
out() { printf '%s=%s\n' "$1" "$2"; }
user_systemctl() { systemctl --user "$@"; }

# Folder deploy: path relatif dihitung dari home user SSH.
resolve_dir() {
    case "$1" in
        /*) printf '%s' "$1" ;;
        *) printf '%s/%s' "$HOME" "$1" ;;
    esac
}

# Membaca satu variabel dari file env tanpa men-source-nya (file berisi secret, bukan script).
# Nilai diambil apa adanya, sama seperti podman --env-file.
env_get() {
    local file="$1" key="$2" line
    [[ -f "$file" ]] || return 0
    line="$(grep -E "^${key}=" "$file" | tail -n 1 || true)"
    [[ -n "$line" ]] || return 0
    line="${line#*=}"
    printf '%s' "${line%$'\r'}"
}

urldecode() {
    local s="${1//\\/\\\\}"
    printf '%b' "${s//%/\\x}"
}

# Memecah DATABASE_URL menjadi DB_USER, DB_PASS, DB_HOST, DB_PORT, DB_NAME.
parse_database_url() {
    local url="$1" rest userinfo hostpart hostport
    [[ "$url" =~ ^postgres(ql)?:// ]] || die "DATABASE_URL harus diawali postgres:// atau postgresql://"
    rest="${url#*://}"
    [[ "$rest" == *@* ]] || die "DATABASE_URL harus berisi user dan password (user:password@host)."
    userinfo="${rest%@*}"
    hostpart="${rest##*@}"
    hostport="${hostpart%%/*}"
    DB_NAME="${hostpart#*/}"
    DB_NAME="${DB_NAME%%\?*}"
    [[ "$hostpart" == */* && -n "$DB_NAME" ]] || die "DATABASE_URL tidak menyebut nama database."
    DB_HOST="${hostport%%:*}"
    DB_PORT=5432
    [[ "$hostport" == *:* ]] && DB_PORT="${hostport##*:}"
    DB_USER="$(urldecode "${userinfo%%:*}")"
    DB_PASS=""
    [[ "$userinfo" == *:* ]] && DB_PASS="$(urldecode "${userinfo#*:}")"
    DB_NAME="$(urldecode "$DB_NAME")"
    [[ -n "$DB_USER" ]] || die "DATABASE_URL tidak menyebut user."
}

# Nilai untuk \set psql di dalam tanda kutip tunggal.
psql_quote() {
    local s="${1//\\/\\\\}"
    printf "'%s'" "${s//\'/\'\'}"
}

pg_superuser() {
    local pg="$1" su
    su="${PG_SUPERUSER:-}"
    if [[ -z "$su" ]]; then
        su="$(podman inspect -f '{{range .Config.Env}}{{println .}}{{end}}' "$pg" \
            | sed -n 's/^POSTGRES_USER=//p' | head -n 1)"
    fi
    printf '%s' "${su:-postgres}"
}

# Menjalankan SQL dari stdin sebagai superuser lewat socket lokal di container Postgres.
psql_su() {
    local pg="$1"; shift
    podman exec -i "$pg" psql -X -q -v ON_ERROR_STOP=1 -U "$(pg_superuser "$pg")" -d postgres "$@"
}

networks_of() {
    podman inspect -f '{{range $k, $v := .NetworkSettings.Networks}}{{println $k}}{{end}}' "$1" \
        | sed '/^$/d'
}

# Nama yang bisa dipakai untuk menghubungi container di network NET: nama container dan alias-nya.
names_on_network() {
    local container="$1" net="$2"
    podman inspect -f '{{.Name}}' "$container" | sed 's#^/##'
    podman inspect -f "{{with index .NetworkSettings.Networks \"$net\"}}{{range .Aliases}}{{println .}}{{end}}{{end}}" \
        "$container" 2>/dev/null | sed '/^$/d' || true
}

bind_port() { printf '%s' "${1##*:}"; }

bind_host() {
    local bind="$1" host=127.0.0.1
    if [[ "$bind" == *:* ]]; then
        host="${bind%:*}"
        [[ "$host" == 0.0.0.0 || -z "$host" ]] && host=127.0.0.1
    fi
    printf '%s' "$host"
}

# GET /health tanpa curl, lewat /dev/tcp bawaan bash. Keluar 0 hanya bila status HTTP 200.
http_health() {
    local host="$1" port="$2"
    (
        exec 3<>"/dev/tcp/${host}/${port}" || exit 1
        printf 'GET /health HTTP/1.0\r\nHost: %s\r\n\r\n' "$host" >&3
        IFS= read -r -t 5 status <&3 || exit 1
        [[ "$status" == *" 200 "* ]]
    ) 2>/dev/null
}

# Generator Quadlet, untuk memeriksa file .container sebelum dipasang. Lokasinya beda per distro.
quadlet_generator() {
    local g
    for g in /usr/lib/systemd/system-generators/podman-system-generator \
        /usr/libexec/podman/quadlet /usr/lib/podman/quadlet; do
        [[ -x "$g" ]] && { printf '%s' "$g"; return 0; }
    done
    return 1
}

cmd_preflight() {
    local dir pg_container="$2" pg_network="$3" bind="$4" envfile key candidates hosts port arch mode
    dir="$(resolve_dir "$1")"

    command -v podman >/dev/null || die "podman tidak ditemukan di VPS."
    podman info >/dev/null 2>&1 || die "podman info gagal untuk user $(id -un)."
    [[ "$(podman info -f '{{.Host.Security.Rootless}}')" == true ]] \
        || die "Script ini untuk Podman rootless, tetapi podman berjalan sebagai root."
    user_systemctl show-environment >/dev/null 2>&1 \
        || die "systemd user (systemctl --user) tidak bisa dihubungi. Jalankan sekali: sudo loginctl enable-linger $(id -un)"
    quadlet_generator >/dev/null || die "Quadlet tidak ditemukan; butuh Podman 4.4 atau lebih baru."

    arch="$(podman info -f '{{.Host.Arch}}')"
    case "$arch" in
        amd64) arch=x86_64 ;;
        arm64) arch=aarch64 ;;
    esac
    out ARCH "$arch"
    out LINGER "$(loginctl show-user "$(id -un)" -p Linger --value 2>/dev/null || echo unknown)"

    mkdir -p "$dir"
    [[ -f "$dir/$TEMPLATE_NAME" ]] || die "Templat $TEMPLATE_NAME belum terkirim ke $dir."
    envfile="$dir/.env.production"
    if [[ ! -f "$envfile" ]]; then
        die "$envfile belum ada. Salin dari $dir/.env.production.example, isi nilainya, lalu deploy ulang."
    fi
    chmod 600 "$envfile"
    # podman --env-file tidak membuang tanda kutip, jadi "abc" akan terbaca lengkap dengan kutipnya.
    key="$(grep -E "^[A-Za-z_][A-Za-z0-9_]*=[\"']" "$envfile" | cut -d= -f1 | tr '\n' ' ' || true)"
    [[ -z "$key" ]] || die "Hapus tanda kutip di $envfile pada: ${key}(Podman tidak membuangnya)."
    for key in DATABASE_URL GOOGLE_CLIENT_ID GOOGLE_CLIENT_SECRET JWT_SECRET; do
        [[ -n "$(env_get "$envfile" "$key")" ]] || die "$key di $envfile masih kosong."
    done
    local jwt
    jwt="$(env_get "$envfile" JWT_SECRET)"
    (( ${#jwt} >= 32 )) || die "JWT_SECRET minimal 32 karakter."
    parse_database_url "$(env_get "$envfile" DATABASE_URL)"

    # Container Postgres: dari argumen, atau satu-satunya container berjalan yang image-nya postgres.
    if [[ -z "$pg_container" ]]; then
        candidates="$(podman ps --format '{{.Names}} {{.Image}}' \
            | awk '$2 ~ /(^|\/)(postgres|postgis|postgresql)[^\/]*(:|$)/ {print $1}')"
        case "$(printf '%s' "$candidates" | grep -c . || true)" in
            1) pg_container="$candidates" ;;
            0) die "Tidak ada container Postgres yang berjalan di podman user $(id -un). Sebutkan dengan --pg-container." ;;
            *) die "Ada beberapa container Postgres: $(echo "$candidates" | tr '\n' ' ')- pilih dengan --pg-container." ;;
        esac
    fi
    [[ "$(podman inspect -f '{{.State.Running}}' "$pg_container" 2>/dev/null)" == true ]] \
        || die "Container Postgres '$pg_container' tidak ada atau tidak berjalan."
    out PG_CONTAINER "$pg_container"

    # Container rootless yang memakai pasta/slirp4netns (bawaan rootless) atau network host tidak
    # bisa disambungkan ke network lain tanpa dibuat ulang.
    mode="$(podman inspect -f '{{.HostConfig.NetworkMode}}' "$pg_container")"
    case "$mode" in
        pasta|slirp4netns|host|none|container:*|ns:*|pod*)
            die "Container '$pg_container' memakai mode jaringan '$mode', sehingga tidak bisa berbagi network dengan backend. Buat ulang container Postgres dengan --network <nama-network> (volume datanya tetap), lalu deploy ulang dengan --pg-network <nama-network>." ;;
    esac

    # Network: dari argumen, atau network buatan user pertama yang dipakai container Postgres.
    # Network bawaan "podman" tidak punya DNS nama container, jadi tidak bisa dipakai.
    if [[ -n "$pg_network" ]]; then
        podman network exists "$pg_network" || die "Network '$pg_network' tidak ada."
        networks_of "$pg_container" | grep -qx "$pg_network" || out NEED_NETWORK 1
    else
        pg_network="$(networks_of "$pg_container" | grep -vxE 'podman|bridge|host|none' | head -n 1 || true)"
        if [[ -z "$pg_network" ]]; then
            pg_network=gomagang-db
            out NEED_NETWORK 1
        fi
    fi
    out PG_NETWORK "$pg_network"

    # Host di DATABASE_URL harus bisa ditemukan backend di network itu.
    hosts="$(names_on_network "$pg_container" "$pg_network" | sort -u)"
    if ! grep -qxF "$DB_HOST" <<<"$hosts"; then
        die "Host '$DB_HOST' di DATABASE_URL bukan nama container Postgres. Ganti host-nya menjadi '$pg_container' (port $DB_PORT)."
    fi

    # Status role dan database, dibaca sebagai superuser lewat socket lokal container.
    local status
    status="$(psql_su "$pg_container" -tA <<SQL 2>&1
\\set r $(psql_quote "$DB_USER")
\\set d $(psql_quote "$DB_NAME")
SELECT (SELECT count(*) FROM pg_roles WHERE rolname = :'r') || ' ' || (SELECT count(*) FROM pg_database WHERE datname = :'d');
SQL
)" || die "Tidak bisa masuk ke Postgres sebagai superuser '$(pg_superuser "$pg_container")': $status"
    out DB_ROLE_EXISTS "${status%% *}"
    out DB_EXISTS "${status##* }"
    out DB_USER "$DB_USER"
    out DB_NAME "$DB_NAME"

    port="$(bind_port "$bind")"
    if ! podman ps --format '{{.Names}}' | grep -qx "$CONTAINER" \
        && command -v ss >/dev/null && [[ -n "$(ss -Hltn "sport = :$port" 2>/dev/null)" ]]; then
        die "Port $port di VPS sudah dipakai proses lain. Pilih port lain dengan --bind."
    fi

    out CURRENT_TAG "$(env_get "$dir/$STATE_NAME" GOMAGANG_TAG)"
    out DEPLOY_DIR "$dir"
}

cmd_ensure_network() {
    local pg_container="$1" net="$2"
    if ! podman network exists "$net"; then
        podman network create "$net" >/dev/null
        say "network $net dibuat"
    fi
    if ! networks_of "$pg_container" | grep -qx "$net"; then
        podman network connect "$net" "$pg_container"
        say "$pg_container disambungkan ke network $net (tanpa restart)"
    fi
}

cmd_ensure_db() {
    local dir pg_container="$2"
    dir="$(resolve_dir "$1")"
    parse_database_url "$(env_get "$dir/.env.production" DATABASE_URL)"
    [[ -n "$DB_PASS" ]] || die "DATABASE_URL tidak berisi password."

    # Lewat stdin, supaya password tidak muncul di daftar proses VPS.
    psql_su "$pg_container" <<SQL
\\set r $(psql_quote "$DB_USER")
\\set p $(psql_quote "$DB_PASS")
\\set d $(psql_quote "$DB_NAME")
SELECT format('CREATE ROLE %I LOGIN PASSWORD %L', :'r', :'p')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'r') \\gexec
SELECT format('CREATE DATABASE %I OWNER %I', :'d', :'r')
WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = :'d') \\gexec
SQL
    say "role '$DB_USER' dan database '$DB_NAME' siap"
}

cmd_enable_linger() {
    # Untuk user sendiri, polkit biasanya mengizinkan tanpa sudo.
    if loginctl enable-linger "$(id -un)" 2>/dev/null; then
        say "linger dinyalakan; backend tetap berjalan setelah logout dan reboot"
    else
        die "Tidak boleh menyalakan linger tanpa sudo. Jalankan sekali di VPS: sudo loginctl enable-linger $(id -un)"
    fi
}

cmd_has_image() {
    podman image exists "$IMAGE_REPO:$1"
}

# Menulis unit Quadlet untuk TAG dan memastikan generator menerimanya sebelum dipasang.
install_quadlet() {
    local dir="$1" tag="$2" net="$3" bind="$4" tmp generator
    tmp="$(mktemp -d)"
    sed -e "s|@IMAGE@|$IMAGE_REPO:$tag|" \
        -e "s|@ENV_FILE@|$dir/.env.production|" \
        -e "s|@NETWORK@|$net|" \
        -e "s|@BIND@|$bind|" \
        "$dir/$TEMPLATE_NAME" >"$tmp/gomagang-backend.container"
    generator="$(quadlet_generator)"
    if ! QUADLET_UNIT_DIRS="$tmp" "$generator" -user -dryrun >"$tmp/generated" 2>&1 \
        || ! grep -q "gomagang-backend.service" "$tmp/generated"; then
        cat "$tmp/generated" >&2
        rm -rf "$tmp"
        die "Quadlet menolak unit gomagang-backend.container (lihat pesan di atas)."
    fi
    mkdir -p "$QUADLET_DIR"
    mv "$tmp/gomagang-backend.container" "$QUADLET_FILE"
    rm -rf "$tmp"
    user_systemctl daemon-reload
    [[ "$(user_systemctl show -p LoadState --value "$SERVICE")" == loaded ]] \
        || die "systemd tidak membuat $SERVICE dari $QUADLET_FILE."
}

write_state() {
    local dir="$1" tag="$2" previous="$3" net="$4" bind="$5"
    cat >"$dir/$STATE_NAME.tmp" <<EOF
# Ditulis oleh scripts/deploy.sh. Jangan diubah manual; deploy berikutnya menimpanya.
GOMAGANG_TAG=$tag
GOMAGANG_PREVIOUS_TAG=$previous
PG_NETWORK=$net
GOMAGANG_BIND=$bind
EOF
    mv "$dir/$STATE_NAME.tmp" "$dir/$STATE_NAME"
}

# Menunggu sampai /health menjawab 200. Gagal lebih cepat bila service crash atau gagal start.
wait_healthy() {
    local host="$1" port="$2" waited=0 state
    while (( waited < HEALTH_TIMEOUT )); do
        if http_health "$host" "$port"; then
            return 0
        fi
        state="$(user_systemctl show -p ActiveState -p SubState --value "$SERVICE" | paste -sd ' ')"
        case "$state" in
            failed*|*auto-restart*|inactive*) return 1 ;;
        esac
        sleep 2
        waited=$(( waited + 2 ))
    done
    return 1
}

start_tag() {
    local dir="$1" tag="$2" previous="$3" net="$4" bind="$5"
    install_quadlet "$dir" "$tag" "$net" "$bind"
    write_state "$dir" "$tag" "$previous" "$net" "$bind"
    say "menjalankan $IMAGE_REPO:$tag lewat $SERVICE"
    # Gagal start ditangani wait_healthy, yang juga menampilkan log.
    user_systemctl restart "$SERVICE" || true
}

show_logs() {
    journalctl --user -u "$SERVICE" -n 60 --no-pager -o cat >&2 2>/dev/null \
        || podman logs --tail 60 "$CONTAINER" >&2 2>&1 || true
}

cmd_up() {
    local dir tag="$2" net="$3" bind="$4" previous host port
    dir="$(resolve_dir "$1")"
    cmd_has_image "$tag" || die "Image $IMAGE_REPO:$tag tidak ada di VPS."

    previous="$(env_get "$dir/$STATE_NAME" GOMAGANG_TAG)"
    [[ "$previous" == "$tag" ]] && previous="$(env_get "$dir/$STATE_NAME" GOMAGANG_PREVIOUS_TAG)"
    host="$(bind_host "$bind")"
    port="$(bind_port "$bind")"

    start_tag "$dir" "$tag" "$previous" "$net" "$bind"
    if wait_healthy "$host" "$port"; then
        say "sehat: GET http://$host:$port/health menjawab 200"
        podman exec "$CONTAINER" sh -c 'test -w /data/uploads' \
            || say "PERINGATAN: /data/uploads tidak bisa ditulis backend; unggah berkas akan gagal."
        out DEPLOYED_TAG "$tag"
        return 0
    fi

    echo "  [vps] backend berhenti atau tidak sehat (batas ${HEALTH_TIMEOUT} detik). Log terakhir:" >&2
    show_logs
    if [[ -n "$previous" ]] && cmd_has_image "$previous"; then
        say "rollback ke $IMAGE_REPO:$previous"
        start_tag "$dir" "$previous" "" "$net" "$bind"
        if wait_healthy "$host" "$port"; then
            say "rollback berhasil; versi lama $previous berjalan lagi"
        else
            say "rollback juga tidak sehat; periksa: journalctl --user -u $SERVICE"
        fi
    fi
    die "deploy $tag gagal"
}

cmd_prune() {
    local dir keep="$2" current previous count=0 tag
    dir="$(resolve_dir "$1")"
    current="$(env_get "$dir/$STATE_NAME" GOMAGANG_TAG)"
    previous="$(env_get "$dir/$STATE_NAME" GOMAGANG_PREVIOUS_TAG)"
    # Diurutkan dari yang paling baru menurut waktu dibuat.
    while read -r tag; do
        [[ -z "$tag" || "$tag" == "<none>" ]] && continue
        count=$(( count + 1 ))
        [[ "$tag" == "$current" || "$tag" == "$previous" ]] && continue
        (( count > keep )) || continue
        podman image rm "$IMAGE_REPO:$tag" >/dev/null 2>&1 && say "image lama $IMAGE_REPO:$tag dihapus" || true
    done < <(podman image ls "$IMAGE_REPO" --format '{{.CreatedAt}}	{{.Tag}}' | sort -r | cut -f2)
}

command="${1:-}"
shift || true
case "$command" in
    preflight) cmd_preflight "$@" ;;
    ensure-network) cmd_ensure_network "$@" ;;
    ensure-db) cmd_ensure_db "$@" ;;
    enable-linger) cmd_enable_linger ;;
    has-image) cmd_has_image "$@" ;;
    up) cmd_up "$@" ;;
    prune) cmd_prune "$@" ;;
    *) die "perintah tidak dikenal: '$command'" ;;
esac
