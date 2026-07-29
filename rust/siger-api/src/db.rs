//! Conexión y esquema.
//!
//! Las migraciones van **dentro del binario** (`sqlx::migrate!`) y se aplican al
//! arrancar. El sistema anterior necesitaba `python -m app.init_db` y después
//! `python migrar_esquema.py`, en ese orden y de memoria; el día que se
//! invirtieron, el instalador terminó con código 0 anunciando credenciales que
//! no existían. Aquí no hay orden que recordar porque no hay dos pasos.

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

#[derive(Debug, thiserror::Error)]
pub enum ErrorBd {
    #[error("no se pudo abrir la base «{0}»: {1}")]
    NoAbre(String, sqlx::Error),
    #[error("la migración del esquema falló: {0}")]
    Migracion(#[from] sqlx::migrate::MigrateError),
}

/// Abre la base y deja el esquema al día.
///
/// SQLite es el motor por defecto de los tres perfiles: un cliente por
/// instalación, un solo escritor y una base que hoy pesa 1,8 MB. PostgreSQL
/// deja de ser una dependencia que administrar para pasar a ser una opción, y
/// entra por el mismo `sqlx` cuando un cliente lo exija.
pub async fn abrir(url: &str) -> Result<SqlitePool, ErrorBd> {
    let opciones = SqliteConnectOptions::from_str(url)
        .map_err(|e| ErrorBd::NoAbre(url.to_string(), e))?
        // Que la base no exista es normal en la primera ejecución; que no se
        // pueda crear, no.
        .create_if_missing(true)
        // WAL: lecturas concurrentes con la escritura. `foreign_keys` no está
        // activo por defecto en SQLite, y sin él las claves foráneas del esquema
        // son documentación, no una restricción.
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opciones)
        .await
        .map_err(|e| ErrorBd::NoAbre(url.to_string(), e))?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}
