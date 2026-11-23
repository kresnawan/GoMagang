#!/usr/bin/env bash
#
# Bagian deploy yang berjalan di VPS. Tidak dijalankan langsung, tetapi dikirim oleh
# scripts/deploy.sh lewat SSH:  ssh host bash -s -- <perintah> [argumen...] < deploy-remote.sh
#
# Semua yang membaca secret (.env.production, password database) terjadi di sini, sehingga secret
# tidak pernah keluar dari VPS. Hasil yang perlu dibaca laptop ditulis ke stdout sebagai KUNCI=nilai;
# pesan untuk manusia ditulis ke stderr.
#
# Perintah:
#   preflight   DIR PG_CONTAINER PG_NETWORK BIND   Memeriksa VPS dan mencari container/network Postgres
#   ensure-network PG_CONTAINER NETWORK            Membuat network dan menyambungkan Postgres ke sana
#   ensure-db   DIR PG_CONTAINER                   Membuat role dan database dari DATABASE_URL
#   has-image   TAG                                Keluar 0 bila image gomagang:TAG sudah ada
#   up          DIR TAG PG_NETWORK BIND            Menjalankan TAG, menunggu sehat, rollback bila gagal
#   prune       DIR KEEP                           Menghapus image gomagang lama, menyisakan KEEP terbaru

set -Eeuo pipefail

IMAGE_NAME=gomagang
CONTAINER=gomagang-backend
HEALTH_TIMEOUT=90

say() { echo "  [vps] $*" >&2; }
die() { echo "  [vps] GAGAL: $*" >&2; exit 1; }
out() { printf '%s=%s\n' "$1" "$2"; }

# Folder deploy: path relatif dihitung dari home user SSH.
resolve_dir() {
    case "$1" in
        /*) printf '%s' "$1" ;;
        *) printf '%s/%s' "$HOME" "$1" ;;
    esac
}

# Membaca satu variabel dari file env tanpa men-source-nya (file berisi secret, bukan script).
# Tanda kutip pembungkus dibuang, seperti yang dilakukan docker compose.
env_get() {
    local file="$1" key="$2" line value
    [[ -f "$file" ]] || return 0
    line="$(grep -E "^[[:space:]]*${key}=" "$file" | tail -n 1 || true)"
    [[ -n "$line" ]] || return 0
    value="${line#*=}"
    value="${value%$'\r'}"
    if [[ "$value" =~ ^\"(.*)\"$ || "$value" =~ ^\'(.*)\'$ ]]; then
        value="${BASH_REMATCH[1]}"
    fi
    printf '%s' "$value"
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
        su="$(docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' "$pg" \
            | sed -n 's/^POSTGRES_USER=//p' | head -n 1)"
    fi
    printf '%s' "${su:-postgres}"
}

# Menjalankan SQL dari stdin sebagai superuser lewat socket lokal di container Postgres.
psql_su() {
    local pg="$1"; shift
    docker exec -i "$pg" psql -X -q -v ON_ERROR_STOP=1 -U "$(pg_superuser "$pg")" -d postgres "$@"
}

networks_of() {
    docker inspect -f '{{range $k, $v := .NetworkSettings.Networks}}{{println $k}}{{end}}' "$1" \
        | sed '/^$/d'
}

# Nama yang bisa dipakai untuk menghubungi container di network NET: nama container dan alias-nya.
names_on_network() {
    local container="$1" net="$2"
    docker inspect -f '{{.Name}}' "$container" | sed 's#^/##'
    docker inspect -f "{{with index .NetworkSettings.Networks \"$net\"}}{{range .Aliases}}{{println .}}{{end}}{{range .DNSNames}}{{println .}}{{end}}{{end}}" \
        "$container" 2>/dev/null | sed '/^$/d'
}

bind_port() {
    local bind="$1"
    printf '%s' "${bind##*:}"
}

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

cmd_preflight() {
    local dir pg_container="$2" pg_network="$3" bind="$4" envfile key candidates hosts port
    dir="$(resolve_dir "$1")"

    docker info >/dev/null 2>&1 \
        || die "Docker tidak bisa dipakai oleh user $(id -un). Tambahkan ke grup docker: sudo usermod -aG docker $(id -un), lalu login ulang."
    docker compose version >/dev/null 2>&1 || die "Plugin docker compose tidak terpasang."
    out ARCH "$(docker info -f '{{.Architecture}}')"

    mkdir -p "$dir"
    envfile="$dir/.env.production"
    if [[ ! -f "$envfile" ]]; then
        die "$envfile belum ada. Salin dari $dir/.env.production.example, isi nilainya, lalu deploy ulang."
    fi
    chmod 600 "$envfile"
    for key in DATABASE_URL GOOGLE_CLIENT_ID GOOGLE_CLIENT_SECRET JWT_SECRET; do
        [[ -n "$(env_get "$envfile" "$key")" ]] || die "$key di $envfile masih kosong."
    done
    local jwt
    jwt="$(env_get "$envfile" JWT_SECRET)"
    (( ${#jwt} >= 32 )) || die "JWT_SECRET minimal 32 karakter."
    parse_database_url "$(env_get "$envfile" DATABASE_URL)"

    # Container Postgres: dari argumen, atau satu-satunya container berjalan yang image-nya postgres.
    if [[ -z "$pg_container" ]]; then
        candidates="$(docker ps --format '{{.Names}} {{.Image}}' \
            | awk '$2 ~ /(^|\/)(postgres|postgis)[^\/]*(:|$)/ {print $1}')"
        case "$(printf '%s' "$candidates" | grep -c . || true)" in
            1) pg_container="$candidates" ;;
            0) die "Tidak ada container Postgres yang berjalan. Sebutkan dengan --pg-container." ;;
            *) die "Ada beberapa container Postgres: $(echo "$candidates" | tr '\n' ' ')- pilih dengan --pg-container." ;;
        esac
    fi
    [[ "$(docker inspect -f '{{.State.Running}}' "$pg_container" 2>/dev/null)" == true ]] \
        || die "Container Postgres '$pg_container' tidak ada atau tidak berjalan."
    out PG_CONTAINER "$pg_container"

    # Network: dari argumen, atau network buatan user pertama yang dipakai container Postgres.
    # Network bawaan "bridge" tidak punya DNS nama container, jadi tidak bisa dipakai.
    if [[ -n "$pg_network" ]]; then
        docker network inspect "$pg_network" >/dev/null 2>&1 || die "Network '$pg_network' tidak ada."
        if ! networks_of "$pg_container" | grep -qx "$pg_network"; then
            out NEED_NETWORK 1
        fi
    else
        pg_network="$(networks_of "$pg_container" | grep -vxE 'bridge|host|none' | head -n 1 || true)"
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
    if ! docker ps --format '{{.Names}}' | grep -qx "$CONTAINER" \
        && command -v ss >/dev/null && [[ -n "$(ss -Hltn "sport = :$port" 2>/dev/null)" ]]; then
        die "Port $port di VPS sudah dipakai proses lain. Pilih port lain dengan --bind."
    fi

    out CURRENT_TAG "$(env_get "$dir/.env" GOMAGANG_TAG)"
    out DEPLOY_DIR "$dir"
}

cmd_ensure_network() {
    local pg_container="$1" net="$2"
    if ! docker network inspect "$net" >/dev/null 2>&1; then
        docker network create "$net" >/dev/null
        say "network $net dibuat"
    fi
    if ! networks_of "$pg_container" | grep -qx "$net"; then
        docker network connect "$net" "$pg_container"
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

cmd_has_image() {
    docker image inspect "$IMAGE_NAME:$1" >/dev/null 2>&1
}

write_compose_env() {
    local dir="$1" tag="$2" previous="$3" net="$4" bind="$5"
    cat >"$dir/.env.tmp" <<EOF
# Ditulis oleh scripts/deploy.sh. Jangan diubah manual; deploy berikutnya menimpanya.
GOMAGANG_TAG=$tag
GOMAGANG_PREVIOUS_TAG=$previous
PG_NETWORK=$net
GOMAGANG_BIND=$bind
EOF
    mv "$dir/.env.tmp" "$dir/.env"
}

# Menunggu sampai /health menjawab 200. Gagal lebih cepat bila container berhenti.
wait_healthy() {
    local host="$1" port="$2" waited=0 state
    while (( waited < HEALTH_TIMEOUT )); do
        if http_health "$host" "$port"; then
            return 0
        fi
        state="$(docker inspect -f '{{.State.Status}} {{.RestartCount}}' "$CONTAINER" 2>/dev/null || echo missing)"
        if [[ "$state" == exited* || "$state" == missing* ]] || (( ${state##* } > 0 )); then
            return 1
        fi
        sleep 2
        waited=$(( waited + 2 ))
    done
    return 1
}

cmd_up() {
    local dir tag="$2" net="$3" bind="$4" previous host port
    dir="$(resolve_dir "$1")"
    cd "$dir"
    cmd_has_image "$tag" || die "Image $IMAGE_NAME:$tag tidak ada di VPS."

    previous="$(env_get .env GOMAGANG_TAG)"
    [[ "$previous" == "$tag" ]] && previous="$(env_get .env GOMAGANG_PREVIOUS_TAG)"
    host="$(bind_host "$bind")"
    port="$(bind_port "$bind")"

    write_compose_env "$dir" "$tag" "$previous" "$net" "$bind"
    say "menjalankan $IMAGE_NAME:$tag"
    docker compose up -d --remove-orphans >&2

    if wait_healthy "$host" "$port"; then
        say "sehat: GET http://$host:$port/health menjawab 200"
        out DEPLOYED_TAG "$tag"
        return 0
    fi

    echo "  [vps] backend berhenti atau tidak sehat (batas ${HEALTH_TIMEOUT} detik). Log terakhir:" >&2
    docker logs --tail 60 "$CONTAINER" >&2 2>&1 || true
    if [[ -n "$previous" ]] && cmd_has_image "$previous"; then
        say "rollback ke $IMAGE_NAME:$previous"
        write_compose_env "$dir" "$previous" "" "$net" "$bind"
        docker compose up -d --remove-orphans >&2
        if wait_healthy "$host" "$port"; then
            say "rollback berhasil; versi lama $previous berjalan lagi"
        else
            say "rollback juga tidak sehat; periksa: docker logs $CONTAINER"
        fi
    fi
    die "deploy $tag gagal"
}

cmd_prune() {
    local dir keep="$2" current previous count=0 tag
    dir="$(resolve_dir "$1")"
    current="$(env_get "$dir/.env" GOMAGANG_TAG)"
    previous="$(env_get "$dir/.env" GOMAGANG_PREVIOUS_TAG)"
    # docker image ls mengurutkan dari yang paling baru.
    while read -r tag; do
        [[ -z "$tag" || "$tag" == "<none>" ]] && continue
        count=$(( count + 1 ))
        [[ "$tag" == "$current" || "$tag" == "$previous" ]] && continue
        (( count > keep )) || continue
        docker image rm "$IMAGE_NAME:$tag" >/dev/null 2>&1 && say "image lama $IMAGE_NAME:$tag dihapus" || true
    done < <(docker image ls "$IMAGE_NAME" --format '{{.Tag}}')
}

command="${1:-}"
shift || true
case "$command" in
    preflight) cmd_preflight "$@" ;;
    ensure-network) cmd_ensure_network "$@" ;;
    ensure-db) cmd_ensure_db "$@" ;;
    has-image) cmd_has_image "$@" ;;
    up) cmd_up "$@" ;;
    prune) cmd_prune "$@" ;;
    *) die "perintah tidak dikenal: '$command'" ;;
esac
