//! Participación: quién interviene en un informe y con qué papel.
//!
//! Es el prerequisito que faltaba (#162): sin partes, el planificador genera
//! entregas **sin responsable** (busca `rol IN ('EJECUTA','ORIGEN')`) y las
//! alertas no tienen a quién avisar (operador por `usuario_id`, tercero por
//! `rol = 'DESTINO'`). Aquí se leen y se asignan.
//!
//! El `PUT` **reemplaza** el conjunto completo de partes del informe: el editor
//! manda la lista entera, no parches, para que no queden restos de una edición
//! anterior sin que nadie lo note.

use axum::{
    extract::{Path as RutaParam, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::autz::{Actor, Rol};
use crate::rutas::{Error, Estado};

const ROLES: &[&str] = &["ORIGEN", "DESTINO", "USUARIO", "SUPERVISA", "EJECUTA"];

/// Una parte, con el nombre ya resuelto para pintarla.
#[derive(Serialize, sqlx::FromRow)]
pub struct ParteVista {
    id: i64,
    usuario_id: Option<i64>,
    stakeholder_id: Option<i64>,
    nombre: String,
    rol: String,
    es_principal: bool,
    orden: i64,
}

/// Una parte tal como la manda el editor.
#[derive(Deserialize)]
pub struct ParteEntrada {
    #[serde(default)]
    usuario_id: Option<i64>,
    #[serde(default)]
    stakeholder_id: Option<i64>,
    rol: String,
    #[serde(default = "verdadero")]
    es_principal: bool,
    #[serde(default)]
    orden: i64,
}

fn verdadero() -> bool {
    true
}

/// Un usuario o un stakeholder, para poblar los selectores del editor.
#[derive(Serialize, sqlx::FromRow)]
pub struct OpcionParte {
    id: i64,
    nombre: String,
    detalle: Option<String>,
}

fn solo_admin(actor: &Actor) -> Result<(), Error> {
    if matches!(actor.rol, Rol::Admin) {
        Ok(())
    } else {
        Err(Error(StatusCode::FORBIDDEN, "solo administración".into()))
    }
}

fn puede_leer(actor: &Actor) -> Result<(), Error> {
    if matches!(actor.rol, Rol::Admin | Rol::Supervisor) {
        Ok(())
    } else {
        Err(Error(StatusCode::FORBIDDEN, "no autorizado".into()))
    }
}

/// Una parte es interna (un usuario) o externa (un stakeholder), **nunca las dos
/// ni ninguna** —lo mismo que exige el CHECK del esquema, pero contestado aquí
/// con un 400 legible en vez de un error interno de la base—; y su rol tiene que
/// ser del flujo.
fn valida_parte(p: &ParteEntrada) -> Result<(), Error> {
    let rol = p.rol.trim().to_uppercase();
    if !ROLES.contains(&rol.as_str()) {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            format!("rol «{}» no es del flujo (ORIGEN|DESTINO|USUARIO|SUPERVISA|EJECUTA)", p.rol),
        ));
    }
    match (p.usuario_id, p.stakeholder_id) {
        (Some(_), None) | (None, Some(_)) => Ok(()),
        (Some(_), Some(_)) => Err(Error(
            StatusCode::BAD_REQUEST,
            "una parte es un usuario interno o un stakeholder externo, no ambos".into(),
        )),
        (None, None) => Err(Error(
            StatusCode::BAD_REQUEST,
            "una parte necesita un usuario interno o un stakeholder externo".into(),
        )),
    }
}

pub async fn listar(
    State(e): State<Estado>,
    actor: Actor,
    RutaParam(id): RutaParam<i64>,
) -> Result<Json<Vec<ParteVista>>, Error> {
    puede_leer(&actor)?;
    let filas = sqlx::query_as::<_, ParteVista>(
        "SELECT p.id, p.usuario_id, p.stakeholder_id, \
                COALESCE(u.nombre, s.nombre, '(desconocido)') AS nombre, \
                p.rol, p.es_principal, p.orden \
         FROM participacion p \
         LEFT JOIN usuarios u ON u.id = p.usuario_id \
         LEFT JOIN stakeholders s ON s.id = p.stakeholder_id \
         WHERE p.informe_id = ? \
         ORDER BY p.rol, p.es_principal DESC, p.orden",
    )
    .bind(id)
    .fetch_all(&e.bd)
    .await?;
    Ok(Json(filas))
}

/// Reemplaza TODAS las partes del informe por la lista recibida, en una sola
/// transacción: o entra el conjunto completo, o no cambia nada.
pub async fn reemplazar(
    State(e): State<Estado>,
    actor: Actor,
    RutaParam(id): RutaParam<i64>,
    Json(partes): Json<Vec<ParteEntrada>>,
) -> Result<Json<serde_json::Value>, Error> {
    solo_admin(&actor)?;

    // El informe tiene que existir: asignar partes a un informe fantasma sería un
    // dato huérfano que nadie reclama.
    let existe: Option<(i64,)> = sqlx::query_as("SELECT id FROM informes WHERE id = ?")
        .bind(id)
        .fetch_optional(&e.bd)
        .await?;
    if existe.is_none() {
        return Err(Error(StatusCode::NOT_FOUND, "el informe no existe".into()));
    }

    for p in &partes {
        valida_parte(p)?;
    }

    let mut tx = e.bd.begin().await?;
    sqlx::query("DELETE FROM participacion WHERE informe_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for p in &partes {
        // Las claves foráneas están activas: un usuario o stakeholder inexistente
        // hace fallar el INSERT, y se traduce a un 400 legible en vez de un 500.
        sqlx::query(
            "INSERT INTO participacion (informe_id, usuario_id, stakeholder_id, rol, es_principal, orden) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(p.usuario_id)
        .bind(p.stakeholder_id)
        .bind(p.rol.trim().to_uppercase())
        .bind(p.es_principal)
        .bind(p.orden)
        .execute(&mut *tx)
        .await
        .map_err(|err| match err {
            sqlx::Error::Database(db) if db.message().contains("FOREIGN KEY") => Error(
                StatusCode::BAD_REQUEST,
                "una de las partes referencia a un usuario o stakeholder que no existe".into(),
            ),
            otro => otro.into(),
        })?;
    }
    tx.commit().await?;

    crate::rutas::auditar(&e.bd, "participacion_actualizada", Some(&actor.email), None).await;
    Ok(Json(serde_json::json!({ "ok": true, "partes": partes.len() })))
}

/// Usuarios internos activos, para el selector del editor.
pub async fn usuarios(State(e): State<Estado>, actor: Actor) -> Result<Json<Vec<OpcionParte>>, Error> {
    puede_leer(&actor)?;
    let filas = sqlx::query_as::<_, OpcionParte>(
        "SELECT id, nombre, email AS detalle FROM usuarios WHERE activo = 1 ORDER BY nombre",
    )
    .fetch_all(&e.bd)
    .await?;
    Ok(Json(filas))
}

/// Stakeholders, para el selector del editor (el destino externo sale de aquí).
pub async fn stakeholders(State(e): State<Estado>, actor: Actor) -> Result<Json<Vec<OpcionParte>>, Error> {
    puede_leer(&actor)?;
    let filas = sqlx::query_as::<_, OpcionParte>(
        "SELECT id, nombre, email AS detalle FROM stakeholders ORDER BY nombre",
    )
    .fetch_all(&e.bd)
    .await?;
    Ok(Json(filas))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parte(u: Option<i64>, s: Option<i64>, rol: &str) -> ParteEntrada {
        ParteEntrada { usuario_id: u, stakeholder_id: s, rol: rol.into(), es_principal: true, orden: 0 }
    }

    #[test]
    fn una_parte_es_interna_o_externa_nunca_ambas_ni_ninguna() {
        assert!(valida_parte(&parte(Some(1), None, "EJECUTA")).is_ok());
        assert!(valida_parte(&parte(None, Some(9), "DESTINO")).is_ok());
        assert_eq!(valida_parte(&parte(Some(1), Some(9), "EJECUTA")).unwrap_err().0, StatusCode::BAD_REQUEST);
        assert_eq!(valida_parte(&parte(None, None, "EJECUTA")).unwrap_err().0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn el_rol_tiene_que_ser_del_flujo() {
        assert!(valida_parte(&parte(Some(1), None, "ejecuta")).is_ok()); // se normaliza
        assert_eq!(valida_parte(&parte(Some(1), None, "JEFE")).unwrap_err().0, StatusCode::BAD_REQUEST);
    }
}
