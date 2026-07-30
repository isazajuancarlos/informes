//! Lo que la base pone para el freno del login: dos consultas y nada de criterio.
//!
//! La REGLA —el umbral, el techo, qué borra la cuenta de fallos, qué se hace con
//! una marca de tiempo ilegible— vive en `guaca::freno`, compartida con medico.
//! Aquí solo se anota y se lee. Es la separación a propósito: lo que diverge
//! cuando se copia entre dos aplicaciones no es el `INSERT`.

use chrono::{DateTime, Utc};
use guaca::freno;

/// Anota un intento. Se llama SIEMPRE, con éxito o sin él.
pub async fn anotar(
    bd: &sqlx::SqlitePool,
    email: &str,
    origen: &str,
    exito: bool,
    ahora: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO intento_acceso (email, origen, momento, exito) VALUES (?, ?, ?, ?)")
        .bind(email)
        .bind(origen)
        .bind(ahora.to_rfc3339())
        .bind(i64::from(exito))
        .execute(bd)
        .await?;

    // La poda se acota a ESTA pareja y a lo más viejo que la retención:
    // exactamente el conjunto que esta llamada vuelve a escribir, y ni una fila
    // más. Una limpieza global aquí borraría el rastro de otras cuentas que nadie
    // pidió borrar — y eso no se ve en la primera vuelta, solo en la segunda.
    // El borde sale de guaca para que la poda y el conteo no puedan discrepar.
    sqlx::query("DELETE FROM intento_acceso WHERE email = ? AND origen = ? AND momento < ?")
        .bind(email)
        .bind(origen)
        .bind(freno::corte_retencion(ahora).to_rfc3339())
        .execute(bd)
        .await?;
    Ok(())
}

/// El estado del freno para esta pareja ahora mismo.
pub async fn estado(
    bd: &sqlx::SqlitePool,
    email: &str,
    origen: &str,
    ahora: DateTime<Utc>,
) -> Result<freno::Estado, sqlx::Error> {
    // Sin `ORDER BY` ni filtro de fecha: `evaluar` no depende del orden y aplica
    // la retención por su cuenta. Filtrar aquí también sería un segundo borde que
    // mantener, y dos bordes de la misma regla acaban discrepando.
    let filas: Vec<(String, i64)> =
        sqlx::query_as("SELECT momento, exito FROM intento_acceso WHERE email = ? AND origen = ?")
            .bind(email)
            .bind(origen)
            .fetch_all(bd)
            .await?;

    let intentos: Vec<freno::Intento<'_>> = filas
        .iter()
        .map(|(momento, exito)| freno::Intento { momento: momento.as_str(), exito: *exito != 0 })
        .collect();
    Ok(freno::evaluar(&intentos, ahora))
}
