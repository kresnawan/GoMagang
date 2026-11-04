use sqlx::{PgPool, Postgres, QueryBuilder};

use super::model::{Mahasiswa, MahasiswaPatch};

pub async fn find_by_id_pengguna(
    db: &PgPool,
    id_pengguna: i32,
) -> Result<Option<Mahasiswa>, sqlx::Error> {
    // The casts drop the CHAR padding and turn NUMERIC ipk into a JSON number.
    sqlx::query_as(
        "SELECT m.nim::text AS nim, p.email, m.nama, m.program_studi,
                m.ipk::float8 AS ipk, m.no_hp,
                m.jenis_kelamin::text AS jenis_kelamin, m.semester
         FROM mahasiswa m
         JOIN pengguna p ON p.id_pengguna = m.id_pengguna
         WHERE m.id_pengguna = $1",
    )
    .bind(id_pengguna)
    .fetch_optional(db)
    .await
}

/// Changes only the columns present in the patch. The column names are fixed here and every value
/// is sent as a bound parameter, so the body can't alter the SQL.
pub async fn update(
    db: &PgPool,
    id_pengguna: i32,
    patch: MahasiswaPatch,
) -> Result<(), sqlx::Error> {
    let mut query = QueryBuilder::<Postgres>::new("UPDATE mahasiswa SET ");
    let mut columns = query.separated(", ");
    let mut changed = false;

    if let Some(nim) = patch.nim {
        columns.push("nim = ").push_bind_unseparated(nim);
        changed = true;
    }
    if let Some(nama) = patch.nama {
        columns.push("nama = ").push_bind_unseparated(nama);
        changed = true;
    }
    if let Some(program_studi) = patch.program_studi {
        columns
            .push("program_studi = ")
            .push_bind_unseparated(program_studi);
        changed = true;
    }
    if let Some(ipk) = patch.ipk {
        columns.push("ipk = ").push_bind_unseparated(ipk);
        changed = true;
    }
    if let Some(no_hp) = patch.no_hp {
        columns.push("no_hp = ").push_bind_unseparated(no_hp);
        changed = true;
    }
    if let Some(jenis_kelamin) = patch.jenis_kelamin {
        columns
            .push("jenis_kelamin = ")
            .push_bind_unseparated(jenis_kelamin);
        changed = true;
    }
    if let Some(semester) = patch.semester {
        columns.push("semester = ").push_bind_unseparated(semester);
        changed = true;
    }

    if !changed {
        return Ok(());
    }

    query.push(" WHERE id_pengguna = ").push_bind(id_pengguna);
    query.build().execute(db).await?;

    Ok(())
}
