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

use crate::autz::{Actor, Rol};
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
        // **La misma expresión que se corrigió en `alertas::destinatarios`**, y
        // estaba TRES LÍNEAS por encima del arreglo anterior: el commit que puso
        // el `warn!` de abajo entró en esta función, endureció la sentencia
        // siguiente y dejó ésta intacta —afirmando de paso «cero degradaciones
        // silenciosas en el camino del correo», que era falso—. Tercera vez que
        // el mismo gemelo aparece a un lado del sitio mirado.
        let destino = match sqlx::query_as::<_, (Option<String>,)>(
            "SELECT email FROM usuarios WHERE id = ? AND activo = 1",
        )
        .bind(usuario_id)
        .fetch_optional(&e.bd)
        .await
        {
            Ok(v) => v.and_then(|(e,)| e),
            Err(err) => {
                // Sin esto, un `SQLITE_BUSY` era indistinguible de «esa cuenta no
                // tiene correo» y de «está inactiva». `warn!` por lo mismo que el
                // de abajo: la notificación DENTRO de la aplicación ya se creó,
                // así que no hay nada roto que atender en el acto.
                tracing::warn!(
                    "no se pudo leer el correo de {usuario_id} para la copia — {err}; \
                     la notificación en la aplicación sí quedó creada"
                );
                None
            }
        };
        if let Some(email) = destino {
            // La copia va con un cuerpo un poco más largo que el título.
            let cuerpo = format!("{mensaje}\n\nEste es un aviso de SIGER.");
            // El resultado se MIRA. Aquí el `let _ =` era defendible —la
            // notificación dentro de la aplicación ya se creó arriba, así que
            // nadie se queda sin enterarse— pero que la copia no saliera no se
            // veía en ninguna parte, y es la misma familia del #604. `warn!` y
            // no `error!` a propósito: no hay nada roto que arreglar en el acto,
            // y el aviso llegó por el canal que sí manda.
            if !crate::alertas::enviar(c, &email, titulo, &cuerpo).await {
                tracing::warn!(
                    "no se pudo enviar la copia por correo a {email}; \
                     la notificación en la aplicación sí quedó creada"
                );
            }
        }
    }
}

/// Los supervisores y administradores activos: a ellos se les avisa de una
/// justificación nueva.
///
/// **Devuelve `Result` a propósito.** Degradaba a lista vacía, y eso es
/// indistinguible de «esta organización no tiene supervisores»: nadie recibe el
/// aviso y nada lo dice. La ronda anterior le puso un `tracing::error!` y no
/// bastaba — en ese mismo commit se decidió que el fallo del planificador, que
/// se reintenta solo en 24 h, merecía la bitácora, y a éste, **que no se
/// reintenta jamás**, se le dio un canal más débil sin escribir por qué.
///
/// Lo que lo decide es el amplificador: `justificacion_estado` se ESCRIBE y no
/// se lee en ninguna consulta de producción, así que la notificación es el
/// ÚNICO camino por el que un supervisor se entera de que hay una justificación
/// esperando. Si la lista sale vacía por un `SQLITE_BUSY`, esa justificación
/// entra en un estado que ningún observable del producto muestra.
async fn supervisores(bd: &sqlx::SqlitePool) -> Result<Vec<i64>, sqlx::Error> {
    Ok(sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM usuarios WHERE activo = 1 AND rol IN ('SUPERVISOR', 'ADMIN')",
    )
    .fetch_all(bd)
    .await?
    .into_iter()
    .map(|(id,)| id)
    .collect())
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
    #[allow(dead_code)]
    pub(crate) id: i64,
    pub(crate) autor: String,
    pub(crate) texto: String,
    pub(crate) momento: String,
}

/// ¿Puede el actor ver esta entrega? Misma lógica de alcance que la descarga y
/// la verificación: la pertenencia se comprueba en la consulta, no después.
async fn entrega_visible(e: &Estado, actor: &Actor, id: i64) -> Result<bool, Error> {
    use crate::autz::{Accion, Predicado};
    let (sql, binds) = match actor.predicado(Accion::VerEntregas, "e.", "e.informe_id") {
        Predicado::SinFiltro => ("SELECT 1 FROM entregas e WHERE e.id = ?".to_string(), Vec::new()),
        Predicado::Filtro { sql, binds } => {
            (format!("SELECT 1 FROM entregas e WHERE e.id = ? AND ({sql})"), binds)
        }
        Predicado::Nada => return Ok(false),
    };
    let mut q = sqlx::query_as::<_, (i64,)>(sql.as_str()).bind(id);
    for b in binds {
        q = q.bind(b);
    }
    Ok(q.fetch_optional(&e.bd).await?.is_some())
}

/// El hilo de una entrega. Aparte del manejador porque lo usan la API y el
/// panel, y `entrega_visible` va DENTRO: la pertenencia se comprueba en la
/// consulta, no después.
pub(crate) async fn comentarios_de(
    e: &Estado,
    actor: &Actor,
    id: i64,
) -> Result<Vec<Comentario>, Error> {
    if !entrega_visible(e, actor, id).await? {
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
    Ok(filas)
}

pub async fn listar_comentarios(
    State(e): State<Estado>,
    actor: Actor,
    RutaParam(id): RutaParam<i64>,
) -> Result<Json<Vec<Comentario>>, Error> {
    // La QUINTA puerta sobre un id de entrega, y era la muda. Su 404 es
    // uniforme —eso ya estaba— pero no dejaba línea, así que enumerar ids por
    // aquí salía gratis y sin rastro. El 404 de `comentarios_de` solo lo produce
    // `entrega_visible == false`; un fallo de base sale con otro código y no se
    // anota, para no meter ruido en la única señal que distingue el sondeo.
    match comentarios_de(&e, &actor, id).await {
        Ok(v) => Ok(Json(v)),
        Err(err) => {
            if err.0 == StatusCode::NOT_FOUND {
                crate::rutas::auditar_entrega_ajena(&e, &actor, id).await;
            }
            Err(err)
        }
    }
}

// --- Justificar un incumplimiento (operador) ---------------------------------

#[derive(Deserialize)]
pub struct Justificacion {
    pub(crate) texto: String,
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

/// Justifica una entrega. Aparte del manejador porque la usan
/// `POST /api/entregas/{id}/justificacion` y el panel, y se lleva DENTRO la
/// guardia: justifica el responsable —o un admin en su nombre—, no cualquiera
/// con sesión. También el aviso a supervisión y la línea de bitácora: un panel
/// que justificara sin avisar dejaría la justificación esperando a que alguien
/// la descubriera de casualidad.
pub(crate) async fn justificar_de(
    e: &Estado,
    actor: &Actor,
    id: i64,
    j: &Justificacion,
) -> Result<(), Error> {
    let texto = j.texto.trim();
    if texto.is_empty() {
        return Err(Error(StatusCode::BAD_REQUEST, "la justificación no puede ir vacía".into()));
    }
    let (usuario_id, informe) = entrega_contexto(e, id).await?;

    // EL ALCANCE VA ANTES QUE EL RESPONSABLE, y ese orden es toda la regla.
    //
    // Al revés —como estaba hasta el 2026-08-10— esto era un ORÁCULO DE
    // EXISTENCIA: a un operador, una entrega ajena que EXISTE le respondía 403 y
    // una inexistente 404, así que probando números aprendía qué ids hay sin
    // llegar a ver nada. Es el mismo agujero que `descargar_archivo` y
    // `comentarios_de` ya cerraban con 404 uniforme; aquí faltaba, y una
    // salvaguarda que solo cubre dos de tres puertas no cubre ninguna.
    //
    // Fuera de alcance se contesta EXACTAMENTE lo mismo que a una entrega que no
    // existe. Lo que discrimina es lo que se ESCRIBE —la línea de bitácora—, no
    // lo que se contesta.
    if !crate::rutas::entrega_en_alcance(e, actor, id).await? {
        crate::rutas::auditar_entrega_ajena(e, actor, id).await;
        return Err(Error(StatusCode::NOT_FOUND, "la entrega no existe".into()));
    }

    // Justifica el responsable de la entrega (o un admin en su nombre). Un
    // tercero o un operador ajeno no justifican lo que no es suyo. Aquí el 403
    // ya no filtra nada: para llegar, la entrega tiene que estar dentro del
    // alcance de quien pide, o sea que ya podía verla.
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
    match supervisores(&e.bd).await {
        Ok(sups) => {
            for sup in sups {
                if sup != actor.id {
                    notificar(e, sup, "justificacion", &titulo, &mensaje, Some(id)).await;
                }
            }
        }
        Err(err) => {
            // **La justificación SÍ quedó guardada** —la transacción ya hizo
            // `commit`— así que no se le devuelve un error a quien la escribió:
            // su trabajo no se perdió y repetirlo no arreglaría nada. Lo que no
            // puede pasar es que nadie se entere, y por eso queda en la bitácora,
            // que es donde se consulta, y no solo en el log del proceso.
            tracing::error!("no se pudo leer la supervisión para avisar — {err}");
            crate::rutas::auditar(
                &e.bd,
                "justificacion_sin_avisar",
                Some(&actor.email),
                Some(&format!("{informe}: no se pudo avisar a supervisión — {err}")),
            )
            .await;
        }
    }
    crate::rutas::auditar(&e.bd, "justificacion_enviada", Some(&actor.email), Some(&informe)).await;
    Ok(())
}

pub async fn justificar(
    State(e): State<Estado>,
    actor: Actor,
    RutaParam(id): RutaParam<i64>,
    Json(j): Json<Justificacion>,
) -> Result<Json<serde_json::Value>, Error> {
    justificar_de(&e, &actor, id, &j).await?;
    Ok(Json(serde_json::json!({ "ok": true, "estado": "PENDIENTE" })))
}

// --- Revisar la justificación (supervisor / admin) ---------------------------

#[derive(Deserialize)]
pub struct Revision {
    pub(crate) aceptar: bool,
    #[serde(default)]
    pub(crate) retroalimentacion: String,
}

/// Revisa una justificación. La guardia va dentro: SOLO supervisión.
pub(crate) async fn revisar_de(
    e: &Estado,
    actor: &Actor,
    id: i64,
    r: &Revision,
) -> Result<&'static str, Error> {
    if !matches!(actor.rol, Rol::Admin | Rol::Supervisor) {
        return Err(Error(StatusCode::FORBIDDEN, "solo supervisión revisa".into()));
    }
    let (usuario_id, informe) = entrega_contexto(e, id).await?;
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
        // Rollback EXPLÍCITO en vez de fiarlo al `Drop` de `sqlx` (que revierte
        // una tx no confirmada): así un `commit` movido de sitio en una edición
        // futura no lo rompe en silencio (residuo de la revisión del 2026-08-16).
        // `entrega_contexto` ya la buscó arriba, así que 0 filas es una carrera
        // —la entrega se borró entremedias— y se trata como NOT_FOUND.
        tx.rollback().await?;
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
        notificar(e, op, "revision", &titulo, &comentario, Some(id)).await;
    }
    crate::rutas::auditar(&e.bd, "justificacion_revisada", Some(&actor.email), Some(&informe)).await;
    Ok(nuevo)
}

pub async fn revisar(
    State(e): State<Estado>,
    actor: Actor,
    RutaParam(id): RutaParam<i64>,
    Json(r): Json<Revision>,
) -> Result<Json<serde_json::Value>, Error> {
    let nuevo = revisar_de(&e, &actor, id, &r).await?;
    Ok(Json(serde_json::json!({ "ok": true, "estado": nuevo })))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **La QUINTA puerta sobre un id de entrega, y era la más barata.**
    ///
    /// `comentarios_de` cerraba el 404 uniforme —eso ya estaba— pero no dejaba
    /// línea, por sus dos extremos: `GET /api/entregas/{id}/comentarios` y
    /// `GET /entregas/{id}`, la pantalla. Cerrar la subida y dejar abierto un
    /// `GET` sin cuerpo, automatizable con un bucle de `curl`, es cerrar la
    /// puerta de servicio con la principal abierta. Lo midió la revisión de este
    /// mismo arreglo, que además cazó que el `CLAUDE.md` había pasado a afirmar
    /// «las CUATRO puertas» cuando eran seis.
    ///
    /// Las tres mitades: la ajena que existe SÍ, la inexistente NO, y la propia
    /// NO —sin esta última, una versión que anotara en cada `GET` de la pantalla
    /// pasaría las dos primeras y llenaría la bitácora de ruido hasta que nadie
    /// la mirase (directiva 23)—.
    #[tokio::test]
    async fn leer_el_hilo_de_una_entrega_ajena_tambien_se_anota() {
        let e = base().await;
        sembrar(&e).await;
        sqlx::query(
            "INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) \
             VALUES (3,'Otro','o@x.co','h','OPERADOR',1)",
        )
        .execute(&e.bd).await.unwrap();
        let otro = Actor {
            id: 3,
            nombre: "Otro".into(),
            email: "o@x.co".into(),
            rol: Rol::Operador,
            stakeholder_id: None,
        };
        let anotadas = |e: Estado| async move {
            sqlx::query_scalar::<_, String>(
                "SELECT COALESCE(detalle,'') FROM auditoria \
                 WHERE evento = 'entrega_ajena_solicitada' ORDER BY id",
            )
            .fetch_all(&e.bd)
            .await
            .unwrap()
        };

        // El extremo JSON: la entrega 9 es del usuario 1.
        assert!(listar_comentarios(State(e.clone()), otro.clone(), RutaParam(9)).await.is_err());
        assert_eq!(
            anotadas(e.clone()).await,
            vec!["9".to_string()],
            "el hilo de una entrega ajena se leía sin dejar rastro: la vía más barata de \
             enumerar ids seguía muda"
        );

        // La inexistente NO se anota.
        assert!(listar_comentarios(State(e.clone()), otro.clone(), RutaParam(4242)).await.is_err());
        assert_eq!(anotadas(e.clone()).await, vec!["9".to_string()]);

        // El extremo PANTALLA, que es el que se recorre con el navegador.
        let _ = crate::vista::entrega_pagina(
            State(e.clone()),
            crate::vista::ActorDePanel(otro),
            axum::extract::Path(9),
        )
        .await;
        assert_eq!(
            anotadas(e.clone()).await,
            vec!["9".to_string(), "9".to_string()],
            "la pantalla de la entrega ajena no dejó línea"
        );

        // POSITIVO DE CONTROL: el DUEÑO abre su propia entrega y no se le anota
        // nada. El usuario 1 es de quien es la entrega 9.
        let duenno = Actor {
            id: 1,
            nombre: "Op".into(),
            email: "op@x.co".into(),
            rol: Rol::Operador,
            stakeholder_id: None,
        };
        let _ = crate::vista::entrega_pagina(
            State(e.clone()),
            crate::vista::ActorDePanel(duenno.clone()),
            axum::extract::Path(9),
        )
        .await;
        assert!(listar_comentarios(State(e.clone()), duenno, RutaParam(9)).await.is_ok());
        assert_eq!(
            anotadas(e).await,
            vec!["9".to_string(), "9".to_string()],
            "se le anotó sondeo al dueño de la entrega por abrir su propia pantalla"
        );
    }

    /// **El fragmento de fila contestaba 200 a quien no puede leer las partes.**
    ///
    /// `partes_fila` envolvía `usuarios_de`/`stakeholders_de` en
    /// `unwrap_or_default()`, así que su 403 se convertía en **200 con la fila
    /// vacía**: un permiso contestado al revés. No filtraba nada —las dos listas
    /// salían vacías— y por eso la auditoría del 2026-08-16 lo dejó como
    /// observación; se cierra igual, porque el día que ese fragmento lleve algo
    /// dentro nadie se acordará de mirar esa línea.
    ///
    /// Las dos mitades: los dos roles que NO pueden reciben 403, y los dos que SÍ
    /// siguen recibiendo su fragmento. Sin la segunda, esto lo pasaría igual una
    /// versión que rechazara a todo el mundo — y `Supervisor` alcanza la pantalla
    /// de partes, así que cerrarle el fragmento le rompería lo que sí compró.
    #[tokio::test]
    async fn el_fragmento_de_fila_no_se_sirve_a_quien_no_puede_leer_las_partes() {
        let e = base().await;
        let como = |rol| {
            crate::vista::ActorDePanel(Actor {
                id: 1,
                nombre: "X".into(),
                email: "x@x.co".into(),
                rol,
                stakeholder_id: None,
            })
        };

        for rol in [Rol::Operador, Rol::Tercero] {
            let r = crate::vista::partes_fila(State(e.clone()), como(rol)).await;
            assert_eq!(
                r.status(),
                StatusCode::FORBIDDEN,
                "{rol:?} no puede leer las partes y recibió una pantalla en vez de una negativa"
            );
        }

        for rol in [Rol::Admin, Rol::Supervisor] {
            let r = crate::vista::partes_fila(State(e.clone()), como(rol)).await;
            assert_eq!(
                r.status(),
                StatusCode::OK,
                "{rol:?} sí alcanza la pantalla de partes y se le cerró el fragmento"
            );
        }
    }

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

    /// Antes del 2026-08-10 esta prueba afirmaba `FORBIDDEN`, y ESE era el
    /// defecto: 403 para la entrega ajena que existe y 404 para la que no
    /// convierte el extremo en un oráculo de existencia — se enumeran ids
    /// probando números, sin llegar a ver nada. Ahora las dos respuestas son
    /// idénticas, cuerpo incluido.
    #[tokio::test]
    async fn un_operador_ajeno_no_distingue_la_entrega_que_existe_de_la_que_no() {
        let e = base().await;
        sembrar(&e).await;
        // Otro operador (id 3) que no es el responsable (id 1).
        sqlx::query("INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) VALUES (3,'Otro','o@x.co','h','OPERADOR',1)").execute(&e.bd).await.unwrap();
        let otro = Actor { id: 3, nombre: "Otro".into(), email: "o@x.co".into(), rol: Rol::Operador, stakeholder_id: None };

        let ajena = justificar(State(e.clone()), otro.clone(), RutaParam(9), Json(Justificacion { texto: "x".into() })).await.unwrap_err();
        let inexistente = justificar(State(e.clone()), otro, RutaParam(4242), Json(Justificacion { texto: "x".into() })).await.unwrap_err();

        assert_eq!(ajena.0, StatusCode::NOT_FOUND, "la entrega ajena que EXISTE tiene que dar 404");
        assert_eq!(inexistente.0, StatusCode::NOT_FOUND);
        assert_eq!(ajena.1, inexistente.1, "el cuerpo también tiene que ser idéntico: si difiere, el oráculo sigue ahí");
    }

    /// **La otra mitad, y es la que hace útil a la primera**: la respuesta no
    /// distingue, pero la BITÁCORA sí. Si las dos se anotaran igual, el 404
    /// uniforme habría cambiado un oráculo por una bitácora ciega; si no se
    /// anotara ninguna, se habría perdido la única señal que dice algo —alguien
    /// probando números ajenos—.
    #[tokio::test]
    async fn la_bitacora_si_distingue_la_ajena_de_la_inexistente() {
        let e = base().await;
        sembrar(&e).await;
        sqlx::query("INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) VALUES (3,'Otro','o@x.co','h','OPERADOR',1)").execute(&e.bd).await.unwrap();
        let otro = Actor { id: 3, nombre: "Otro".into(), email: "o@x.co".into(), rol: Rol::Operador, stakeholder_id: None };

        let _ = justificar(State(e.clone()), otro.clone(), RutaParam(9), Json(Justificacion { texto: "x".into() })).await;
        let _ = justificar(State(e.clone()), otro, RutaParam(4242), Json(Justificacion { texto: "x".into() })).await;

        let anotados: Vec<String> = sqlx::query_scalar(
            "SELECT COALESCE(detalle,'') FROM auditoria WHERE evento = 'entrega_ajena_solicitada' ORDER BY id",
        )
        .fetch_all(&e.bd)
        .await
        .unwrap();
        assert_eq!(anotados, vec!["9".to_string()], "se anota la ajena que existe, y SOLO esa");
    }

    /// El 403 no desaparece: sobrevive donde no filtra nada. Un supervisor ve
    /// todas las entregas (`Alcance::Todas`), así que decirle «esta no es suya»
    /// no le enseña ninguna existencia que no pudiera consultar ya.
    #[tokio::test]
    async fn al_supervisor_que_ya_la_ve_se_le_dice_403_y_no_404() {
        let e = base().await;
        sembrar(&e).await;
        let sup = Actor { id: 2, nombre: "Sup".into(), email: "sup@x.co".into(), rol: Rol::Supervisor, stakeholder_id: None };
        let err = justificar(State(e.clone()), sup, RutaParam(9), Json(Justificacion { texto: "x".into() })).await.unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }

    /// `Alcance::Ninguna` —un tercero sin entidad vinculada— es un valor
    /// legítimo, no un hueco: no ve nada, y por tanto tampoco distingue.
    #[tokio::test]
    async fn un_tercero_sin_entidad_tampoco_distingue() {
        let e = base().await;
        sembrar(&e).await;
        let suelto = Actor { id: 7, nombre: "T".into(), email: "t@x.co".into(), rol: Rol::Tercero, stakeholder_id: None };
        let err = justificar(State(e.clone()), suelto, RutaParam(9), Json(Justificacion { texto: "x".into() })).await.unwrap_err();
        assert_eq!(err.0, StatusCode::NOT_FOUND);
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
