//! Los extremos de la API.
//!
//! Regla del módulo, y no admite excepción: **ningún manejador acepta una ruta
//! de archivo, un identificador de usuario ni un filtro que venga del
//! navegador**. Recibe un id de recurso, vuelve a la base y comprueba
//! pertenencia con [`crate::autz::Alcance`]. Es la respuesta directa a los dos
//! agujeros que tenía el sistema anterior, y está escrita como regla y no como
//! parche: un extremo nuevo no puede saltársela porque no tiene de dónde sacar
//! la lista de lo que le corresponde.

use std::path::{Component, Path, PathBuf};

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    extract::{DefaultBodyLimit, FromRequestParts, Multipart, Path as RutaParam, State},
    http::{header, request::Parts, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{Datelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::autz::{Actor, Alcance, Rol};
use crate::config::Config;
use crate::sesion;

#[derive(Clone)]
pub struct Estado {
    pub bd: SqlitePool,
    pub cfg: Config,
    /// El mismo motor que genera las tareas. Va en el estado para que la pantalla
    /// del cronograma calcule las fechas CON ÉL y no con una reimplementación: una
    /// previsualización que no sale del mismo cálculo es una promesa que después no
    /// se cumple. `Arc` porque `Motor` no es `Clone` y axum exige que el estado lo
    /// sea.
    pub motor: std::sync::Arc<siger_core::Motor>,
}

pub fn router(estado: Estado) -> Router {
    let tope_mb = estado.cfg.max_mb;
    Router::new()
        .route("/api/salud", get(salud))
        .route("/api/sesion", post(abrir_sesion).delete(cerrar_sesion))
        .route("/api/yo", get(yo))
        .route("/api/entregas", get(listar_entregas))
        .route("/api/entregas/{id}/verificar", get(verificar_entrega))
        .route(
            "/api/entregas/{id}/archivo",
            get(descargar_archivo)
                .post(subir_archivo)
                // El tope del cuerpo se aplica AQUÍ, no en una comprobación
                // después de leerlo: un archivo de 2 GB que se rechaza tras
                // cargarlo en memoria ya hizo el daño. Axum corta la petición al
                // pasarse, y el cliente recibe 413.
                .layer(DefaultBodyLimit::max(tope_mb * 1024 * 1024)),
        )
        .route("/api/periodicidad/previsualizar", post(previsualizar))
        .route("/api/planificador/generar", post(generar_ahora))
        .route(
            "/api/cronograma/lecturas",
            post(crate::informes::lecturas),
        )
        .route(
            "/api/informes",
            get(crate::informes::listar).post(crate::informes::crear),
        )
        .route(
            "/api/informes/{id}",
            get(crate::informes::detalle).put(crate::informes::actualizar),
        )
        .route(
            "/api/informes/{id}/participacion",
            get(crate::participacion::listar).put(crate::participacion::reemplazar),
        )
        .route("/api/usuarios", get(crate::participacion::usuarios))
        .route("/api/stakeholders", get(crate::participacion::stakeholders))
        .route("/api/estadisticas", get(crate::estadisticas::resumen))
        .route("/api/notificaciones", get(crate::flujo::listar_notificaciones))
        .route("/api/notificaciones/{id}/leida", post(crate::flujo::marcar_leida))
        .route(
            "/api/entregas/{id}/comentarios",
            get(crate::flujo::listar_comentarios),
        )
        .route("/api/entregas/{id}/justificacion", post(crate::flujo::justificar))
        .route("/api/entregas/{id}/revision", post(crate::flujo::revisar))
        .with_state(estado)
}

// --- Errores -----------------------------------------------------------------

#[derive(Debug)]
pub struct Error(pub StatusCode, pub String);

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        // El detalle va al registro, no al cliente: un mensaje de la base
        // describe el esquema a quien esté probando.
        tracing::error!("fallo de base de datos: {e}");
        Error(StatusCode::INTERNAL_SERVER_ERROR, "error interno".into())
    }
}

// --- El extractor: quién pide -------------------------------------------------

impl FromRequestParts<Estado> for Actor {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, estado: &Estado) -> Result<Self, Self::Rejection> {
        let no_autorizado = || Error(StatusCode::UNAUTHORIZED, "sesión no válida".into());

        let cookie = sesion::de_cabeceras(&parts.headers).ok_or_else(no_autorizado)?;
        let s = sesion::verificar(&estado.cfg.secreto_sesion, &cookie).ok_or_else(no_autorizado)?;

        // El rol se lee de la BASE en cada petición, no de la cookie: dentro de
        // la cookie sobreviviría a que un administrador lo revoque, y entonces
        // revocarlo no revocaría nada.
        let fila = sqlx::query_as::<_, (i64, String, String, String, i64, Option<i64>)>(
            "SELECT id, nombre, email, rol, activo, stakeholder_id FROM usuarios WHERE id = ?",
        )
        .bind(s.usuario_id)
        .fetch_optional(&estado.bd)
        .await?
        .ok_or_else(no_autorizado)?;

        if fila.4 == 0 {
            return Err(no_autorizado());
        }
        let rol = Rol::desde_bd(&fila.3).ok_or_else(no_autorizado)?;

        Ok(Actor { id: fila.0, nombre: fila.1, email: fila.2, rol, stakeholder_id: fila.5 })
    }
}

// --- Sesión -------------------------------------------------------------------

#[derive(Deserialize)]
pub struct Credenciales {
    email: String,
    clave: String,
}

async fn abrir_sesion(
    State(e): State<Estado>,
    Json(cred): Json<Credenciales>,
) -> Result<Response, Error> {
    let fila = sqlx::query_as::<_, (i64, String, i64)>(
        "SELECT id, clave_hash, activo FROM usuarios WHERE email = ?",
    )
    .bind(cred.email.trim().to_lowercase())
    .fetch_optional(&e.bd)
    .await?;

    // Mismo mensaje y mismo trabajo para «no existe» y «clave incorrecta»: la
    // diferencia le dice a quien prueba qué correos son de verdad.
    let rechazo = Error(StatusCode::UNAUTHORIZED, "credenciales incorrectas".into());
    let Some((id, hash, activo)) = fila else {
        return Err(rechazo);
    };
    if activo == 0 {
        return Err(rechazo);
    }
    let Ok(analizado) = PasswordHash::new(&hash) else {
        // Un hash que ningún esquema reconoce no deja entrar, pero se registra:
        // significa que alguien quedó fuera del sistema.
        tracing::error!("el hash del usuario {id} no lo reconoce ningún esquema");
        return Err(rechazo);
    };
    if Argon2::default().verify_password(cred.clave.as_bytes(), &analizado).is_err() {
        auditar(&e.bd, "acceso_fallido", Some(&cred.email), None).await;
        return Err(rechazo);
    }

    auditar(&e.bd, "acceso", Some(&cred.email), None).await;
    let valor = sesion::emitir(&e.cfg.secreto_sesion, id, e.cfg.minutos_sesion);
    let mut cabeceras = HeaderMap::new();
    cabeceras.insert(
        header::SET_COOKIE,
        sesion::cabecera_abrir(&valor, e.cfg.minutos_sesion)
            .parse()
            .expect("la cabecera es ASCII"),
    );
    Ok((cabeceras, Json(serde_json::json!({ "ok": true }))).into_response())
}

async fn cerrar_sesion(State(e): State<Estado>, actor: Actor) -> Result<Response, Error> {
    auditar(&e.bd, "salida", Some(&actor.email), None).await;
    let mut cabeceras = HeaderMap::new();
    cabeceras.insert(
        header::SET_COOKIE,
        sesion::cabecera_cerrar().parse().expect("la cabecera es ASCII"),
    );
    Ok((cabeceras, Json(serde_json::json!({ "ok": true }))).into_response())
}

async fn yo(actor: Actor) -> Json<Actor> {
    Json(actor)
}

async fn salud(State(e): State<Estado>) -> Response {
    match sqlx::query("SELECT 1").fetch_one(&e.bd).await {
        Ok(_) => Json(serde_json::json!({ "estado": "sano", "bd": "ok" })).into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "estado": "degradado", "bd": "error" })),
        )
            .into_response(),
    }
}

// --- Entregas -----------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct EntregaVista {
    id: i64,
    informe: String,
    fecha_nominal: String,
    fecha_limite: String,
    tipo_ajuste: String,
    estado: String,
    /// Solo el nombre. La ruta no sale nunca de aquí: al navegador no le hace
    /// falta y dársela fue exactamente el fallo anterior.
    archivo_nombre: Option<String>,
    tiene_firma: bool,
    /// Factor de calificación por contenido (#165): `Some(true)` el contenido
    /// concuerda con la extensión, `Some(false)` NO concuerda —marcado para
    /// revisión—, `None` sin archivo o tipo no determinable.
    archivo_valido: Option<bool>,
}

/// El `WHERE` sale del alcance del actor, no de la petición. No hay parámetro
/// `usuario_id` que un cliente pueda mandar: no existe.
fn consulta_por_alcance(alcance: &Alcance) -> (String, Option<i64>) {
    const COLUMNAS: &str = "SELECT e.id, i.nombre AS informe, e.fecha_nominal, e.fecha_limite, \
         e.tipo_ajuste, e.estado, e.archivo_nombre, (e.firma IS NOT NULL) AS tiene_firma, \
         e.archivo_valido \
         FROM entregas e JOIN informes i ON i.id = e.informe_id ";
    const ORDEN: &str = " ORDER BY e.fecha_limite";

    match alcance {
        Alcance::Todas => (format!("{COLUMNAS}{ORDEN}"), None),
        Alcance::DeUsuario(id) => {
            (format!("{COLUMNAS} WHERE e.usuario_id = ?{ORDEN}"), Some(*id))
        }
        Alcance::DeStakeholder(id) => (
            format!(
                "{COLUMNAS} WHERE EXISTS (SELECT 1 FROM participacion p \
                 WHERE p.informe_id = i.id AND p.stakeholder_id = ?){ORDEN}"
            ),
            Some(*id),
        ),
        // Una consulta que no puede devolver filas, en vez de un `Option` que
        // alguien desenvuelva con un `unwrap_or(todas)`.
        Alcance::Ninguna => (format!("{COLUMNAS} WHERE 0{ORDEN}"), None),
    }
}

async fn listar_entregas(
    State(e): State<Estado>,
    actor: Actor,
) -> Result<Json<Vec<EntregaVista>>, Error> {
    let (sql, param) = consulta_por_alcance(&actor.alcance());
    let sql = sql.as_str();
    let mut q = sqlx::query_as::<_, EntregaVista>(sql);
    if let Some(p) = param {
        q = q.bind(p);
    }
    Ok(Json(q.fetch_all(&e.bd).await?))
}

/// Descarga del archivo de una entrega.
///
/// **Aquí vivía el agujero.** El portal anterior abría la ruta que le mandaba el
/// navegador, así que un tercero externo podía pedir `.env` —con las claves de
/// correo, el secreto de sesión y la clave de firma— o la base entera. Ahora:
///
///   1. llega un id, no una ruta;
///   2. se comprueba que ese id está dentro del alcance del actor;
///   3. la ruta la construye el servidor desde el id y el nombre guardado;
///   4. y aun así se verifica que el resultado cae dentro del almacén, porque
///      una defensa que depende de que los tres pasos anteriores estén bien es
///      una sola defensa.
async fn descargar_archivo(
    State(e): State<Estado>,
    actor: Actor,
    RutaParam(id): RutaParam<i64>,
) -> Result<Response, Error> {
    let no_existe = || Error(StatusCode::NOT_FOUND, "la entrega no existe".into());

    // El alcance entra en la MISMA consulta: no se lee primero y se comprueba
    // después, porque entre las dos cosas cabe un olvido.
    const COLS: &str = "archivo_nombre, archivo_cifrado";
    let (sql, param) = match actor.alcance() {
        Alcance::Todas => (format!("SELECT {COLS} FROM entregas WHERE id = ?"), None),
        Alcance::DeUsuario(u) => (
            format!("SELECT {COLS} FROM entregas WHERE id = ? AND usuario_id = ?"),
            Some(u),
        ),
        Alcance::DeStakeholder(s) => (
            format!(
                "SELECT {COLS} FROM entregas e WHERE e.id = ? AND EXISTS \
                 (SELECT 1 FROM participacion p WHERE p.informe_id = e.informe_id \
                  AND p.stakeholder_id = ?)"
            ),
            Some(s),
        ),
        Alcance::Ninguna => return Err(no_existe()),
    };

    let mut q = sqlx::query_as::<_, (Option<String>, i64)>(&sql).bind(id);
    if let Some(p) = param {
        q = q.bind(p);
    }
    let (nombre, cifrado) = q.fetch_optional(&e.bd).await?.ok_or_else(no_existe)?;
    let nombre = nombre.ok_or_else(no_existe)?;

    let ruta = ruta_de_entrega(&e.cfg.dir_entregas, id, &nombre).ok_or_else(|| {
        tracing::error!("nombre de archivo inaceptable en la entrega {id}: {nombre:?}");
        Error(StatusCode::INTERNAL_SERVER_ERROR, "error interno".into())
    })?;

    let datos = match leer_en_claro(&ruta, cifrado != 0, e.cfg.clave_reposo.as_deref()).await {
        Documento::Claro(d) => d,
        // Que no esté es un 404 esté cifrado o no: el estado del despliegue no
        // cambia qué le pasó al archivo.
        Documento::NoEsta => return Err(no_existe()),
        Documento::NoDescifra => {
            tracing::error!(
                "entrega {id}: no se pudo descifrar el archivo (¿falta SIGER_CLAVE_REPOSO, está rotada, o el blob fue alterado?)"
            );
            return Err(Error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "no se pudo leer el archivo".into(),
            ));
        }
    };
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", nombre.replace('"', "")),
            ),
        ],
        datos,
    )
        .into_response())
}

/// Verifica la firma de una entrega. Reconstruye lo que se firmó desde la base y
/// el archivo actual, y comprueba la firma. Distingue firma inválida/forjada de
/// entrega alterada tras firmarse — que es justo lo que un ente de control
/// pregunta. Es una lectura: cualquiera que pueda ver la entrega puede auditarla.
async fn verificar_entrega(
    State(e): State<Estado>,
    actor: Actor,
    RutaParam(id): RutaParam<i64>,
) -> Result<Json<serde_json::Value>, Error> {
    let no_existe = || Error(StatusCode::NOT_FOUND, "la entrega no existe".into());

    const COLS: &str = "fecha_limite, informe_id, usuario_id, fecha_nominal, tipo_ajuste, \
                        estado, fecha_entrega_real, archivo_nombre, firma, archivo_cifrado";
    let (sql, param) = match actor.alcance() {
        Alcance::Todas => (format!("SELECT {COLS} FROM entregas e WHERE e.id = ?"), None),
        Alcance::DeUsuario(u) => (
            format!("SELECT {COLS} FROM entregas e WHERE e.id = ? AND e.usuario_id = ?"),
            Some(u),
        ),
        Alcance::DeStakeholder(s) => (
            format!(
                "SELECT {COLS} FROM entregas e WHERE e.id = ? AND EXISTS \
                 (SELECT 1 FROM participacion p WHERE p.informe_id = e.informe_id \
                  AND p.stakeholder_id = ?)"
            ),
            Some(s),
        ),
        Alcance::Ninguna => return Err(no_existe()),
    };

    type Fila = (
        String,         // fecha_limite
        i64,            // informe_id
        Option<i64>,    // usuario_id
        String,         // fecha_nominal
        String,         // tipo_ajuste
        String,         // estado
        Option<String>, // fecha_entrega_real
        Option<String>, // archivo_nombre
        Option<String>, // firma
        i64,            // archivo_cifrado
    );
    let mut q = sqlx::query_as::<_, Fila>(&sql).bind(id);
    if let Some(p) = param {
        q = q.bind(p);
    }
    let (limite, informe_id, usuario_id, fecha_nominal, tipo_ajuste, estado, fecha_entrega, archivo_nombre, firma, cifrado) =
        q.fetch_optional(&e.bd).await?.ok_or_else(no_existe)?;

    let Some(firma) = firma else {
        return Ok(Json(serde_json::json!({ "verificada": false, "motivo": "sin firma" })));
    };

    // El sha256 se recalcula del archivo AHORA: si el documento cambió tras
    // firmarse, el hash difiere y la verificación lo delata como alteración. Y si
    // no se pudo leer, NO se sigue: no poder mirar no es haber visto algo.
    let sha = match sha_del_documento(&e.cfg, id, archivo_nombre.as_deref(), cifrado != 0).await {
        Ok(sha) => sha,
        Err(motivo) => return Ok(Json(serde_json::json!({ "verificada": false, "motivo": motivo }))),
    };

    let (ok, motivo) = crate::firma::verificar(
        &crate::firma::Auditable {
            id,
            informe_id,
            usuario_id,
            fecha_limite: &limite,
            fecha_entrega: fecha_entrega.as_deref(),
            estado: &estado,
            archivo_sha256: sha.as_deref(),
            fecha_vencimiento_nominal: &fecha_nominal,
            tipo_ajuste: &tipo_ajuste,
        },
        &firma,
        e.cfg.firma_vk_hex.as_deref(),
    );
    Ok(Json(serde_json::json!({ "verificada": ok, "motivo": motivo })))
}

/// `<almacén>/<id>/<nombre>`, y solo si el resultado sigue dentro del almacén.
///
/// Un nombre con `..`, con separadores o con raíz absoluta se rechaza en vez de
/// sanearse: sanear deja abierta la pregunta de si el saneo fue completo, y esa
/// pregunta ya se contestó mal una vez.
pub fn ruta_de_entrega(almacen: &str, id: i64, nombre: &str) -> Option<PathBuf> {
    let limpio = Path::new(nombre);
    let unico = match limpio.components().collect::<Vec<_>>().as_slice() {
        [Component::Normal(n)] => n.to_owned(),
        _ => return None,
    };
    let ruta = Path::new(almacen).join(id.to_string()).join(unico);
    if ruta.starts_with(almacen) {
        Some(ruta)
    } else {
        None
    }
}

/// El SHA-256 del documento EN CLARO tal como está ahora, para contrastarlo con
/// el que se firmó. `Err(motivo)` cuando no se puede saber.
///
/// La regla, y es la que evita una acusación falsa: **`Ok(None)` solo significa
/// «esta entrega no tiene archivo»**, un hecho que la firma acredita igual que
/// cualquier otro. Todo lo demás —el archivo que no está donde se guardó, el blob
/// que no descifra, un nombre que la base no debería contener— es *no se puede
/// saber*, y sale por `Err`. Antes todos caían en el mismo `None`: el canónico
/// local quedaba con `"archivo_sha256":null`, no cuadraba con el firmado, y
/// `verificar` respondía «la entrega fue alterada tras la firma» a una entrega
/// intacta cuyo despliegue había perdido `SIGER_CLAVE_REPOSO` o tenía
/// `SIGER_DIR_ENTREGAS` mal puesto. Es exactamente la acusación que este producto
/// existe para hacer bien (directivas 20 y 23).
async fn sha_del_documento(
    cfg: &Config,
    id: i64,
    nombre: Option<&str>,
    cifrado: bool,
) -> Result<Option<String>, &'static str> {
    let Some(nombre) = nombre else {
        return Ok(None); // la entrega no trae archivo: eso es un hecho, no un hueco
    };
    let Some(ruta) = ruta_de_entrega(&cfg.dir_entregas, id, nombre) else {
        tracing::error!("nombre de archivo inaceptable en la entrega {id}: {nombre:?}");
        return Err("no se pudo leer el archivo de la entrega");
    };
    // Se descifra ANTES de hashear: el SHA firmado es el del documento en claro,
    // no el del blob.
    match leer_en_claro(&ruta, cifrado, cfg.clave_reposo.as_deref()).await {
        Documento::Claro(d) => Ok(Some(crate::firma::sha256_hex(&d))),
        Documento::NoEsta => {
            tracing::error!("entrega {id}: el archivo firmado no está en el almacén");
            Err("el archivo de la entrega no está en el almacén")
        }
        Documento::NoDescifra => {
            tracing::error!(
                "entrega {id}: no se pudo descifrar el archivo para verificar la firma (¿falta SIGER_CLAVE_REPOSO, está rotada, o el blob fue alterado?)"
            );
            Err("no se pudo descifrar el archivo de la entrega")
        }
    }
}

/// Lo que se obtuvo al intentar leer el documento de una entrega.
///
/// Los tres casos NO son el mismo hecho, y colapsarlos en un `None` fue el origen
/// de una acusación falsa: quien no distingue «no está» de «no descifra» acaba
/// diciéndole a un ente de control que hubo manipulación donde solo faltó un
/// `export SIGER_CLAVE_REPOSO`.
#[derive(Debug, PartialEq, Eq)]
enum Documento {
    /// El documento, en claro.
    Claro(Vec<u8>),
    /// No hay archivo en esa ruta.
    NoEsta,
    /// El archivo está y la fila lo declara cifrado, pero no se pudo descifrar:
    /// falta la frase de reposo, está rotada, o el blob fue alterado.
    NoDescifra,
}

/// Lee el archivo de una entrega y lo devuelve EN CLARO. Si la fila dice que está
/// cifrado, lo descifra con la frase de reposo. **Nunca** se sirve un blob
/// cifrado como si fuera el documento (directiva 20: ante la ausencia de la clave
/// se falla, no se entrega basura).
async fn leer_en_claro(ruta: &Path, cifrado: bool, clave: Option<&str>) -> Documento {
    let Ok(crudo) = tokio::fs::read(ruta).await else {
        return Documento::NoEsta;
    };
    if !cifrado {
        return Documento::Claro(crudo);
    }
    match clave.and_then(|c| guaca::reposo::descifrar(&crudo, c).ok()) {
        Some(claro) => Documento::Claro(claro),
        None => Documento::NoDescifra,
    }
}

// --- Subida -------------------------------------------------------------------

/// La extensión que declara el nombre del cliente, si es aceptable.
///
/// **Del nombre que manda el navegador se toma ESTO y nada más.** El nombre
/// completo no se usa ni saneado: sanear deja abierta la pregunta de si el saneo
/// fue completo, y en la versión anterior esa pregunta se contestó mal —
/// `os.path.join(carpeta, nombre_del_cliente)` con un `../` sale del almacén.
/// Construir el nombre en el servidor no deja pregunta que contestar.
fn extension_de(nombre: &str, permitidas: &[String]) -> Option<String> {
    let ext = nombre.rsplit_once('.')?.1.to_lowercase();
    // Una «extensión» con separadores, puntos o rarezas no es una extensión.
    if ext.is_empty() || ext.len() > 8 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let con_punto = format!(".{ext}");
    // Lista vacía significa «cualquiera», y hay que escribirla explícitamente.
    if permitidas.is_empty() || permitidas.contains(&con_punto) {
        Some(ext)
    } else {
        None
    }
}

/// Verifica que el CONTENIDO del archivo concuerda con la extensión declarada,
/// por sus magic bytes (#165). Es un FACTOR de calificación, no una compuerta:
///
///   - `Some(true)`: el tipo detectado concuerda con la extensión.
///   - `Some(false)`: se detectó un tipo y es OTRO — se marca para revisión.
///   - `None`: el tipo no es determinable (CSV, XML, TXT no tienen firma), así
///     que no se afirma ni se niega — no se inventa un veredicto que no se tiene.
fn verifica_contenido(ext: &str, datos: &[u8]) -> Option<bool> {
    let detectada = infer::get(datos)?.extension();
    Some(concuerda(ext, detectada))
}

/// ¿La extensión declarada es compatible con el tipo detectado? Lenient a
/// propósito, para NO marcar archivos legítimos: los ofimáticos abiertos son ZIP
/// por dentro y `infer` puede verlos como `zip` o como su tipo específico, y
/// `jpg`/`jpeg` son el mismo. Solo se marca un desajuste CLARO.
fn concuerda(declarada: &str, detectada: &str) -> bool {
    if declarada == detectada {
        return true;
    }
    const OFIMATICA: &[&str] = &["docx", "xlsx", "pptx", "zip"];
    if OFIMATICA.contains(&declarada) && OFIMATICA.contains(&detectada) {
        return true;
    }
    const JPG: &[&str] = &["jpg", "jpeg"];
    if JPG.contains(&declarada) && JPG.contains(&detectada) {
        return true;
    }
    false
}

/// Registrar la entrega subiendo su archivo.
///
/// Lo que hace distinto a lo anterior:
///
///   - el **nombre lo construye el servidor** desde el id de la entrega;
///   - la **extensión** se compara con las admitidas, y la lista vacía hay que
///     pedirla;
///   - el **tamaño** lo corta el límite del cuerpo, antes de leer nada;
///   - la **pertenencia** se comprueba con el alcance del actor, en la consulta;
///   - el **estado** sale de comparar con la fecha límite, no de lo que diga el
///     cliente: CUMPLIDO o EXTEMPORÁNEO no son opciones de un formulario.
async fn subir_archivo(
    State(e): State<Estado>,
    actor: Actor,
    RutaParam(id): RutaParam<i64>,
    mut partes: Multipart,
) -> Result<Json<serde_json::Value>, Error> {
    let no_existe = || Error(StatusCode::NOT_FOUND, "la entrega no existe".into());

    // Pertenencia primero: si no le corresponde, no se lee ni un byte. Se traen
    // de una vez todos los campos que la firma necesita: el nominal, el informe y
    // el ajuste son parte de lo que se acredita, no solo la fecha límite.
    const COLS: &str = "fecha_limite, informe_id, usuario_id, fecha_nominal, tipo_ajuste";
    let (sql, param) = match actor.alcance() {
        Alcance::Todas => (format!("SELECT {COLS} FROM entregas WHERE id = ?"), None),
        Alcance::DeUsuario(u) => (
            format!("SELECT {COLS} FROM entregas WHERE id = ? AND usuario_id = ?"),
            Some(u),
        ),
        // Un tercero no entrega: recibe. Darle la subida porque «también ve la
        // entrega» confundiría ver con hacer.
        Alcance::DeStakeholder(_) | Alcance::Ninguna => return Err(no_existe()),
    };
    let mut q =
        sqlx::query_as::<_, (String, i64, Option<i64>, String, String)>(&sql).bind(id);
    if let Some(p) = param {
        q = q.bind(p);
    }
    let (limite, informe_id, usuario_id, fecha_nominal, tipo_ajuste) =
        q.fetch_optional(&e.bd).await?.ok_or_else(no_existe)?;

    let mal = |m: &str| Error(StatusCode::BAD_REQUEST, m.to_string());
    let Some(parte) = partes
        .next_field()
        .await
        .map_err(|_| mal("no se pudo leer el archivo"))?
    else {
        return Err(mal("la petición no traía ningún archivo"));
    };

    let nombre_cliente = parte.file_name().unwrap_or_default().to_string();
    let Some(ext) = extension_de(&nombre_cliente, &e.cfg.extensiones) else {
        return Err(mal(&format!(
            "formato no admitido. Se aceptan: {}",
            if e.cfg.extensiones.is_empty() { "cualquiera".into() } else { e.cfg.extensiones.join(", ") }
        )));
    };

    let datos = parte.bytes().await.map_err(|_| {
        // Aquí cae también el archivo que se pasó del tope: axum corta el flujo.
        Error(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("el archivo supera el máximo de {} MB", e.cfg.max_mb),
        )
    })?;
    if datos.is_empty() {
        return Err(mal("el archivo está vacío"));
    }

    // El nombre lo pone el servidor. Lleva el id y el instante para que dos
    // envíos de la misma entrega no se pisen y quede rastro de cuál es cuál.
    let nombre = format!("entrega-{id}-{}.{ext}", Utc::now().format("%Y%m%d-%H%M%S"));
    let ruta = ruta_de_entrega(&e.cfg.dir_entregas, id, &nombre)
        .ok_or_else(|| Error(StatusCode::INTERNAL_SERVER_ERROR, "error interno".into()))?;

    if let Some(carpeta) = ruta.parent() {
        tokio::fs::create_dir_all(carpeta).await.map_err(|err| {
            tracing::error!("no se pudo crear {}: {err}", carpeta.display());
            Error(StatusCode::INTERNAL_SERVER_ERROR, "no se pudo guardar el archivo".into())
        })?;
    }
    // Cifrado EN REPOSO (A1): con frase, al disco va el BLOB cifrado; sin frase,
    // el archivo en claro, como antes. El SHA que se firma se calcula MÁS ABAJO
    // sobre `datos` (el documento en claro), nunca sobre esto: la firma acredita
    // el contenido, no cómo se guarda.
    let (para_disco, cifrado): (Vec<u8>, i64) = match e.cfg.clave_reposo.as_deref() {
        Some(frase) => (guaca::reposo::cifrar(&datos, frase), 1),
        None => (datos.to_vec(), 0),
    };
    tokio::fs::write(&ruta, &para_disco).await.map_err(|err| {
        tracing::error!("no se pudo escribir {}: {err}", ruta.display());
        Error(StatusCode::INTERNAL_SERVER_ERROR, "no se pudo guardar el archivo".into())
    })?;

    // CUMPLIDO o EXTEMPORÁNEO lo decide la fecha, no quien entrega.
    let hoy = Utc::now().date_naive();
    let vencida = chrono::NaiveDate::parse_from_str(&limite, "%Y-%m-%d")
        .map(|l| hoy > l)
        .unwrap_or(false);
    let estado = if vencida { "EXTEMPORANEO" } else { "CUMPLIDO" };

    // El contenido se verifica contra la extensión (#165). Es un FACTOR: no
    // impide registrar la entrega —un archivo raro no puede causar el
    // incumplimiento que pretende evitar—, pero si no concuerda queda marcado
    // para que un supervisor lo revise.
    let archivo_valido = verifica_contenido(&ext, &datos);

    // La firma va DENTRO de esta misma sentencia (una sola UPDATE es atómica),
    // por la razón de siempre: una segunda pasada puede no llegar a ocurrir y
    // dejaría entregas firmables sin firmar sin que nadie lo note. Se firma lo
    // que se GUARDA —el mismo estado, la misma fecha de entrega y el hash del
    // archivo escrito—, no una versión aparte que podría no coincidir.
    let sha = crate::firma::sha256_hex(&datos);
    let entrega_real = Utc::now().to_rfc3339();
    let firma_blob = crate::firma::firmar(
        &crate::firma::Auditable {
            id,
            informe_id,
            usuario_id,
            fecha_limite: &limite,
            fecha_entrega: Some(&entrega_real),
            estado,
            archivo_sha256: Some(&sha),
            fecha_vencimiento_nominal: &fecha_nominal,
            tipo_ajuste: &tipo_ajuste,
        },
        e.cfg.firma_sk_hex.as_deref(),
    );

    let filas = sqlx::query(
        "UPDATE entregas SET archivo_nombre = ?, estado = ?, fecha_entrega_real = ?, firma = ?, \
         archivo_valido = ?, archivo_cifrado = ? WHERE id = ?",
    )
    .bind(&nombre)
    .bind(estado)
    .bind(&entrega_real)
    .bind(&firma_blob)
    .bind(archivo_valido)
    .bind(cifrado)
    .bind(id)
    .execute(&e.bd)
    .await?
    .rows_affected();

    if filas == 0 {
        // La entrega desapareció entre la comprobación y ahora. El archivo
        // escrito se retira: dejarlo sería basura que nadie va a reclamar.
        let _ = tokio::fs::remove_file(&ruta).await;
        return Err(no_existe());
    }

    if archivo_valido == Some(false) {
        auditar(&e.bd, "entrega_contenido_no_concuerda", Some(&actor.email), Some(&nombre)).await;
    }
    auditar(&e.bd, "entrega_registrada", Some(&actor.email), Some(&nombre)).await;
    Ok(Json(serde_json::json!({
        "ok": true, "estado": estado, "archivo": nombre, "bytes": datos.len(),
        "firmada": firma_blob.is_some(),
        // El front avisa cuando el contenido no concuerda con la extensión, sin
        // rechazar la entrega: quedó registrada y marcada para revisión.
        "archivo_valido": archivo_valido
    })))
}

// --- Planificador bajo demanda ------------------------------------------------

/// Generar las entregas ahora, sin esperar al reloj.
///
/// Solo administración: es la orden que se da tras crear o corregir una regla, y
/// no tendría sentido que quien no puede tocar reglas pudiera dispararla.
async fn generar_ahora(State(e): State<Estado>, actor: Actor) -> Result<Response, Error> {
    if !matches!(actor.rol, Rol::Admin) {
        return Err(Error(StatusCode::FORBIDDEN, "solo administración".into()));
    }
    let calendario = siger_core::Calendario::de_pais(&e.cfg.pais_iso, 2000, 2100)
        .map_err(|err| Error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let motor = siger_core::Motor::nuevo(calendario);
    let r = crate::planificador::generar(&e.bd, &motor, Utc::now().date_naive()).await?;
    Ok(Json(serde_json::json!({
        "informes_revisados": r.informes_revisados,
        "entregas_creadas": r.entregas_creadas,
        // Las reglas que no se pudieron leer VIAJAN en la respuesta. Que el
        // resumen diga «0 creadas» sin decir que tres reglas están rotas es la
        // clase de silencio que este sistema persigue.
        "reglas_ilegibles": r.reglas_ilegibles.iter().map(|(id, m)| serde_json::json!({"informe": id, "motivo": m})).collect::<Vec<_>>(),
    }))
    .into_response())
}

// --- Vista previa de una regla ------------------------------------------------

#[derive(Deserialize)]
pub struct PeticionPrevia {
    regla: siger_core::ReglaJson,
    desde: chrono::NaiveDate,
    hasta: chrono::NaiveDate,
}

#[derive(Serialize)]
pub struct FilaPrevia {
    nominal: chrono::NaiveDate,
    limite: chrono::NaiveDate,
    ajuste: String,
    /// Para pintar en la interfaz por qué las dos fechas difieren sin que quien
    /// mira tenga que deducirlo comparándolas.
    se_movio: bool,
}

/// Qué vencimientos genera una regla, **antes** de guardarla.
///
/// Usa el motor de producción, no una copia: una vista previa con su propia
/// lógica mostraría fechas que el sistema luego no genera, y nadie sabría cuál
/// de las dos está mal. Es la misma razón por la que el motor vive en un crate
/// aparte y sin E/S — para que la interfaz pueda preguntarle sin arrastrar media
/// aplicación.
async fn previsualizar(
    State(e): State<Estado>,
    _actor: Actor,
    Json(p): Json<PeticionPrevia>,
) -> Result<Json<Vec<FilaPrevia>>, Error> {
    let regla = siger_core::Regla::try_from(&p.regla)
        .map_err(|err| Error(StatusCode::BAD_REQUEST, err.to_string()))?;

    // Un rango abierto pondría al servidor a generar años de fechas porque
    // alguien escribió mal una cifra. El tope se dice, no se adivina.
    const TOPE_DIAS: i64 = 366 * 5;
    if (p.hasta - p.desde).num_days() > TOPE_DIAS || p.hasta < p.desde {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            format!("el rango debe ir de menos a más y no pasar de {TOPE_DIAS} días"),
        ));
    }

    // Un año de margen a cada lado: el ajuste puede llevar el día límite al año
    // vecino, y un calendario que no lo cubra lo dejaría sin festivos.
    let calendario = siger_core::Calendario::de_pais(
        &e.cfg.pais_iso,
        p.desde.year() - 1,
        p.hasta.year() + 1,
    )
    .map_err(|err| Error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let motor = siger_core::Motor::nuevo(calendario);

    Ok(Json(
        motor
            .vencimientos_entre(p.desde, p.hasta, &regla)
            .into_iter()
            .map(|v| FilaPrevia {
                nominal: v.nominal.fecha(),
                limite: v.limite.fecha(),
                ajuste: format!("{:?}", v.ajuste).to_uppercase(),
                se_movio: v.se_movio(),
            })
            .collect(),
    ))
}

pub async fn auditar(bd: &SqlitePool, evento: &str, email: Option<&str>, detalle: Option<&str>) {
    // Que falle el registro no puede tumbar la operación, pero tampoco puede
    // pasar en silencio: se anota en el log del proceso.
    if let Err(e) = sqlx::query("INSERT INTO auditoria (momento, evento, email, detalle) VALUES (?, ?, ?, ?)")
        .bind(Utc::now().to_rfc3339())
        .bind(evento)
        .bind(email)
        .bind(detalle)
        .execute(bd)
        .await
    {
        tracing::error!("no se pudo registrar el evento {evento}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_nombre_con_travesia_no_produce_ruta() {
        // El caso exacto del sistema anterior: el nombre lo pone quien sube.
        assert!(ruta_de_entrega("storage/entregas", 7, "../../../.env").is_none());
        assert!(ruta_de_entrega("storage/entregas", 7, "/etc/passwd").is_none());
        assert!(ruta_de_entrega("storage/entregas", 7, "sub/dir/x.pdf").is_none());
        assert!(ruta_de_entrega("storage/entregas", 7, "..").is_none());
    }

    #[test]
    fn un_nombre_normal_cae_dentro_del_almacen() {
        let r = ruta_de_entrega("storage/entregas", 7, "informe.pdf").unwrap();
        assert_eq!(r, Path::new("storage/entregas/7/informe.pdf"));
    }

    #[test]
    fn la_extension_es_el_segundo_limite_de_la_subida() {
        // El límite de formato de #158: del nombre del cliente se toma SOLO la
        // extensión, en minúsculas, y solo si está admitida.
        let solo_pdf = vec![".pdf".to_string()];
        assert_eq!(extension_de("informe.PDF", &solo_pdf).as_deref(), Some("pdf"));
        assert_eq!(extension_de("virus.exe", &solo_pdf), None);
        // La extensión es lo que va tras el ÚLTIMO punto: `.tar.gz` es `gz`.
        assert_eq!(extension_de("copia.tar.gz", &[".gz".to_string()]).as_deref(), Some("gz"));
        // Lista vacía = cualquiera, pero hay que escribirla; y sin punto no hay
        // extensión que tomar.
        assert_eq!(extension_de("cualquiera.csv", &[]).as_deref(), Some("csv"));
        assert_eq!(extension_de("sinextension", &[]), None);
        // Una «extensión» con separadores, espacios o demasiado larga no es una
        // extensión: no se sanea, se rechaza.
        assert_eq!(extension_de("x.p df", &[]), None);
        assert_eq!(extension_de("x.exageradamente_larga", &[]), None);
        assert_eq!(extension_de("x.pd/f", &[]), None);
    }

    #[test]
    fn el_contenido_concuerda_o_se_marca_o_no_se_sabe() {
        // Un PDF de verdad concuerda con .pdf y NO con .png (se marcaría).
        let pdf = b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\nrest";
        assert_eq!(verifica_contenido("pdf", pdf), Some(true));
        assert_eq!(verifica_contenido("png", pdf), Some(false));
        // Un PNG de verdad: concuerda con .png.
        let png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01";
        assert_eq!(verifica_contenido("png", png), Some(true));
        // Un CSV no tiene magic bytes: no se puede saber -> None, NO se marca.
        assert_eq!(verifica_contenido("csv", b"nombre,valor\n1,2\n"), None);
    }

    // --- Cifrado en reposo (#128, A1) ----------------------------------------

    fn dir_tmp(etiqueta: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "siger_reposo_{etiqueta}_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[tokio::test]
    async fn el_sha_firmado_es_el_mismo_en_claro_o_cifrado() {
        // LA invariante de #128: el SHA que firma la entrega es el del DOCUMENTO
        // en claro. Se cifra en reposo, se relee descifrando, y el SHA debe
        // coincidir con el del original —si se hasheara el blob, la verificación
        // cantaría un falso «alterado»—.
        let documento = b"contenido real de la entrega con datos del informe";
        let frase = "frase-de-reposo-del-despliegue-larga-y-propia";
        let sha_original = crate::firma::sha256_hex(documento);

        let dir = dir_tmp("inv");
        let ruta = dir.join("entrega.bin");
        std::fs::write(&ruta, guaca::reposo::cifrar(documento, frase)).unwrap();

        // En disco NO aparece el documento en claro.
        let en_disco = std::fs::read(&ruta).unwrap();
        assert!(!en_disco.windows(documento.len()).any(|w| w == documento));

        let Documento::Claro(claro) = leer_en_claro(&ruta, true, Some(frase)).await else {
            panic!("con la frase correcta el documento se lee")
        };
        assert_eq!(crate::firma::sha256_hex(&claro), sha_original);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn sin_la_frase_o_con_la_equivocada_no_se_sirve_el_blob() {
        // Directiva 20: ante la ausencia de la clave se falla, no se entrega el
        // blob cifrado como si fuera el documento.
        let dir = dir_tmp("fail");
        let ruta = dir.join("e.bin");
        let frase = "la-frase-correcta-de-mas-de-treinta-bytes";
        std::fs::write(&ruta, guaca::reposo::cifrar(b"secreto", frase)).unwrap();

        // «No descifra» y «no está» son hechos distintos y se devuelven distintos:
        // de eso depende que la verificación no acuse de alteración a quien solo
        // perdió la frase.
        assert_eq!(leer_en_claro(&ruta, true, None).await, Documento::NoDescifra);
        assert_eq!(
            leer_en_claro(&ruta, true, Some("frase-equivocada")).await,
            Documento::NoDescifra
        );
        assert_eq!(
            leer_en_claro(&dir.join("no-existe.bin"), true, Some(frase)).await,
            Documento::NoEsta
        );
        assert_eq!(
            leer_en_claro(&ruta, true, Some(frase)).await,
            Documento::Claro(b"secreto".to_vec())
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn un_archivo_en_claro_se_lee_tal_cual_aunque_haya_frase() {
        // Los archivos subidos antes de activar la frase (cifrado=false) se leen
        // directo: la bandera POR FILA manda, no la config global. Así encender la
        // frase no invalida lo ya guardado en claro.
        let dir = dir_tmp("claro");
        let ruta = dir.join("viejo.pdf");
        std::fs::write(&ruta, b"documento viejo en claro").unwrap();
        assert_eq!(
            leer_en_claro(&ruta, false, Some("hay-frase-pero-la-fila-dice-claro")).await,
            Documento::Claro(b"documento viejo en claro".to_vec())
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Una entrega firmada, con el archivo cifrado en reposo, lista para auditar.
    /// Devuelve el estado y el directorio del almacén (que hay que borrar).
    async fn entrega_firmada_y_cifrada(frase: Option<&str>) -> (Estado, std::path::PathBuf) {
        const DOCUMENTO: &[u8] = b"%PDF-1.5\nel informe que se entrego\n";
        let (vk, sk) = crate::firma::generar_claves();
        let dir = dir_tmp("verificar");

        let mut cfg = Config::de_prueba();
        cfg.dir_entregas = dir.to_string_lossy().into_owned();
        cfg.firma_sk_hex = Some(sk);
        cfg.firma_vk_hex = Some(vk);
        cfg.clave_reposo = frase.map(str::to_owned);

        // En disco va el BLOB cifrado; lo que se firma es el SHA del documento.
        let ruta = ruta_de_entrega(&cfg.dir_entregas, 9, "e9.pdf").unwrap();
        std::fs::create_dir_all(ruta.parent().unwrap()).unwrap();
        std::fs::write(&ruta, guaca::reposo::cifrar(DOCUMENTO, FRASE_REPOSO)).unwrap();

        let firma = crate::firma::firmar(
            &crate::firma::Auditable {
                id: 9,
                informe_id: 5,
                usuario_id: Some(1),
                fecha_limite: "2026-01-01",
                fecha_entrega: Some("2026-01-01T10:00:00+00:00"),
                estado: "CUMPLIDO",
                archivo_sha256: Some(&crate::firma::sha256_hex(DOCUMENTO)),
                fecha_vencimiento_nominal: "2026-01-01",
                tipo_ajuste: "NINGUNO",
            },
            cfg.firma_sk_hex.as_deref(),
        )
        .expect("hay clave de firma");

        let bd = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&bd).await.unwrap();
        sqlx::query("INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) VALUES (1,'Op','op@x.co','h','OPERADOR',1)").execute(&bd).await.unwrap();
        sqlx::query("INSERT INTO informes (id,nombre,recurrencia) VALUES (5,'Informe','{\"modo\":\"INMEDIATO\"}')").execute(&bd).await.unwrap();
        sqlx::query(
            "INSERT INTO entregas (id,informe_id,usuario_id,fecha_nominal,fecha_limite,tipo_ajuste,\
             estado,fecha_entrega_real,archivo_nombre,firma,archivo_cifrado) \
             VALUES (9,5,1,'2026-01-01','2026-01-01','NINGUNO','CUMPLIDO','2026-01-01T10:00:00+00:00','e9.pdf',?,1)",
        )
        .bind(&firma)
        .execute(&bd)
        .await
        .unwrap();

        (Estado { bd, cfg, motor: motor_de_prueba() }, dir)
    }

    /// Motor con un rango de años corto: en pruebas no hace falta un siglo de
    /// calendario, y construirlo en cada caso se notaría en la suite.
    fn motor_de_prueba() -> std::sync::Arc<siger_core::Motor> {
        std::sync::Arc::new(siger_core::Motor::nuevo(
            siger_core::Calendario::de_pais("CO", 2024, 2030).unwrap(),
        ))
    }

    const FRASE_REPOSO: &str = "frase-de-reposo-del-despliegue-larga-y-propia";

    fn auditor() -> Actor {
        Actor { id: 2, nombre: "Adm".into(), email: "adm@x.co".into(), rol: Rol::Admin, stakeholder_id: None }
    }

    async fn motivo_de(e: &Estado) -> String {
        let r = verificar_entrega(State(e.clone()), auditor(), RutaParam(9)).await.unwrap();
        r.0["motivo"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn una_entrega_cifrada_e_intacta_verifica() {
        // El camino feliz de #128: el SHA se recalcula descifrando, y cuadra.
        let (e, dir) = entrega_firmada_y_cifrada(Some(FRASE_REPOSO)).await;
        let r = verificar_entrega(State(e.clone()), auditor(), RutaParam(9)).await.unwrap();
        assert_eq!(r.0["verificada"], serde_json::json!(true), "motivo: {}", r.0["motivo"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn sin_la_frase_de_reposo_no_se_acusa_de_alteracion() {
        // El fallo que esto cierra: perder `SIGER_CLAVE_REPOSO` —un reinicio sin el
        // `export`, una rotación en mal orden, un restore en otra máquina— hacía
        // que el SHA quedara en `null`, el canónico no cuadrara, y el extremo le
        // dijera al ente de control «la entrega fue alterada tras la firma» de una
        // entrega que nadie tocó. No poder mirar no es haber visto algo.
        let (e, dir) = entrega_firmada_y_cifrada(None).await;
        let motivo = motivo_de(&e).await;
        assert_ne!(motivo, "la entrega fue alterada tras la firma");
        assert_eq!(motivo, "no se pudo descifrar el archivo de la entrega");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn con_el_almacen_mal_puesto_tampoco_se_acusa_de_alteracion() {
        // La misma acusación falsa por la otra vía: `SIGER_DIR_ENTREGAS` apuntando
        // donde no está el archivo (volumen sin montar, restore a medias).
        let (mut e, dir) = entrega_firmada_y_cifrada(Some(FRASE_REPOSO)).await;
        e.cfg.dir_entregas = dir.join("otro-volumen").to_string_lossy().into_owned();
        let motivo = motivo_de(&e).await;
        assert_ne!(motivo, "la entrega fue alterada tras la firma");
        assert_eq!(motivo, "el archivo de la entrega no está en el almacén");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn el_documento_cambiado_tras_firmar_si_se_delata() {
        // Y la prueba de que sigue DISCRIMINANDO: con la frase puesta y el archivo
        // legible, un contenido distinto al firmado sí es una alteración y se dice
        // con esas palabras. Si esto pasara a «no se pudo descifrar», el arreglo
        // habría convertido la salvaguarda en un atajo.
        let (e, dir) = entrega_firmada_y_cifrada(Some(FRASE_REPOSO)).await;
        let ruta = ruta_de_entrega(&e.cfg.dir_entregas, 9, "e9.pdf").unwrap();
        std::fs::write(&ruta, guaca::reposo::cifrar(b"otro documento", FRASE_REPOSO)).unwrap();
        assert_eq!(motivo_de(&e).await, "la entrega fue alterada tras la firma");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn la_compatibilidad_de_tipos_es_lenient_para_no_marcar_lo_legitimo() {
        assert!(concuerda("pdf", "pdf"));
        assert!(!concuerda("pdf", "png")); // desajuste claro -> se marca
        assert!(concuerda("xlsx", "zip")); // ofimática abierta es zip por dentro
        assert!(concuerda("jpg", "jpeg"));
        assert!(!concuerda("xlsx", "pdf")); // desajuste claro
    }
}
