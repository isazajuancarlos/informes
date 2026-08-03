//! Lo que la base pone para la puerta: el freno del login y la revocación.
//!
//! La REGLA del freno —el umbral, el techo, qué borra la cuenta de fallos, qué se
//! hace con una marca de tiempo ilegible— vive en `guaca::freno`, compartida con
//! medico. Aquí solo se anota y se lee. Es la separación a propósito: lo que
//! diverge cuando se copia entre dos aplicaciones no es el `INSERT`.
//!
//! Las dos piezas están juntas por la misma razón que comparten la migración
//! `0006_freno_y_revocacion`: son la misma puerta. El freno impide que alguien
//! adivine la contraseña; la revocación cierra lo que se abrió con una que ya no
//! vale.

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

/// Sube el `token_version` del usuario: **todas** sus sesiones abiertas dejan de
/// valer en la siguiente petición (#194).
///
/// Es la única escritura del entero que existe fuera de una prueba, y hasta que
/// la hubo la revocación era una pieza sin gatillo: el mecanismo estaba
/// documentado y probado, y ningún operador podía dispararlo sin abrir la base
/// con `sqlite3` a mano.
///
/// # Por qué devuelve `bool` y no `()`
///
/// `false` significa que la sentencia no tocó ninguna fila —el usuario ya no
/// está—, y quien llama TIENE que distinguirlo: responder «listo, cerré todas
/// tus sesiones» cuando no se cerró ninguna es exactamente el defecto silencioso
/// que la directiva 20 prohíbe, y encima en el camino donde el usuario acaba de
/// pedir seguridad. Un `Result<(), _>` no dejaría verlo, porque el `UPDATE` sin
/// filas afectadas **no es un error de SQL**.
pub async fn revocar_sesiones(bd: &sqlx::SqlitePool, usuario_id: i64) -> Result<bool, sqlx::Error> {
    // `token_version + 1` en la base y no un número calculado aquí: dos procesos
    // revocando a la vez leerían el mismo valor y escribirían el mismo, con lo
    // que una de las dos revocaciones no revocaría nada. El incremento lo hace
    // SQLite dentro de la sentencia.
    let r = sqlx::query("UPDATE usuarios SET token_version = token_version + 1 WHERE id = ?")
        .bind(usuario_id)
        .execute(bd)
        .await?;
    Ok(r.rows_affected() > 0)
}
