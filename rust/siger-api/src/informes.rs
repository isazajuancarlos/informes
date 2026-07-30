//! Gestión de informes y sus reglas desde la interfaz.
//!
//! Un informe es «qué hay que entregar y cada cuánto». La regla —la periodicidad—
//! es el corazón: de ella salen las entregas, las fechas límite y las alertas.
//! Por eso al crear o editar un informe la regla se VALIDA con el mismo motor que
//! después la ejecuta (`Regla::try_from`), y si no se puede leer se rechaza aquí,
//! con el administrador delante, en vez de guardarla y descubrir meses después
//! que ese informe nunca generó una sola entrega.
//!
//! Es administración: crear y editar informes cambia lo que el sistema exige a
//! todos. Verlos, además del administrador, puede hacerlo un supervisor.

use axum::{
    extract::{Path as RutaParam, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::autz::{Actor, Rol};
use crate::rutas::{Error, Estado};

/// La fila de la lista: lo justo para pintar la tabla sin traer la regla entera.
#[derive(Serialize, sqlx::FromRow)]
pub struct InformeFila {
    id: i64,
    nombre: String,
    descripcion: Option<String>,
    activo: bool,
    /// El modo de la regla, ya extraído, para que la lista lo muestre sin que el
    /// navegador tenga que parsear el JSON.
    modo: Option<String>,
}

/// El detalle: incluye la regla cruda, tal como está guardada, para que el editor
/// la cargue y la modifique.
#[derive(Serialize, sqlx::FromRow)]
pub struct InformeDetalle {
    id: i64,
    nombre: String,
    descripcion: Option<String>,
    activo: bool,
    /// El JSON de la regla, verbatim. El front lo interpreta con `ReglaJson`.
    recurrencia: String,
}

/// Lo que manda el formulario al crear o editar. La regla llega ya como la
/// estructura que el motor entiende, no como texto suelto.
#[derive(Deserialize)]
pub struct InformeEntrada {
    nombre: String,
    #[serde(default)]
    descripcion: Option<String>,
    /// La regla como el cliente la manda. Se guarda VERBATIM tras validarla —
    /// como hace el volcado—, no re-serializada: así los informes nuevos quedan
    /// en el mismo JSON compacto que los migrados, sin campos nulos de relleno.
    recurrencia: serde_json::Value,
    /// Solo lo usa la edición. Al crear, un informe nace activo.
    #[serde(default = "activo_por_defecto")]
    activo: bool,
}

fn activo_por_defecto() -> bool {
    true
}

/// Solo administración escribe; administración y supervisión leen.
fn puede_escribir(actor: &Actor) -> Result<(), Error> {
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

/// Valida la regla y la devuelve como el JSON compacto que se guarda. Rechazar
/// aquí una regla ilegible es el punto entero del módulo: un informe con una
/// regla que no parsea es un informe que nunca vencerá, y el fallo aparecería
/// lejos de su causa.
///
/// Se valida sobre `ReglaJson` (que el motor entiende) pero se guarda el `Value`
/// tal como llegó: así lo nuevo queda en el mismo JSON compacto que lo migrado.
fn regla_valida(v: &serde_json::Value) -> Result<String, Error> {
    let rj: siger_core::ReglaJson = serde_json::from_value(v.clone())
        .map_err(|e| Error(StatusCode::BAD_REQUEST, format!("la regla no tiene la forma esperada: {e}")))?;
    siger_core::Regla::try_from(&rj)
        .map_err(|e| Error(StatusCode::BAD_REQUEST, format!("la regla no es válida: {e}")))?;
    Ok(v.to_string())
}

fn nombre_valido(nombre: &str) -> Result<String, Error> {
    let n = nombre.trim();
    if n.is_empty() {
        return Err(Error(StatusCode::BAD_REQUEST, "el nombre no puede estar vacío".into()));
    }
    if n.chars().count() > 200 {
        return Err(Error(StatusCode::BAD_REQUEST, "el nombre no puede pasar de 200 caracteres".into()));
    }
    Ok(n.to_string())
}

pub async fn listar(State(e): State<Estado>, actor: Actor) -> Result<Json<Vec<InformeFila>>, Error> {
    puede_leer(&actor)?;
    // El modo se extrae en SQL con json_extract: es una lectura, no vale la pena
    // traer la regla entera a Rust para sacarle un campo.
    let filas = sqlx::query_as::<_, InformeFila>(
        "SELECT id, nombre, descripcion, activo, \
         json_extract(recurrencia, '$.modo') AS modo \
         FROM informes ORDER BY activo DESC, nombre",
    )
    .fetch_all(&e.bd)
    .await?;
    Ok(Json(filas))
}

pub async fn detalle(
    State(e): State<Estado>,
    actor: Actor,
    RutaParam(id): RutaParam<i64>,
) -> Result<Json<InformeDetalle>, Error> {
    puede_leer(&actor)?;
    let fila = sqlx::query_as::<_, InformeDetalle>(
        "SELECT id, nombre, descripcion, activo, recurrencia FROM informes WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&e.bd)
    .await?
    .ok_or_else(|| Error(StatusCode::NOT_FOUND, "el informe no existe".into()))?;
    Ok(Json(fila))
}

pub async fn crear(
    State(e): State<Estado>,
    actor: Actor,
    Json(entrada): Json<InformeEntrada>,
) -> Result<(StatusCode, Json<serde_json::Value>), Error> {
    puede_escribir(&actor)?;
    let nombre = nombre_valido(&entrada.nombre)?;
    let recurrencia = regla_valida(&entrada.recurrencia)?;

    // El nombre es único en el esquema. Un choque se responde como conflicto
    // legible, no como «error interno»: es un dato que el administrador puede
    // corregir, no un fallo del sistema.
    let r = sqlx::query("INSERT INTO informes (nombre, descripcion, recurrencia, activo) VALUES (?, ?, ?, ?)")
        .bind(&nombre)
        .bind(&entrada.descripcion)
        .bind(&recurrencia)
        .bind(entrada.activo)
        .execute(&e.bd)
        .await;

    let id = match r {
        Ok(res) => res.last_insert_rowid(),
        Err(sqlx::Error::Database(db)) if db.is_unique_violation() => {
            return Err(Error(StatusCode::CONFLICT, format!("ya existe un informe llamado «{nombre}»")));
        }
        Err(e) => return Err(e.into()),
    };

    crate::rutas::auditar(&e.bd, "informe_creado", Some(&actor.email), Some(&nombre)).await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

pub async fn actualizar(
    State(e): State<Estado>,
    actor: Actor,
    RutaParam(id): RutaParam<i64>,
    Json(entrada): Json<InformeEntrada>,
) -> Result<Json<serde_json::Value>, Error> {
    puede_escribir(&actor)?;
    let nombre = nombre_valido(&entrada.nombre)?;
    let recurrencia = regla_valida(&entrada.recurrencia)?;

    let r = sqlx::query(
        "UPDATE informes SET nombre = ?, descripcion = ?, recurrencia = ?, activo = ? WHERE id = ?",
    )
    .bind(&nombre)
    .bind(&entrada.descripcion)
    .bind(&recurrencia)
    .bind(entrada.activo)
    .bind(id)
    .execute(&e.bd)
    .await;

    let filas = match r {
        Ok(res) => res.rows_affected(),
        Err(sqlx::Error::Database(db)) if db.is_unique_violation() => {
            return Err(Error(StatusCode::CONFLICT, format!("ya existe un informe llamado «{nombre}»")));
        }
        Err(e) => return Err(e.into()),
    };
    if filas == 0 {
        return Err(Error(StatusCode::NOT_FOUND, "el informe no existe".into()));
    }

    // Cambiar la regla NO borra ni recalcula las entregas ya existentes: la
    // historia no se reescribe. Las entregas futuras salen de la nueva regla
    // cuando el planificador corra; para verlas ya, «generar ahora».
    crate::rutas::auditar(&e.bd, "informe_actualizado", Some(&actor.email), Some(&nombre)).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Lo que la pantalla del cronograma manda: los días marcados y el ajuste.
#[derive(Deserialize)]
pub struct Seleccion {
    /// Días marcados en el calendario, en `YYYY-MM-DD`.
    fechas: Vec<String>,
    /// Cómo tratar los días no hábiles. La inferencia no lo adivina.
    #[serde(default)]
    ajuste: siger_core::inferencia::AjusteSel,
}

/// Qué puede ser lo que el usuario marcó en el calendario.
///
/// Devuelve la lista de lecturas posibles —«todas las semanas, lunes y jueves»,
/// «el 5º día hábil de cada mes»— cada una con las próximas fechas que
/// produciría y con la regla que se guardaría si se elige. Solo aparecen lecturas
/// que CONTIENEN lo marcado: si una interpretación no generase alguna de las
/// fechas señaladas, no es una lectura de lo que se señaló.
///
/// Es una previsualización, así que no escribe nada; pero se restringe a quien
/// puede crear informes, porque es parte de ese flujo y no hay razón para
/// ofrecerla a quien no va a poder guardar.
pub async fn lecturas(
    State(e): State<Estado>,
    actor: Actor,
    Json(sel): Json<Seleccion>,
) -> Result<Json<serde_json::Value>, Error> {
    puede_escribir(&actor)?;

    if sel.fechas.is_empty() {
        return Err(Error(StatusCode::BAD_REQUEST, "no se marcó ninguna fecha".into()));
    }
    // Un tope para que la petición no se convierta en un barrido: marcar un año
    // entero a mano no es la operación que esta pantalla sirve.
    if sel.fechas.len() > 366 {
        return Err(Error(StatusCode::BAD_REQUEST, "demasiadas fechas marcadas".into()));
    }

    // Una fecha ilegible es un error del cliente, no un día que se descarta en
    // silencio: descartarlo cambiaría la lectura sin decírselo a nadie.
    let mut dias = Vec::with_capacity(sel.fechas.len());
    for f in &sel.fechas {
        let d = chrono::NaiveDate::parse_from_str(f, "%Y-%m-%d")
            .map_err(|_| Error(StatusCode::BAD_REQUEST, format!("fecha ilegible: {f}")))?;
        dias.push(d);
    }

    let lecturas = siger_core::inferencia::interpretaciones_con(&dias, &e.motor, sel.ajuste);
    Ok(Json(serde_json::json!({ "lecturas": lecturas })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regla(json: &str) -> serde_json::Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn una_regla_ilegible_se_rechaza_antes_de_guardar() {
        // El punto entero del módulo: un modo que el motor no conoce no se
        // guarda. Si esto pasara, el informe existiría pero no vencería nunca.
        let e = regla_valida(&regla(r#"{"modo":"QUINCENAL_LUNAR"}"#)).unwrap_err();
        assert_eq!(e.0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn una_regla_valida_se_guarda_compacta_y_verbatim() {
        // Incluye «a solicitud»: un informe sin periodicidad es legítimo. Lo
        // guardado, parseado de vuelta, es exactamente lo que llegó —sin campos
        // nulos de relleno—: el JSON no crece al pasar por aquí.
        for entrada in [
            r#"{"modo":"INMEDIATO"}"#,
            r#"{"modo":"SEMANAL","dias_semana":[0],"fecha_inicio":"2026-01-01"}"#,
        ] {
            let v = regla(entrada);
            let guardado = regla_valida(&v).unwrap();
            let re: serde_json::Value = serde_json::from_str(&guardado).unwrap();
            assert_eq!(re, v, "lo guardado debe ser lo que llegó, sin añadir nulos");
        }
    }

    #[test]
    fn el_nombre_no_puede_ir_vacio_ni_pasarse() {
        assert_eq!(nombre_valido("  ").unwrap_err().0, StatusCode::BAD_REQUEST);
        assert_eq!(nombre_valido(&"x".repeat(201)).unwrap_err().0, StatusCode::BAD_REQUEST);
        assert_eq!(nombre_valido("  Informe anual  ").unwrap(), "Informe anual");
    }
}
