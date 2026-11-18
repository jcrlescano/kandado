use kandado_protocol::{AccountKeys, ChangeMaster, KdfSettings, Opaque};
use sqlx::{PgPool, Row, postgres::PgRow};
use uuid::Uuid;

fn from_row(row: &PgRow) -> sqlx::Result<AccountKeys> {
    let count = |name: &str| -> sqlx::Result<u32> {
        let value: i32 = row.try_get(name)?;
        u32::try_from(value).map_err(|_| sqlx::Error::Decode("negative value".into()))
    };
    let version: i16 = row.try_get("version")?;
    Ok(AccountKeys {
        version: u8::try_from(version).map_err(|_| sqlx::Error::Decode("bad version".into()))?,
        kdf_salt: Opaque::new(row.try_get("kdf_salt")?),
        kdf: KdfSettings {
            memory_kib: count("kdf_memory_kib")?,
            iterations: count("kdf_iterations")?,
            parallelism: count("kdf_parallelism")?,
        },
        user_key_by_master: Opaque::new(row.try_get("user_key_by_master")?),
        user_key_by_recovery: Opaque::new(row.try_get("user_key_by_recovery")?),
        public_x25519: Opaque::new(row.try_get("public_x25519")?),
        private_keys_enc: Opaque::new(row.try_get("private_keys_enc")?),
    })
}

pub async fn get(db: &PgPool, user_id: Uuid) -> sqlx::Result<Option<AccountKeys>> {
    let row = sqlx::query("SELECT * FROM user_keys WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    row.as_ref().map(from_row).transpose()
}

pub async fn exists(db: &PgPool, user_id: Uuid) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM user_keys WHERE user_id = $1)")
        .bind(user_id)
        .fetch_one(db)
        .await
}

/// Stores the keys once. Returns false when the user already has some, which
/// is never overwritten here.
pub async fn insert(db: &PgPool, user_id: Uuid, keys: &AccountKeys) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "INSERT INTO user_keys (
            user_id, version, kdf_salt, kdf_memory_kib, kdf_iterations, kdf_parallelism,
            user_key_by_master, user_key_by_recovery, public_x25519, private_keys_enc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(i16::from(keys.version))
    .bind(keys.kdf_salt.as_bytes())
    .bind(keys.kdf.memory_kib as i32)
    .bind(keys.kdf.iterations as i32)
    .bind(keys.kdf.parallelism as i32)
    .bind(keys.user_key_by_master.as_bytes())
    .bind(keys.user_key_by_recovery.as_bytes())
    .bind(keys.public_x25519.as_bytes())
    .bind(keys.private_keys_enc.as_bytes())
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn change_master(
    db: &PgPool,
    user_id: Uuid,
    change: &ChangeMaster,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE user_keys SET
            kdf_salt = $2, kdf_memory_kib = $3, kdf_iterations = $4, kdf_parallelism = $5,
            user_key_by_master = $6, updated_at = now()
         WHERE user_id = $1",
    )
    .bind(user_id)
    .bind(change.kdf_salt.as_bytes())
    .bind(change.kdf.memory_kib as i32)
    .bind(change.kdf.iterations as i32)
    .bind(change.kdf.parallelism as i32)
    .bind(change.user_key_by_master.as_bytes())
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}
