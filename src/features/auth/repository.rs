use std::time::Duration;

use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use super::model::{GoogleClaims, Peran, User};

const CLEANUP_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[derive(FromRow)]
struct CurrentToken {
    id_token: i64,
    id_pengguna: i32,
    id_keluarga: Uuid,
    active: bool,
    reused: bool,
}

pub struct RotatedToken {
    pub user: User,
    pub expires_in: Duration,
}

/// Menyimpan akun mahasiswa yang login dan memastikan baris `mahasiswa`-nya ada.
///
/// Pengguna dicari berdasarkan `google_sub` (ID akun Google yang tidak pernah berubah). Bila sudah
/// ada, hanya email yang diperbarui. Baris `mahasiswa` dibuat kosong bila belum ada, tanpa
/// menyentuh data diri yang sudah diisi. Keduanya dilakukan dalam satu transaksi.
pub async fn upsert_mahasiswa(db: &PgPool, claims: &GoogleClaims) -> Result<User, sqlx::Error> {
    let mut tx = db.begin().await?;

    let user: User = sqlx::query_as(
        "INSERT INTO pengguna (google_sub, email)
         VALUES ($1, $2)
         ON CONFLICT (google_sub) DO UPDATE
         SET email = EXCLUDED.email
         RETURNING id_pengguna, email",
    )
    .bind(&claims.sub)
    .bind(&claims.email)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("INSERT INTO mahasiswa (id_pengguna) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(user.id_pengguna)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(user)
}

/// Mencari anggota Panitia Magang dengan email ini dan mengikatnya ke akun Google pada login
/// pertama.
///
/// Panitia Magang didaftarkan lebih dulu hanya dengan email, sehingga `google_sub`-nya masih
/// kosong.
/// Pada login pertama, `google_sub` diisi dengan ID akun Google. Login berikutnya hanya diterima
/// dari akun Google yang sama. Email dibandingkan tanpa membedakan huruf besar-kecil.
///
/// Mengembalikan `None` bila email tidak terdaftar di `panitia_magang`, atau baris itu sudah
/// diklaim oleh akun Google lain.
pub async fn find_panitia_magang(
    db: &PgPool,
    claims: &GoogleClaims,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as(
        "UPDATE pengguna p
         SET google_sub = $1
         FROM panitia_magang t
         WHERE t.id_pengguna = p.id_pengguna
           AND lower(p.email) = lower($2)
           AND (p.google_sub IS NULL OR p.google_sub = $1)
         RETURNING p.id_pengguna, p.email",
    )
    .bind(&claims.sub)
    .bind(&claims.email)
    .fetch_optional(db)
    .await
}

/// Menentukan peran pengguna dari tabel tempat barisnya berada.
///
/// Mengembalikan [`Peran::PanitiaMagang`] bila ada di tabel `panitia_magang` (peran ini menang bila
/// pengguna kebetulan ada di kedua tabel), [`Peran::Mahasiswa`] bila hanya ada di tabel
/// `mahasiswa`, dan `None` bila tidak punya peran sama sekali.
pub async fn find_peran(db: &PgPool, id_pengguna: i32) -> Result<Option<Peran>, sqlx::Error> {
    let (panitia_magang, mahasiswa): (bool, bool) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM panitia_magang WHERE id_pengguna = $1),
                EXISTS (SELECT 1 FROM mahasiswa WHERE id_pengguna = $1)",
    )
    .bind(id_pengguna)
    .fetch_one(db)
    .await?;

    Ok(match (panitia_magang, mahasiswa) {
        (true, _) => Some(Peran::PanitiaMagang),
        (false, true) => Some(Peran::Mahasiswa),
        (false, false) => None,
    })
}

/// Menyimpan refresh token baru sebagai awal keluarga token baru.
///
/// Setiap login membuat satu keluarga (`id_keluarga` acak). Token disimpan dalam bentuk hash,
/// dengan masa berlaku `ttl` yang dihitung dari jam database.
pub async fn create_refresh_token(
    db: &PgPool,
    id_pengguna: i32,
    token_hash: &[u8],
    ttl: Duration,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO token_refresh (id_pengguna, id_keluarga, hash_token, expires_at)
         VALUES ($1, gen_random_uuid(), $2, now() + make_interval(secs => $3))",
    )
    .bind(id_pengguna)
    .bind(token_hash)
    .bind(ttl.as_secs_f64())
    .execute(db)
    .await?;

    Ok(())
}

/// Menukar refresh token dengan token baru di keluarga yang sama.
///
/// Token baru mewarisi `expires_at` keluarganya, sehingga rotasi tidak memperpanjang sesi. Token
/// lama ditandai `rotated_at`, dan baris token dikunci agar request bersamaan dengan token yang
/// sama diproses satu per satu.
///
/// Token yang sudah dirotasi masih diterima dalam selang `grace`. Ini supaya beberapa tab yang
/// me-refresh bersamaan tidak saling membuat logout. Lewat dari itu, pemakaian ulang dianggap
/// tanda token bocor, sehingga seluruh keluarga dihapus.
///
/// Mengembalikan `None` bila token tidak dikenal, kedaluwarsa, atau terdeteksi dipakai ulang.
pub async fn rotate_refresh_token(
    db: &PgPool,
    token_hash: &[u8],
    new_token_hash: &[u8],
    grace: Duration,
) -> Result<Option<RotatedToken>, sqlx::Error> {
    let mut tx = db.begin().await?;

    // The row lock makes concurrent requests with the same token run one at a time.
    let current: Option<CurrentToken> = sqlx::query_as(
        "SELECT id_token, id_pengguna, id_keluarga,
                expires_at > now() AS active,
                COALESCE(rotated_at < now() - make_interval(secs => $2), false) AS reused
         FROM token_refresh
         WHERE hash_token = $1
         FOR UPDATE",
    )
    .bind(token_hash)
    .bind(grace.as_secs_f64())
    .fetch_optional(&mut *tx)
    .await?;

    let Some(current) = current else {
        return Ok(None);
    };
    if !current.active {
        return Ok(None);
    }
    if current.reused {
        sqlx::query("DELETE FROM token_refresh WHERE id_keluarga = $1")
            .bind(current.id_keluarga)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(None);
    }

    // COALESCE keeps the first rotation time, so repeated use can't extend the grace period.
    sqlx::query(
        "UPDATE token_refresh SET rotated_at = COALESCE(rotated_at, now()) WHERE id_token = $1",
    )
    .bind(current.id_token)
    .execute(&mut *tx)
    .await?;

    let expires_in: i64 = sqlx::query_scalar(
        "INSERT INTO token_refresh (id_pengguna, id_keluarga, hash_token, expires_at)
         SELECT id_pengguna, id_keluarga, $2, expires_at FROM token_refresh WHERE id_token = $1
         RETURNING EXTRACT(EPOCH FROM expires_at - now())::bigint",
    )
    .bind(current.id_token)
    .bind(new_token_hash)
    .fetch_one(&mut *tx)
    .await?;

    let user = sqlx::query_as("SELECT id_pengguna, email FROM pengguna WHERE id_pengguna = $1")
        .bind(current.id_pengguna)
        .fetch_one(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(Some(RotatedToken {
        user,
        expires_in: Duration::from_secs(expires_in.max(0) as u64),
    }))
}

/// Menghapus seluruh keluarga refresh token yang memuat token dengan hash ini.
///
/// Dipakai saat logout, sehingga semua token hasil rotasi dari login yang sama ikut tidak berlaku.
/// Token yang tidak dikenal diabaikan tanpa error.
pub async fn revoke_family(db: &PgPool, token_hash: &[u8]) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM token_refresh
         WHERE id_keluarga = (SELECT id_keluarga FROM token_refresh WHERE hash_token = $1)",
    )
    .bind(token_hash)
    .execute(db)
    .await?;

    Ok(())
}

/// Menghapus refresh token yang sudah kedaluwarsa, dan mengembalikan jumlah baris yang terhapus.
pub async fn delete_expired_tokens(db: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM token_refresh WHERE expires_at <= now()")
        .execute(db)
        .await?;

    Ok(result.rows_affected())
}

/// Menjalankan pembersihan refresh token kedaluwarsa di latar belakang setiap
/// `CLEANUP_INTERVAL` (1 jam).
///
/// Putaran pertama langsung berjalan saat server start. Kegagalan hanya dicatat ke log, dan
/// pembersihan dicoba lagi pada putaran berikutnya.
pub fn spawn_token_cleanup(db: PgPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
        loop {
            interval.tick().await;
            if let Err(err) = delete_expired_tokens(&db).await {
                eprintln!("failed to delete expired refresh tokens: {err}");
            }
        }
    });
}
