//! Flujo del supervisor (#163): notificaciones internas, justificación de un
//! incumplimiento y su revisión.
//!
//! La mensajería es INTERNA (tabla `notificaciones`); si el correo está
//! configurado, se manda además una COPIA por email. La copia es best-effort:
//! nunca tumba la operación —la notificación interna ya quedó guardada—.
//!
//! El ciclo: un incumplimiento se justifica (el operador escribe, `estado =
//! PENDIENTE`), el supervisor lo revisa (`ACEPTADA` / `RECHAZADA`) y da
//! retroalimentación. Cada paso deja un comentario en el hilo de la entrega y
//! notifica a la otra parte.

use axum::{
    extract::{Path as RutaParam, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::autz::{Actor, Alcance, Rol};
use crate::rutas::{Error, Estado};

// --- Notificaciones -----------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct Notificacion {
    pub(crate) id: i64,
    pub(crate) tipo: String,
    pub(crate) titulo: String,
    pub(crate) mensaje: String,
    pub(crate) entrega_id: Option<i64>,
    pub(crate) leida: bool,
    pub(crate) momento: String,
}

/// Crea una notificación interna para un usuario y, si el correo está activo, le
/// manda una copia. Errores se registran, no se propagan: avisar no puede tumbar
/// la acción que lo dispara.
async fn notificar(
    e: &Estado,
    usuario_id: i64,
    tipo: &str,
    titulo: &str,
    mensaje: &str,
    entrega_id: Option<i64>,
) {
    if let Err(err) = sqlx::query(
        "INSERT INTO notificaciones (usuario_id, tipo, titulo, mensaje, entrega_id, momento) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(usuario_id)
    .bind(tipo)
    .bind(titulo)
    .bind(mensaje)
    .bind(entrega_id)
    .bind(Utc::now().to_rfc3339())
    .execute(&e.bd)
    .await
    {
        tracing::error!("no se pudo crear la notificación para {usuario_id}: {err}");
    }

    if let Some(c) = &e.cfg.correo {
        if let Ok(Some((Some(email),))) =
            sqlx::query_as::<_, (Option<String>,)>("SELECT email FROM usuarios WHERE id = ? AND activo = 1")
                .bind(usuario_id)
                .fetch_optional(&e.bd)
                .await
        {
            // La copia va con un cuerpo un poco más largo que el título.
            let cuerpo = format!("{mensaje}\n\nEste es un aviso de SIGER.");
            let _ = crate::alertas::enviar(c, &email, titulo, &cuerpo).await;
        }
    }
}

/// Los supervisores y administradores activos: a ellos se les avisa de una
/// justificación nueva.
async fn supervisores(bd: &sqlx::SqlitePool) -> Vec<i64> {
    sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM usuarios WHERE activo = 1 AND rol IN ('SUPERVISOR', 'ADMIN')",
    )
    .fetch_all(bd)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|(id,)| id)
    .collect()
}

/// Las notificaciones de este actor, y solo las suyas: el filtro es su id, no un
/// parámetro que alguien pueda cambiar. Aparte del manejador porque la usan la
/// API y el panel.
pub(crate) async fn notificaciones_de(e: &Estado, actor: &Actor) -> Result<Vec<Notificacion>, Error> {
    // Cada quien ve las SUYAS: el filtro es el id del actor, no un parámetro.
    let filas = sqlx::query_as::<_, Notificacion>(
        "SELECT id, tipo, titulo, mensaje, entrega_id, leida, momento \
         FROM notificaciones WHERE usuario_id = ? ORDER BY leida, momento DESC LIMIT 100",
    )
    .bind(actor.id)
    .fetch_all(&e.bd)
    .await?;
    Ok(filas)
}

pub async fn listar_notificaciones(
    State(e): State<Estado>,
    actor: Actor,
) -> Result<Json<Vec<Notificacion>>, Error> {
    Ok(Json(notificaciones_de(&e, &actor).await?))
}

/// Marca una notificación como leída. **El `AND usuario_id` es la guardia**, y
/// va dentro del `UPDATE`: comprobar la pertenencia antes, en otra consulta,
/// deja un hueco entre las dos. Aparte del manejador porque la usan la API y el
/// panel.
pub(crate) async fn marcar_leida_de(e: &Estado, actor: &Actor, id: i64) -> Result<(), Error> {
    sqlx::query("UPDATE notificaciones SET leida = 1 WHERE id = ? AND usuario_id = ?")
        .bind(id)
        .bind(actor.id)
        .execute(&e.bd)
        .await?;
    Ok(())
}

pub async fn marcar_leida(
    State(e): State<Estado>,
    actor: Actor,
    RutaParam(id): RutaParam<i64>,
) -> Result<Json<serde_json::Value>, Error> {
    marcar_leida_de(&e, &actor, id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// --- Hilo de comentarios de una entrega --------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct Comentario {
    id: i64,
    autor: String,
    texto: String,
    momento: String,
}

/// ¿Puede el actor ver esta entrega? Misma lógica de alcance que la descarga y
/// la verificación: la pertenencia se comprueba en la consulta, no después.
async fn entrega_visible(e: &Estado, alcance: &Alcance, id: i64) -> Result<bool, Error> {
    let (sql, param) = match alcance {
        Alcance::Todas => ("SELECT 1 FROM entregas e WHERE e.id = ?", None),
        Alcance::DeUsuario(u) => ("SELECT 1 FROM entregas e WHERE e.id = ? AND e.usuario_id = ?", Some(*u)),
        Alcance::DeStakeholder(s) => (
            "SELECT 1 FROM entregas e WHERE e.id = ? AND EXISTS \
             (SELECT 1 FROM participacion p WHERE p.informe_id = e.informe_id AND p.stakeholder_id = ?)",
            Some(*s),
        ),
        Alcance::Ninguna => return Ok(false),
    };
    let mut q = sqlx::query_as::<_, (i64,)>(sql).bind(id);
    if let Some(p) = param {
        q = q.bind(p);
    }
    Ok(q.fetch_optional(&e.bd).await?.is_some())
}

pub async fn listar_comentarios(
    State(e): State<Estado>,
    actor: Actor,
    RutaParam(id): RutaParam<i64>,
) -> Result<Json<Vec<Comentario>>, Error> {
    if !entrega_visible(&e, &actor.alcance(), id).await? {
        return Err(Error(StatusCode::NOT_FOUND, "la entrega no existe".into()));
    }
    let filas = sqlx::query_as::<_, Comentario>(
        "SELECT c.id, COALESCE(u.nombre, '(sistema)') AS autor, c.texto, c.momento \
         FROM comentarios c LEFT JOIN usuarios u ON u.id = c.autor_id \
         WHERE c.entrega_id = ? ORDER BY c.momento",
    )
    .bind(id)
    .fetch_all(&e.bd)
    .await?;
    Ok(Json(filas))
}

// --- Justificar un incumplimiento (operador) ---------------------------------

#[derive(Deserialize)]
pub struct Justificacion {
    texto: String,
}

async fn entrega_contexto(e: &Estado, id: i64) -> Result<(Option<i64>, String), Error> {
    sqlx::query_as::<_, (Option<i64>, String)>(
        "SELECT e.usuario_id, i.nombre FROM entregas e JOIN informes i ON i.id = e.informe_id WHERE e.id = ?",
    )
    .bind(id)
    .fetch_optional(&e.bd)
    .await?
    .ok_or_else(|| Error(StatusCode::NOT_FOUND, "la entrega no existe".into()))
}

pub async fn justificar(
    State(e): State<Estado>,
    actor: Actor,
    RutaParam(id): RutaParam<i64>,
    Json(j): Json<Justificacion>,
) -> Result<Json<serde_json::Value>, Error> {
    let texto = j.texto.trim();
    if texto.is_empty() {
        return Err(Error(StatusCode::BAD_REQUEST, "la justificación no puede ir vacía".into()));
    }
    let (usuario_id, informe) = entrega_contexto(&e, id).await?;
    // Justifica el responsable de la entrega (o un admin en su nombre). Un
    // tercero o un operador ajeno no justifican lo que no es suyo.
    let responsable = usuario_id == Some(actor.id);
    if !(responsable || matches!(actor.rol, Rol::Admin)) {
        return Err(Error(StatusCode::FORBIDDEN, "solo el responsable de la entrega puede justificarla".into()));
    }

    let mut tx = e.bd.begin().await?;
    sqlx::query("INSERT INTO comentarios (entrega_id, autor_id, texto, momento) VALUES (?, ?, ?, ?)")
        .bind(id)
        .bind(actor.id)
        .bind(texto)
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE entregas SET justificacion_estado = 'PENDIENTE' WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    // Avisar a la supervisión (interna + copia por correo).
    let titulo = format!("Justificación por revisar: «{informe}»");
    let mensaje = format!("{} justificó la entrega #{id} de «{informe}». Revísala.", actor.nombre);
    for sup in supervisores(&e.bd).await {
        if sup != actor.id {
            notificar(&e, sup, "justificacion", &titulo, &mensaje, Some(id)).await;
        }
    }
    crate::rutas::auditar(&e.bd, "justificacion_enviada", Some(&actor.email), Some(&informe)).await;
    Ok(Json(serde_json::json!({ "ok": true, "estado": "PENDIENTE" })))
}

// --- Revisar la justificación (supervisor / admin) ---------------------------

#[derive(Deserialize)]
pub struct Revision {
    aceptar: bool,
    #[serde(default)]
    retroalimentacion: String,
}

pub async fn revisar(
    State(e): State<Estado>,
    actor: Actor,
    RutaParam(id): RutaParam<i64>,
    Json(r): Json<Revision>,
) -> Result<Json<serde_json::Value>, Error> {
    if !matches!(actor.rol, Rol::Admin | Rol::Supervisor) {
        return Err(Error(StatusCode::FORBIDDEN, "solo supervisión revisa".into()));
    }
    let (usuario_id, informe) = entrega_contexto(&e, id).await?;
    let nuevo = if r.aceptar { "ACEPTADA" } else { "RECHAZADA" };
    let retro = r.retroalimentacion.trim();

    let mut tx = e.bd.begin().await?;
    let filas = sqlx::query("UPDATE entregas SET justificacion_estado = ? WHERE id = ?")
        .bind(nuevo)
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if filas == 0 {
        return Err(Error(StatusCode::NOT_FOUND, "la entrega no existe".into()));
    }
    // La retroalimentación, si la hay, queda en el hilo con el veredicto.
    let comentario = if retro.is_empty() {
        format!("[{}] la justificación fue {}.", actor.nombre, nuevo.to_lowercase())
    } else {
        format!("[{}] {} — {retro}", actor.nombre, nuevo.to_lowercase())
    };
    sqlx::query("INSERT INTO comentarios (entrega_id, autor_id, texto, momento) VALUES (?, ?, ?, ?)")
        .bind(id)
        .bind(actor.id)
        .bind(&comentario)
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    // Avisar al responsable de la entrega.
    if let Some(op) = usuario_id {
        let titulo = format!("Tu justificación fue {}: «{informe}»", nuevo.to_lowercase());
        notificar(&e, op, "revision", &titulo, &comentario, Some(id)).await;
    }
    crate::rutas::auditar(&e.bd, "justificacion_revisada", Some(&actor.email), Some(&informe)).await;
    Ok(Json(serde_json::json!({ "ok": true, "estado": nuevo })))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn base() -> Estado {
        let bd = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&bd).await.unwrap();
        Estado { bd, cfg: crate::config::Config::de_prueba(), motor: motor_de_prueba() }
    }

    /// Motor con un rango de años corto: en pruebas no hace falta un siglo de
    /// calendario, y construirlo en cada caso se notaría en la suite.
    fn motor_de_prueba() -> std::sync::Arc<siger_core::Motor> {
        std::sync::Arc::new(siger_core::Motor::nuevo(
            siger_core::Calendario::de_pais("CO", 2024, 2030).unwrap(),
        ))
    }

    async fn sembrar(e: &Estado) {
        sqlx::query("INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) VALUES (1,'Op','op@x.co','h','OPERADOR',1),(2,'Sup','sup@x.co','h','SUPERVISOR',1)").execute(&e.bd).await.unwrap();
        sqlx::query("INSERT INTO informes (id,nombre,recurrencia) VALUES (5,'Informe','{\"modo\":\"INMEDIATO\"}')").execute(&e.bd).await.unwrap();
        sqlx::query("INSERT INTO entregas (id,informe_id,usuario_id,fecha_nominal,fecha_limite,estado) VALUES (9,5,1,'2026-01-01','2026-01-01','PENDIENTE')").execute(&e.bd).await.unwrap();
    }

    #[tokio::test]
    async fn justificar_marca_pendiente_y_notifica_al_supervisor() {
        let e = base().await;
        sembrar(&e).await;
        let op = Actor { id: 1, nombre: "Op".into(), email: "op@x.co".into(), rol: Rol::Operador, stakeholder_id: None };
        let _ = justificar(State(e.clone()), op, RutaParam(9), Json(Justificacion { texto: "hubo un corte".into() })).await.unwrap();
        let estado: Option<String> = sqlx::query_scalar("SELECT justificacion_estado FROM entregas WHERE id=9").fetch_one(&e.bd).await.unwrap();
        assert_eq!(estado.as_deref(), Some("PENDIENTE"));
        let n_sup: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notificaciones WHERE usuario_id=2").fetch_one(&e.bd).await.unwrap();
        assert_eq!(n_sup, 1, "el supervisor recibe una notificación");
        let n_com: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM comentarios WHERE entrega_id=9").fetch_one(&e.bd).await.unwrap();
        assert_eq!(n_com, 1);
    }

    #[tokio::test]
    async fn un_tercero_no_justifica_lo_ajeno() {
        let e = base().await;
        sembrar(&e).await;
        // Otro operador (id 3) que no es el responsable (id 1).
        sqlx::query("INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) VALUES (3,'Otro','o@x.co','h','OPERADOR',1)").execute(&e.bd).await.unwrap();
        let otro = Actor { id: 3, nombre: "Otro".into(), email: "o@x.co".into(), rol: Rol::Operador, stakeholder_id: None };
        let err = justificar(State(e.clone()), otro, RutaParam(9), Json(Justificacion { texto: "x".into() })).await.unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn revisar_acepta_y_notifica_al_operador() {
        let e = base().await;
        sembrar(&e).await;
        let sup = Actor { id: 2, nombre: "Sup".into(), email: "sup@x.co".into(), rol: Rol::Supervisor, stakeholder_id: None };
        let _ = revisar(State(e.clone()), sup, RutaParam(9), Json(Revision { aceptar: true, retroalimentacion: "ok".into() })).await.unwrap();
        let estado: Option<String> = sqlx::query_scalar("SELECT justificacion_estado FROM entregas WHERE id=9").fetch_one(&e.bd).await.unwrap();
        assert_eq!(estado.as_deref(), Some("ACEPTADA"));
        let n_op: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notificaciones WHERE usuario_id=1").fetch_one(&e.bd).await.unwrap();
        assert_eq!(n_op, 1, "el operador recibe el veredicto");
    }

    #[tokio::test]
    async fn un_operador_no_revisa() {
        let e = base().await;
        sembrar(&e).await;
        let op = Actor { id: 1, nombre: "Op".into(), email: "op@x.co".into(), rol: Rol::Operador, stakeholder_id: None };
        let err = revisar(State(e.clone()), op, RutaParam(9), Json(Revision { aceptar: true, retroalimentacion: "".into() })).await.unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }
}
