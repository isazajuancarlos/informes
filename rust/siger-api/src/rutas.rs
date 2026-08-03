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
    routing::{delete, get, post, put},
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

/// La política de contenido. Ver [`cabeceras_de_seguridad`].
///
/// Cada directiva está aquí porque se comprobó contra el Angular COMPILADO, no
/// porque venga en una lista de buenas prácticas.
const CSP: &str = "default-src 'self'; \
                   script-src 'self'; \
                   style-src 'self' 'unsafe-inline'; \
                   img-src 'self' data:; \
                   font-src 'self'; \
                   connect-src 'self'; \
                   object-src 'none'; \
                   base-uri 'self'; \
                   form-action 'self'; \
                   frame-ancestors 'none'";

/// Cabeceras que van en TODA respuesta, también en la interfaz.
///
/// # Por qué en la aplicación y no solo en nginx
///
/// `despliegue/informes.xiliux.com.nginx` ya pone tres de estas. Pero esa es la
/// configuración de UN despliegue: una instalación en casa del cliente, un
/// contenedor, o cualquiera que ponga su propio proxy se queda sin ninguna. Lo
/// que viaja dentro del binario protege siempre; lo que vive en el proxy protege
/// donde alguien se acordó de copiarlo.
///
/// # La CSP: qué se cerró y qué no, con su razón
///
/// - **`script-src 'self'`, sin `'unsafe-inline'` ni `'unsafe-eval'`.** Es la
///   directiva que de verdad para un XSS, y es la que costó trabajo: el
///   `index.html` que producía Angular traía
///   `onload="this.media='all'"` en la hoja de estilos —un manejador en línea, o
///   sea script— porque `inlineCritical` estaba encendido. Con esta directiva el
///   navegador no lo ejecuta, la hoja se queda en `media="print"` y la
///   aplicación se ve SIN ESTILOS *sin dar un solo error*. Se apagó
///   `inlineCritical` en `angular.json`; el `index.html` ya no lleva nada en
///   línea que ejecutar.
/// - **`style-src` sí lleva `'unsafe-inline'`, y no es pereza.** Angular Material
///   escribe estilos en tiempo de ejecución y usa atributos `style="…"` para
///   animaciones y ondas. Un *nonce* no lo arreglaría: los nonces valen para
///   elementos `<style>`, no para atributos `style`, que necesitarían
///   `'unsafe-hashes'`. Quitarlo rompe la interfaz y el riesgo que deja abierto
///   —inyectar CSS— es mucho menor que el de ejecutar código.
/// - **`frame-ancestors 'none'`** duplica a `X-Frame-Options`. La cabecera vieja
///   se queda para los navegadores que aún no leen la directiva.
/// - **Nada de HSTS aquí**: el TLS termina en nginx, que es quien puede
///   prometerlo con conocimiento de causa.
pub async fn cabeceras_de_seguridad(
    peticion: axum::extract::Request,
    siguiente: axum::middleware::Next,
) -> Response {
    let mut r = siguiente.run(peticion).await;
    let h = r.headers_mut();
    // No reinterpretes el tipo que declaro.
    h.insert(header::X_CONTENT_TYPE_OPTIONS, header::HeaderValue::from_static("nosniff"));
    // Nadie enmarca esto: sin ello un sitio ajeno la mete en un iframe y engaña a
    // quien la usa para que pulse donde no cree.
    h.insert(header::X_FRAME_OPTIONS, header::HeaderValue::from_static("DENY"));
    // La ruta de un informe no viaja como `Referer` a terceros.
    h.insert(
        header::REFERRER_POLICY,
        header::HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    h.insert(header::CONTENT_SECURITY_POLICY, header::HeaderValue::from_static(CSP));
    r
}

pub fn router(estado: Estado) -> Router {
    let tope_mb = estado.cfg.max_mb;
    Router::new()
        .route("/api/salud", get(salud))
        .route("/api/demostracion", get(demostracion))
        .route("/api/sesion", post(abrir_sesion).delete(cerrar_sesion))
        .route("/api/salir-todo", post(salir_todo))
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
        .route("/api/mi-clave", post(crate::cuentas::cambiar_mi_clave))
        // El segundo factor (#212). `verificar` es el ÚNICO que no exige sesión:
        // se llama con el token pendiente, que es lo que emite el login cuando la
        // cuenta tiene 2FA. Los otros cuatro son de la cuenta propia y cualquier
        // rol los usa — el segundo factor no es un privilegio de administración.
        .route("/api/2fa", get(crate::segundofactor::estado))
        .route("/api/2fa/iniciar", post(crate::segundofactor::iniciar))
        .route("/api/2fa/activar", post(crate::segundofactor::activar))
        .route("/api/2fa/desactivar", post(crate::segundofactor::desactivar))
        .route("/api/2fa/codigos", post(crate::segundofactor::renovar_codigos))
        .route("/api/2fa/verificar", post(crate::segundofactor::verificar))
        // Administración de cuentas. Recurso propio y no `/api/usuarios`, que
        // significa «quién puede ser parte de un flujo» y lo consume el selector
        // de partes: ver la cabecera de `cuentas.rs`.
        .route(
            "/api/cuentas",
            get(crate::cuentas::listar).post(crate::cuentas::crear),
        )
        .route("/api/cuentas/{id}", put(crate::cuentas::actualizar))
        .route("/api/cuentas/{id}/clave", post(crate::cuentas::restablecer_clave))
        // El rescate de quien perdió el teléfono Y los códigos. Es un DELETE, y
        // los DELETE son los que se caen del router sin que nadie lo note: lo
        // sujeta `humo.sh` pidiendo 401 —no 404— sin sesión.
        .route("/api/cuentas/{id}/2fa", delete(crate::cuentas::retirar_segundo_factor))
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
        let fila = sqlx::query_as::<_, (i64, String, String, String, i64, Option<i64>, i64)>(
            "SELECT id, nombre, email, rol, activo, stakeholder_id, token_version \
             FROM usuarios WHERE id = ?",
        )
        .bind(s.usuario_id)
        .fetch_optional(&estado.bd)
        .await?
        .ok_or_else(no_autorizado)?;

        if fila.4 == 0 {
            return Err(no_autorizado());
        }

        // Revocación selectiva (#194): la versión del token contra la del
        // usuario, en la MISMA lectura que ya se hacía. Distinta = la sesión se
        // emitió antes de una revocación —cambio de contraseña, «cerrar sesión en
        // todos los dispositivos»— y no vale, aunque la firma cuadre y no haya
        // caducado. `<` no: solo `==`. Con `<` bastaría con haber guardado un
        // token de una versión MAYOR (un contador que alguien bajó al restaurar un
        // respaldo) para entrar.
        if s.version != fila.6 {
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

/// El hash contra el que se comprueba la contraseña cuando **no hay usuario**.
///
/// Existe por una razón sola: que las tres respuestas —correo inexistente,
/// cuenta desactivada y contraseña incorrecta— cuesten lo mismo. Antes el correo
/// inexistente salía por la puerta sin llegar a Argon2, así que respondía en
/// menos de un milisegundo mientras el existente tardaba decenas: el mensaje era
/// idéntico y el CRONÓMETRO decía cuáles de una lista de correos son cuentas de
/// verdad. El comentario de esta función afirmaba «mismo mensaje y mismo
/// trabajo», y solo la primera mitad era cierta.
///
/// Se deriva en caliente, una vez, y no se incrusta como constante a propósito:
/// una constante se calculó con los parámetros de Argon2 de su día, y el día que
/// `Argon2::default()` cambie —o que se endurezcan a mano— el señuelo costaría
/// distinto que los hashes reales y la diferencia de tiempo volvería, en
/// silencio. Derivándolo del mismo `default()` que verifica, no puede desalinearse.
fn hash_senuelo() -> &'static PasswordHash<'static> {
    static SENUELO: std::sync::OnceLock<PasswordHash<'static>> = std::sync::OnceLock::new();
    SENUELO.get_or_init(|| {
        use argon2::password_hash::{PasswordHasher, SaltString};
        // La sal es fija y pública, y no protege nada: nadie tiene esta
        // «contraseña», y lo único que se le pide al señuelo es costar lo mismo.
        let sal = SaltString::from_b64("c2VudWVsb2Rlc2lnZXIxMg")
            .expect("la sal del señuelo es base64 válido");
        let phc = Argon2::default()
            .hash_password(b"ninguna cuenta tiene esta clave", &sal)
            .expect("hashear con los parámetros por defecto no falla")
            .to_string();
        // `PasswordHash` presta de la cadena que lo contiene, así que la cadena
        // se filtra a propósito: es UNA, para todo el proceso.
        let prestada: &'static str = Box::leak(phc.into_boxed_str());
        PasswordHash::new(prestada).expect("el PHC recién generado es válido")
    })
}

async fn abrir_sesion(
    State(e): State<Estado>,
    crate::origen::OrigenCliente(origen): crate::origen::OrigenCliente,
    Json(cred): Json<Credenciales>,
) -> Result<Response, Error> {
    let email = cred.email.trim().to_lowercase();

    // Una dirección de más de 320 octetos no existe: es el máximo de la RFC 5321
    // (64 de parte local + `@` + 255 de dominio). No se rechaza lo raro, se
    // rechaza lo IMPOSIBLE — y aquí importa porque el correo TECLEADO se guarda,
    // tanto en el rastro del freno como en la auditoría. Sin este corte, el
    // cuerpo de la petición (2 MB por defecto en axum) entra entero en la base
    // en cada intento, sin autenticarse: a diez peticiones por segundo son giga
    // y medio de disco al día. Se corta antes de escribir NADA.
    const MAX_EMAIL: usize = 320;
    if email.len() > MAX_EMAIL {
        // El mismo 401 de siempre. Responder antes no delata nada: ninguna
        // cuenta puede tener una dirección así, de modo que la diferencia de
        // tiempo no dice qué correos existen.
        return Err(Error(StatusCode::UNAUTHORIZED, "credenciales incorrectas".into()));
    }

    // Sin origen no hay freno posible, y un freno a medias es peor que ninguno
    // porque se cuenta como puesto. Se falla, y el mensaje dice qué arreglar.
    let Some(origen) = origen else {
        tracing::error!(
            "SIGER_TRAS_PROXY está activo pero no llegó la cabecera {}: el proxy no la escribe. \
             Añada `proxy_set_header X-Real-IP $remote_addr` al sitio, o quite SIGER_TRAS_PROXY.",
            crate::origen::CABECERA
        );
        return Err(Error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no se pudo determinar el origen de la petición".into(),
        ));
    };

    let ahora = chrono::Utc::now();
    let freno = crate::acceso::estado(&e.bd, &email, &origen, ahora).await?;
    if freno.frenado() {
        // 429 y `Retry-After`, no 401: decir «credenciales incorrectas» cuando lo
        // que pasa es que hay que esperar manda al usuario legítimo a cambiar una
        // contraseña que estaba bien.
        let segundos = freno.segundos_restantes();
        auditar(&e.bd, "acceso_frenado", Some(&email), None).await;
        let mut cabeceras = HeaderMap::new();
        cabeceras.insert(
            header::RETRY_AFTER,
            segundos.to_string().parse().expect("un número es ASCII"),
        );
        return Ok((
            StatusCode::TOO_MANY_REQUESTS,
            cabeceras,
            Json(serde_json::json!({
                "error": format!("demasiados intentos. Vuelva a probar en {segundos} s."),
                "segundos": segundos,
            })),
        )
            .into_response());
    }

    // `totp_confirmado_en` viaja en la MISMA consulta que ya se hacía. Traerlo
    // después, con el usuario ya identificado, sería una segunda consulta que
    // solo ocurre cuando el correo existe — y el cronómetro volvería a decir
    // cuáles existen, que es justo lo que el señuelo evita.
    let fila = sqlx::query_as::<_, (i64, String, i64, i64, Option<String>)>(
        "SELECT id, clave_hash, activo, token_version, totp_confirmado_en \
         FROM usuarios WHERE email = ?",
    )
    .bind(&email)
    .fetch_optional(&e.bd)
    .await?;
    let con_segundo_factor = matches!(&fila, Some((_, _, _, _, Some(_))));

    // Mismo mensaje y mismo trabajo para «no existe» y «clave incorrecta»: la
    // diferencia —cualquiera de las dos— le dice a quien prueba qué correos son
    // de verdad. Ver `hash_senuelo` para la mitad que faltaba.
    let rechazo = Error(StatusCode::UNAUTHORIZED, "credenciales incorrectas".into());
    let (id, version, analizado) = match &fila {
        Some((id, hash, activo, version, _)) if *activo != 0 => match PasswordHash::new(hash) {
            Ok(h) => (Some(*id), *version, h),
            Err(_) => {
                // Un hash que ningún esquema reconoce no deja entrar, pero se
                // registra: significa que alguien quedó fuera del sistema. Se
                // sigue hasta el señuelo para no responder antes de tiempo.
                tracing::error!("el hash del usuario {id} no lo reconoce ningún esquema");
                (None, 0, hash_senuelo().clone())
            }
        },
        // Correo que no existe, o cuenta desactivada: mismo trabajo.
        _ => (None, 0, hash_senuelo().clone()),
    };

    // Fuera del hilo del runtime, y con turno, por la MISMA razón que el
    // descifrado (ver `leer_en_claro`) y con un agravante: aquí no hace falta
    // sesión. El señuelo hace que TODO intento —también el de un correo que no
    // existe— pague un Argon2 completo, que es lo correcto (sin él, el cronómetro
    // delata qué cuentas existen), pero eso pone 45 ms de CPU y 19 MiB en manos de
    // cualquiera. Medido con un solo hilo de trabajo: 32 intentos fallidos a la
    // vez llevaban `/api/salud` de 0 a 711 ms.
    let hash_texto = analizado.to_string();
    let clave = cred.clave.clone();
    let permiso = match tokio::time::timeout(ESPERA_CRIPTO, e.cfg.cripto.clone().acquire_owned())
        .await
    {
        Ok(Ok(p)) => p,
        _ => {
            tracing::warn!("acceso: sin turno de cripto en {} s", ESPERA_CRIPTO.as_secs());
            // 503 y no el 401 de siempre. Rechazar por «credenciales incorrectas»
            // cuando el servidor está lleno le contaría al usuario legítimo una
            // mentira sobre SU contraseña, y de paso dejaría al freno anotando
            // fallos que nadie cometió. Que sea un código distinto no delata
            // ninguna cuenta: habla del servidor, no del correo.
            return Err(Error(
                StatusCode::SERVICE_UNAVAILABLE,
                "el servidor está ocupado; reinténtelo".into(),
            ));
        }
    };
    let clave_correcta = tokio::task::spawn_blocking(move || {
        let r = PasswordHash::new(&hash_texto)
            .map(|h| Argon2::default().verify_password(clave.as_bytes(), &h).is_ok())
            .unwrap_or(false);
        drop(permiso); // el turno se libera con la memoria, no antes
        r
    })
    .await
    .unwrap_or(false);
    // El señuelo nunca casa —nadie tiene esa contraseña—, pero que la respuesta
    // no dependa de si había usuario tiene que ser explícito y no una casualidad
    // del optimizador.
    let entra = clave_correcta && id.is_some();

    if !entra {
        crate::acceso::anotar(&e.bd, &email, &origen, false, ahora).await?;
        auditar(&e.bd, "acceso_fallido", Some(&email), None).await;
        return Err(rechazo);
    }
    let id = id.expect("`entra` exige que haya usuario");

    // Con segundo factor, la contraseña correcta **no abre nada**: emite el token
    // pendiente, que solo sirve para `/api/2fa/verificar` y dura cinco minutos.
    //
    // Y **no se anota nada en `intento_acceso`**, ni éxito ni fallo. Aquí antes se
    // anotaba el éxito, con el argumento de que la contraseña era buena y dejar el
    // fallo puesto frenaría a quien está a punto de teclear su código. El
    // argumento no se sostiene y la consecuencia era grave:
    //
    // - `guaca::freno` no cuenta fallos en una ventana fija: cuenta **los
    //   posteriores al último éxito**. Un éxito escrito aquí pone el contador a
    //   cero.
    // - Ese contador es la pareja `(correo, origen)`, y es EL MISMO que consulta
    //   `/api/2fa/verificar`: es decir, el único freno que protege al segundo
    //   factor.
    // - Quien ya tiene la contraseña —que es exactamente el atacante contra el
    //   que existe el segundo factor— podía reponerlo cuando quisiera: cuatro
    //   códigos malos, volver a entrar, otros cuatro. El freno no llegaba a
    //   engancharse NUNCA, y el millón de combinaciones bajaba de años a horas.
    // - Lo que motivaba el éxito no hacía falta: con cuatro fallos o menos la
    //   espera ya es cero, y con cinco el freno saltó antes de llegar hasta aquí.
    //
    // El éxito lo escribe quien abre la puerta de verdad: el camino sin segundo
    // factor, abajo, y `segundofactor::verificar` al final del segundo paso.
    if con_segundo_factor {
        auditar(&e.bd, "acceso_falta_segundo_factor", Some(&email), None).await;
        let valor = sesion::emitir_pendiente(&e.cfg.secreto_sesion, id, version);
        let mut cabeceras = HeaderMap::new();
        cabeceras.insert(
            header::SET_COOKIE,
            sesion::cabecera_pendiente(&valor).parse().expect("la cabecera es ASCII"),
        );
        return Ok((
            cabeceras,
            Json(serde_json::json!({ "ok": false, "segundo_factor": true })),
        )
            .into_response());
    }

    // Sin segundo factor, ESTA es la puerta: aquí sí se anota el éxito, que es lo
    // que le borra los fallos al freno.
    crate::acceso::anotar(&e.bd, &email, &origen, true, ahora).await?;
    auditar(&e.bd, "acceso", Some(&email), None).await;
    let valor = sesion::emitir(&e.cfg.secreto_sesion, id, version, e.cfg.minutos_sesion);
    let mut cabeceras = HeaderMap::new();
    cabeceras.insert(
        header::SET_COOKIE,
        sesion::cabecera_abrir(&valor, e.cfg.minutos_sesion)
            .parse()
            .expect("la cabecera es ASCII"),
    );
    Ok((cabeceras, Json(serde_json::json!({ "ok": true, "segundo_factor": false }))).into_response())
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

/// `POST /api/salir-todo` — «cerrar sesión en todos los dispositivos» (#194).
///
/// Sube el `token_version` del usuario, con lo que toda cookie ya emitida —la de
/// este navegador incluida— deja de valer en la siguiente petición, y de paso
/// borra la de aquí para que la pantalla no se quede enseñando una sesión muerta.
///
/// # Actúa sobre el usuario AUTENTICADO, nunca sobre un id del cliente
///
/// No recibe cuerpo ni parámetro. Si el id viniera de fuera, este extremo sería
/// un botón para echar a cualquiera de su sesión con solo saber su número: una
/// denegación de servicio contra una persona concreta, servida por la propia
/// aplicación. El `Actor` sale del extractor, que lo saca de la cookie firmada.
///
/// Es de uso propio a propósito. Un administrador que quiera dejar fuera a
/// alguien tiene ya el camino correcto —desactivar la cuenta, que `activo` se
/// relee en cada petición—; revocar sesiones ajenas sin desactivar la cuenta no
/// resuelve ningún caso que ese camino no cubra ya (directiva 1).
async fn salir_todo(State(e): State<Estado>, actor: Actor) -> Result<Response, Error> {
    let revocadas = crate::acceso::revocar_sesiones(&e.bd, actor.id).await?;
    if !revocadas {
        // La sentencia no tocó ninguna fila: el usuario que acabamos de
        // autenticar ya no está en la base. No hubo revocación, así que tampoco
        // hay 200 — descartar este booleano sería fingir el cierre que el
        // doc-comment promete no fingir, y en el peor sitio para hacerlo.
        tracing::error!("revocación sin efecto: el usuario {} ya no existe", actor.id);
        return Err(Error(StatusCode::INTERNAL_SERVER_ERROR, "error interno".into()));
    }

    auditar(&e.bd, "salida_total", Some(&actor.email), None).await;
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

/// `GET /api/demostracion` — las cuentas que esta instalación anuncia.
///
/// Sin sesión, porque quien lo pide todavía no puede tener ninguna: es la
/// pantalla de acceso preguntando si aquí hay algo que enseñar.
///
/// **En una instalación de cliente devuelve una lista vacía**, y no un 404: el
/// 404 obligaría a la interfaz a distinguir «no hay demostración» de «esta
/// versión no tiene ese extremo», y son dos cosas que se dibujan igual. Lo que
/// hay dentro sale entero de `SIGER_DEMO_CUENTAS`, así que aquí no se publica
/// nada que el despliegue no haya escrito a mano.
async fn demostracion(State(e): State<Estado>) -> Json<Vec<crate::config::CuentaDemo>> {
    Json(e.cfg.demo.clone())
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
        Alcance::Ninguna => {
            auditar_entrega_ajena(&e, &actor, id).await;
            return Err(no_existe());
        }
    };

    let mut q = sqlx::query_as::<_, (Option<String>, i64)>(&sql).bind(id);
    if let Some(p) = param {
        q = q.bind(p);
    }
    let Some((nombre, cifrado)) = q.fetch_optional(&e.bd).await? else {
        auditar_entrega_ajena(&e, &actor, id).await;
        return Err(no_existe());
    };
    // Que la entrega no tenga archivo NO es un acceso ajeno: es suya y está
    // vacía. Va por el 404 sin anotar nada.
    let nombre = nombre.ok_or_else(no_existe)?;

    let ruta = ruta_de_entrega(&e.cfg.dir_entregas, id, &nombre).ok_or_else(|| {
        tracing::error!("nombre de archivo inaceptable en la entrega {id}: {nombre:?}");
        Error(StatusCode::INTERNAL_SERVER_ERROR, "error interno".into())
    })?;

    let datos = match leer_en_claro(&ruta, cifrado != 0, &e.cfg).await {
        Documento::Claro(d) => d,
        // Que no esté es un 404 esté cifrado o no: el estado del despliegue no
        // cambia qué le pasó al archivo.
        Documento::NoEsta => return Err(no_existe()),
        // 503 y no 500: el archivo está perfectamente y volver a pedirlo dentro de
        // un rato funcionará. Un 500 mandaría a buscar una avería que no existe.
        Documento::Ocupado => {
            return Err(Error(
                StatusCode::SERVICE_UNAVAILABLE,
                "el servidor está atendiendo demasiadas descargas cifradas; reinténtelo".into(),
            ));
        }
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

    // Se anota AQUÍ y no al entrar: el evento afirma que el documento salió, y
    // los otros tres estados de `Documento` no lo entregaron. Un «descargada»
    // sobre un archivo que el servidor no llegó a mirar sería la misma clase de
    // mentira que acusar de alteración al que estaba ocupado.
    auditar(
        &e.bd,
        "entrega_descargada",
        Some(&actor.email),
        Some(&format!("{id}: {nombre}")),
    )
    .await;

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
        Alcance::Ninguna => {
            auditar_entrega_ajena(&e, &actor, id).await;
            return Err(no_existe());
        }
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
    let Some((limite, informe_id, usuario_id, fecha_nominal, tipo_ajuste, estado, fecha_entrega, archivo_nombre, firma, cifrado)) =
        q.fetch_optional(&e.bd).await?
    else {
        auditar_entrega_ajena(&e, &actor, id).await;
        return Err(no_existe());
    };

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
    match leer_en_claro(&ruta, cifrado, cfg).await {
        Documento::Claro(d) => Ok(Some(crate::firma::sha256_hex(&d))),
        Documento::NoEsta => {
            tracing::error!("entrega {id}: el archivo firmado no está en el almacén");
            Err("el archivo de la entrega no está en el almacén")
        }
        // «No se puede saber», que es exactamente lo que pasó: no se llegó a leer
        // el documento. Decir «alterada» porque el servidor estaba lleno sería la
        // acusación falsa que documenta la cabecera de esta función.
        Documento::Ocupado => {
            tracing::warn!("entrega {id}: no se verificó, el servidor estaba saturado de descifrados");
            Err("el servidor estaba ocupado y no se pudo leer el archivo; reinténtelo")
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
    /// No se llegó a MIRAR el archivo: el servidor tenía ya todos sus descifrados
    /// en marcha y el turno no llegó a tiempo.
    ///
    /// **Es un caso aparte de `NoDescifra` a propósito**, y por la misma razón que
    /// `NoEsta` lo es. «No descifra» dice algo sobre el ARCHIVO —que está alterado,
    /// o que la frase no corresponde— y el verificador lo traduce en una duda sobre
    /// la entrega. «Ocupado» no dice nada sobre el archivo: dice que el servidor
    /// estaba lleno. Meterlos en el mismo cajón haría que una tarde de mucho
    /// tráfico se leyera como una entrega sospechosa, que es exactamente la
    /// acusación falsa que este módulo existe para no hacer.
    Ocupado,
}

/// Cuánto se espera un turno de trabajo criptográfico pesado antes de rendirse.
///
/// Lo comparten el descifrado en reposo (64 MiB) y el Argon2 del login (19 MiB).
///
/// Generoso a propósito: lo que se rechaza es la sobrecarga real, no el uso
/// intenso. Un navegador abre seis conexiones por servidor, así que el peor caso
/// de un usuario normal —seis descargas a la vez con un solo permiso— espera cerca
/// de un segundo. Treinta segundos solo se alcanzan si de verdad hay cientos de
/// peticiones encima, y siguen por debajo del `proxy_read_timeout` de nginx (60 s),
/// de modo que quien contesta el 503 es el servidor, que sabe por qué, y no el
/// proxy con un 504 mudo.
pub(crate) const ESPERA_CRIPTO: std::time::Duration = std::time::Duration::from_secs(30);

/// Lee el archivo de una entrega y lo devuelve EN CLARO. Si la fila dice que está
/// cifrado, lo descifra con la frase de reposo. **Nunca** se sirve un blob
/// cifrado como si fuera el documento (directiva 20: ante la ausencia de la clave
/// se falla, no se entrega basura).
/// # Por qué el descifrado NO corre aquí
///
/// `guaca::reposo::descifrar` es un Argon2id completo: 162 ms de CPU y 64 MiB,
/// medidos, y **da igual el tamaño del archivo**. Hasta hoy se llamaba en esta
/// misma función `async`, es decir SOBRE un hilo de trabajo de tokio, que se
/// quedaba clavado esos 162 ms sin poder atender nada más.
///
/// No era una teoría. Contra el binario real, con un solo hilo de trabajo —un VPS
/// de 1 vCPU— la latencia máxima de `/api/salud`, el punto más barato del sistema:
///
/// | descargas cifradas en paralelo | `/api/salud` peor caso |
/// |---|---|
/// | 0 | 0 ms |
/// | 4 | 329 ms |
/// | 16 | 2 019 ms |
/// | 32 | 2 744 ms |
///
/// Es decir: un puñado de descargas AUTENTICADAS y perfectamente legítimas
/// congelaban el inicio de sesión de todos los demás. No hacía falta un atacante,
/// bastaba un auditor con prisa.
///
/// Ahora el descifrado va a `spawn_blocking` —fuera de los hilos del runtime— y el
/// semáforo de [`crate::config::permisos_de_cripto`] limita cuántos corren a la
/// vez, que es lo que acota la memoria. Las dos cosas juntas, porque
/// `spawn_blocking` a secas admitiría 512 en paralelo: 32 GiB.
async fn leer_en_claro(ruta: &Path, cifrado: bool, cfg: &Config) -> Documento {
    let Ok(crudo) = tokio::fs::read(ruta).await else {
        return Documento::NoEsta;
    };
    if !cifrado {
        return Documento::Claro(crudo);
    }
    // Sin frase no hay nada que esperar: se responde ya, sin ocupar un turno que
    // otro sí puede aprovechar.
    let Some(clave) = cfg.clave_reposo.clone() else {
        return Documento::NoDescifra;
    };

    // `acquire_owned` y no `acquire`, y el permiso viaja DENTRO de la tarea
    // bloqueante. No es un detalle de préstamos: si el permiso se soltara aquí, al
    // terminar este `await`, un cliente que corta la conexión a mitad de descarga
    // haría que axum tirase esta función —liberando el permiso— mientras el
    // Argon2id sigue corriendo con sus 64 MiB. Repetido deprisa, el tope dejaría
    // de acotar nada y volveríamos al techo de 512 hilos, que es justo lo que el
    // semáforo existe para evitar. Soltándolo dentro, el turno solo se libera
    // cuando la memoria de verdad se libera.
    let permiso = match tokio::time::timeout(
        ESPERA_CRIPTO,
        cfg.cripto.clone().acquire_owned(),
    )
    .await
    {
        // El semáforo vive en `Config`, que vive tanto como el proceso: solo se
        // cierra si alguien llama a `close()`, y nadie lo hace.
        Ok(Ok(p)) => p,
        Ok(Err(_)) => return Documento::Ocupado,
        Err(_) => {
            tracing::warn!(
                "descifrado en cola más de {} s: el servidor está por encima de \
                 SIGER_CRIPTO_PARALELA. Si se repite, o sobra memoria para \
                 subirlo, o falta capacidad.",
                ESPERA_CRIPTO.as_secs()
            );
            return Documento::Ocupado;
        }
    };

    let resultado = tokio::task::spawn_blocking(move || {
        let r = guaca::reposo::descifrar(&crudo, &clave);
        drop(permiso); // el turno se libera con la memoria, no antes
        r
    })
    .await;

    match resultado {
        Ok(Ok(claro)) => Documento::Claro(claro),
        Ok(Err(_)) => Documento::NoDescifra,
        // La tarea bloqueante entró en pánico o el runtime está cerrando. No es un
        // hecho sobre el archivo, así que tampoco puede salir por `NoDescifra`.
        Err(e) => {
            tracing::error!("el descifrado no llegó a terminar: {e}");
            Documento::Ocupado
        }
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

/// Anota el intento de leer una entrega que EXISTE y cae fuera del alcance de
/// quien la pide.
///
/// La segunda consulta corre solo en el camino de error, y existe para
/// **discriminar**: un id inexistente es un enlace viejo o una entrega borrada, y
/// anotarlo llenaría la bitácora de falsos positivos hasta que nadie la mirara
/// (directiva 23). Un id que existe y no le corresponde es la única señal que
/// dice algo — alguien probando números ajenos.
///
/// Quien lo ve todo no necesita un caso aparte, y **no se le pone uno**: para
/// `Alcance::Todas` el `WHERE` es solo `id = ?`, así que si el manejador llegó
/// aquí es que la entrega no existe, y esta consulta tampoco la encuentra. Un
/// `if` que saliera antes ahorraría una consulta y no decidiría nada — y un atajo
/// que parece salvaguarda sin serlo es peor que la consulta que evita
/// (directiva 23). Lo sujeta el caso del administrador en
/// `se_anota_la_entrega_ajena_y_no_la_inexistente`.
///
/// La respuesta al cliente no cambia: sigue siendo el mismo 404 en los dos casos.
/// Lo que se discrimina es lo que se ESCRIBE, no lo que se contesta.
async fn auditar_entrega_ajena(e: &Estado, actor: &Actor, id: i64) {
    match sqlx::query_scalar::<_, i64>("SELECT 1 FROM entregas WHERE id = ?")
        .bind(id)
        .fetch_optional(&e.bd)
        .await
    {
        Ok(Some(_)) => {
            auditar(
                &e.bd,
                "entrega_ajena_solicitada",
                Some(&actor.email),
                Some(&id.to_string()),
            )
            .await;
        }
        Ok(None) => {}
        Err(err) => tracing::error!("no se pudo comprobar si la entrega {id} existe: {err}"),
    }
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

/// El cartel de la demostración pública: qué se anuncia y qué no.
///
/// Las dos pruebas son las dos mitades de la misma característica: que un
/// cliente NO publique nada, y que lo que la demostración publica sirva de
/// verdad para entrar.
#[cfg(test)]
mod pruebas_demostracion {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    async fn router_con(demo: Vec<crate::config::CuentaDemo>, clave_real: &str) -> axum::Router {
        use argon2::password_hash::{PasswordHasher, SaltString};
        let bd = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&bd).await.unwrap();
        let sal = SaltString::from_b64("c2FsZGVsYXBydWViYWRlZGVtbw").unwrap();
        let hash = Argon2::default().hash_password(clave_real.as_bytes(), &sal).unwrap().to_string();
        sqlx::query(
            "INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) \
             VALUES (1,'Administradora','admin@co.co',?,'ADMIN',1)",
        )
        .bind(&hash)
        .execute(&bd)
        .await
        .unwrap();

        let mut cfg = Config::de_prueba();
        cfg.demo = demo;
        router(Estado {
            bd,
            cfg,
            motor: std::sync::Arc::new(siger_core::Motor::nuevo(
                siger_core::Calendario::de_pais("CO", 2024, 2030).unwrap(),
            )),
        })
    }

    fn pedir(metodo: &str, uri: &str, cuerpo: Option<&str>) -> Request<Body> {
        let b = Request::builder().method(metodo).uri(uri).extension(
            axum::extract::ConnectInfo("127.0.0.1:5555".parse::<std::net::SocketAddr>().unwrap()),
        );
        match cuerpo {
            Some(j) => b
                .header("content-type", "application/json")
                .body(Body::from(j.to_string()))
                .unwrap(),
            None => b.body(Body::empty()).unwrap(),
        }
    }

    async fn json(app: &axum::Router, r: Request<Body>) -> (StatusCode, serde_json::Value) {
        let res = app.clone().oneshot(r).await.unwrap();
        let s = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
        (s, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
    }

    #[tokio::test]
    async fn una_instalacion_de_cliente_no_anuncia_ninguna_cuenta() {
        // La mitad que de verdad importa de esta característica. El extremo
        // existe en TODOS los binarios —es el mismo producto—, así que lo único
        // que separa a un cliente de tener sus credenciales publicadas es que
        // esta lista salga vacía cuando nadie escribió la variable.
        let app = router_con(Vec::new(), "la-de-verdad").await;
        let (s, cuerpo) = json(&app, pedir("GET", "/api/demostracion", None)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(cuerpo, serde_json::json!([]), "una instalación normal anunció algo");
    }

    #[tokio::test]
    async fn con_lo_que_la_demostracion_anuncia_se_entra() {
        // Un cartel que dice una contraseña que no funciona es peor que no tener
        // cartel: el visitante concluye que el producto está roto, y se va. Así
        // que no basta con que el extremo devuelva la lista; hay que entrar con
        // lo que devuelve. La contraseña de la base y la anunciada se escriben
        // aquí por separado a propósito — si el día de mañana alguien las
        // desacopla, esta prueba lo dice.
        let anunciada = "admin";
        let app = router_con(
            vec![crate::config::CuentaDemo {
                correo: "admin@co.co".into(),
                clave: anunciada.into(),
                nombre: "Administradora".into(),
            }],
            anunciada,
        )
        .await;

        let (s, cuerpo) = json(&app, pedir("GET", "/api/demostracion", None)).await;
        assert_eq!(s, StatusCode::OK);
        let correo = cuerpo[0]["correo"].as_str().unwrap().to_string();
        let clave = cuerpo[0]["clave"].as_str().unwrap().to_string();
        assert_eq!(cuerpo[0]["nombre"], "Administradora");

        let (s, _) = json(
            &app,
            pedir("POST", "/api/sesion", Some(&format!(r#"{{"email":"{correo}","clave":"{clave}"}}"#))),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "la cuenta que se anuncia no pudo entrar");
    }
}

/// El freno del login y la revocación selectiva, probados en la RUTA (#194).
///
/// No basta con que `guaca::freno` diga la regla: el borde que importa es el que
/// aplica este manejador, y las dos maneras de contar la misma cosa es como se
/// llega a que una diga cinco y la otra seis.
#[cfg(test)]
mod pruebas_acceso {
    use super::*;

    const CLAVE: &str = "la-clave-de-la-prueba";

    /// Una base con un usuario real: hash de Argon2 de verdad, porque lo que se
    /// mide aquí incluye lo que cuesta comprobarlo.
    async fn estado_con_usuario() -> Estado {
        use argon2::password_hash::{PasswordHasher, SaltString};

        let bd = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&bd).await.unwrap();
        let sal = SaltString::from_b64("c2FsZGVsYXBydWViYWRlYWNj").unwrap();
        let hash = Argon2::default().hash_password(CLAVE.as_bytes(), &sal).unwrap().to_string();
        sqlx::query(
            "INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) \
             VALUES (1,'Ada','ada@x.co',?,'ADMIN',1)",
        )
        .bind(&hash)
        .execute(&bd)
        .await
        .unwrap();

        Estado {
            bd,
            cfg: Config::de_prueba(),
            motor: std::sync::Arc::new(siger_core::Motor::nuevo(
                siger_core::Calendario::de_pais("CO", 2024, 2030).unwrap(),
            )),
        }
    }

    async fn intentar(e: &Estado, email: &str, clave: &str, origen: &str) -> Response {
        let credenciales = Credenciales { email: email.into(), clave: clave.into() };
        match abrir_sesion(
            State(e.clone()),
            crate::origen::OrigenCliente(Some(origen.into())),
            Json(credenciales),
        )
        .await
        {
            Ok(r) => r,
            Err(err) => err.into_response(),
        }
    }

    #[tokio::test]
    async fn el_servidor_lleno_no_le_dice_a_nadie_que_su_clave_esta_mal() {
        // Con el cupo de cripto agotado, la clave CORRECTA no puede salir por 401.
        // Sería mentirle al usuario sobre su propia contraseña —y mandarle a
        // cambiarla— y de paso dejaría al freno anotando un fallo que nadie
        // cometió: seis tardes con el servidor lleno y la cuenta se frena sola.
        let e = estado_con_usuario().await;

        // Primero, la mitad que evita que esta prueba pase por la razón
        // equivocada: con turno libre, la clave buena entra.
        assert_eq!(
            intentar(&e, "ada@x.co", CLAVE, "203.0.113.9").await.status(),
            StatusCode::OK,
            "con turno libre la clave correcta tiene que entrar"
        );

        // Y ahora sin turno posible. Se CIERRA el semáforo en vez de agotarlo con
        // el reloj virtual: `pause` + `advance` aquí no vale, porque este camino
        // consulta la base y el `advance` dispara también el plazo del pool de
        // sqlx — la petición muere con un 500 antes de llegar al turno. Costó un
        // rojo entender que el 500 no era mi código. Un semáforo cerrado devuelve
        // «no hay turno» en el acto, sin relojes, y ejercita exactamente la misma
        // rama.
        let mut e = e;
        e.cfg.cripto = std::sync::Arc::new(tokio::sync::Semaphore::new(1));
        e.cfg.cripto.close();

        assert_eq!(
            intentar(&e, "ada@x.co", CLAVE, "203.0.113.9").await.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "sin turno, la clave BUENA salió como credenciales incorrectas"
        );
    }

    #[tokio::test]
    async fn cinco_fallos_no_frenan_y_el_sexto_intento_si() {
        let e = estado_con_usuario().await;
        for i in 0..5 {
            let r = intentar(&e, "ada@x.co", "no-es", "203.0.113.9").await;
            assert_eq!(r.status(), StatusCode::UNAUTHORIZED, "el intento {i} no dio 401");
        }
        // El sexto ya no es un 401: es un 429 con cuánto hay que esperar. Decir
        // «credenciales incorrectas» aquí manda a cambiar una contraseña buena.
        let r = intentar(&e, "ada@x.co", "no-es", "203.0.113.9").await;
        assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
        let espera: u64 = r
            .headers()
            .get(header::RETRY_AFTER)
            .expect("un 429 sin Retry-After no dice cuándo volver")
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(espera >= 1, "Retry-After: 0 invita a reintentar ya mismo");
    }

    /// Insistir durante la espera NO la alarga.
    ///
    /// El intento frenado sale por 429 y NO se anota, y eso hay que sujetarlo con
    /// una prueba porque el razonamiento contrario es convincente: «si no se
    /// anota, insistir sale gratis». Sale caro de otra manera. Anotarlo hace que
    /// cada reintento sume un fallo, y como la espera dobla por fallo, la
    /// impaciencia multiplica lo que hay que esperar hasta el techo — medido en
    /// medico, que lo hacía así: tres clics llevaban la espera de 30 s a 300 s, y
    /// al vencer, otro clic devolvía arriba. El freno dejaba de limitar el ritmo
    /// de ensayo y pasaba a ser un bloqueo que la víctima se renueva sola.
    ///
    /// Al atacante paciente no le afecta: él espera. Le afecta a quien pulsa tres
    /// veces creyendo que la página se colgó.
    ///
    /// No se exige que el `Retry-After` sea el mismo —el tiempo corre—, sino que
    /// NO HAYA CRECIDO.
    #[tokio::test]
    async fn insistir_durante_la_espera_no_alarga_la_espera() {
        let e = estado_con_usuario().await;
        for _ in 0..6 {
            intentar(&e, "ada@x.co", "no-es", "203.0.113.9").await;
        }
        let espera = |r: Response| -> u64 {
            assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS, "se esperaba estar frenado");
            r.headers()
                .get(header::RETRY_AFTER)
                .expect("un 429 sin Retry-After no dice cuándo volver")
                .to_str()
                .unwrap()
                .parse()
                .expect("Retry-After no es un número de segundos")
        };
        let inicial = espera(intentar(&e, "ada@x.co", "no-es", "203.0.113.9").await);
        assert!(inicial >= 1, "no llegó a frenar");

        for _ in 0..3 {
            intentar(&e, "ada@x.co", "no-es", "203.0.113.9").await;
        }
        let despues = espera(intentar(&e, "ada@x.co", "no-es", "203.0.113.9").await);
        assert!(
            despues <= inicial,
            "insistir alargó la espera de {inicial}s a {despues}s: el intento frenado se \
             está anotando, y eso convierte el freno en un bloqueo"
        );
    }

    #[tokio::test]
    async fn con_la_clave_buena_se_entra_y_el_freno_se_borra() {
        let e = estado_con_usuario().await;
        for _ in 0..4 {
            intentar(&e, "ada@x.co", "no-es", "203.0.113.9").await;
        }
        let r = intentar(&e, "ada@x.co", CLAVE, "203.0.113.9").await;
        assert_eq!(r.status(), StatusCode::OK, "la clave correcta no entró");

        // Y tras entrar, la cuenta vuelve a cero: equivocarse cuatro veces un
        // martes no puede penalizar el miércoles.
        for i in 0..5 {
            let r = intentar(&e, "ada@x.co", "no-es", "203.0.113.9").await;
            assert_eq!(r.status(), StatusCode::UNAUTHORIZED, "el intento {i} ya venía frenado");
        }
    }

    #[tokio::test]
    async fn el_freno_es_de_quien_falla_y_no_del_dueno_de_la_cuenta() {
        // La prueba que justifica meter el origen en la clave: por mucho que un
        // desconocido aporree el correo de Ada, Ada entra desde su equipo. Sin
        // esto, seis peticiones de fuera dejan a alguien sin sistema.
        let e = estado_con_usuario().await;
        for _ in 0..8 {
            intentar(&e, "ada@x.co", "no-es", "203.0.113.9").await;
        }
        assert_eq!(
            intentar(&e, "ada@x.co", "no-es", "203.0.113.9").await.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "el que insiste no quedó frenado"
        );
        assert_eq!(
            intentar(&e, "ada@x.co", CLAVE, "10.0.0.5").await.status(),
            StatusCode::OK,
            "la dueña de la cuenta quedó fuera por culpa de otro"
        );
    }

    #[tokio::test]
    async fn un_correo_que_no_existe_se_frena_igual() {
        // Si solo se frenaran las cuentas reales, recibir el 429 diría que el
        // correo existe — justo lo que el login evita dando el mismo 401 para
        // todo. El freno no puede deshacer lo que la respuesta oculta.
        let e = estado_con_usuario().await;
        for _ in 0..5 {
            let r = intentar(&e, "no-existe@x.co", "loquesea", "203.0.113.9").await;
            assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        }
        assert_eq!(
            intentar(&e, "no-existe@x.co", "loquesea", "203.0.113.9").await.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "un correo inexistente no se frenó: el freno delata qué cuentas existen"
        );
    }

    #[tokio::test]
    async fn un_correo_inexistente_cuesta_lo_mismo_que_uno_existente() {
        // El oráculo de tiempo. Antes, el correo que no existía salía sin llegar
        // a Argon2: mismo mensaje, y el CRONÓMETRO diciendo cuáles de una lista
        // son cuentas de verdad. La diferencia era de tres órdenes de magnitud,
        // así que un solo par de medidas la separa de sobra; el margen es ancho a
        // propósito para que la prueba no dependa de la carga de la máquina.
        let e = estado_con_usuario().await;

        let t0 = std::time::Instant::now();
        intentar(&e, "ada@x.co", "no-es", "203.0.113.9").await;
        let existente = t0.elapsed();

        let t1 = std::time::Instant::now();
        intentar(&e, "no-existe@x.co", "no-es", "198.51.100.4").await;
        let inexistente = t1.elapsed();

        assert!(
            inexistente * 3 >= existente,
            "el correo inexistente respondió en {inexistente:?} y el existente en \
             {existente:?}: la diferencia dice qué cuentas existen"
        );
    }

    #[tokio::test]
    async fn una_cuenta_desactivada_tampoco_responde_antes() {
        // La tercera rama del mismo oráculo, y la que se olvida: desactivar a
        // alguien no puede convertirlo en un correo que responde distinto.
        let e = estado_con_usuario().await;
        sqlx::query("UPDATE usuarios SET activo = 0 WHERE id = 1").execute(&e.bd).await.unwrap();

        let t0 = std::time::Instant::now();
        let r = intentar(&e, "ada@x.co", CLAVE, "203.0.113.9").await;
        let desactivada = t0.elapsed();
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED, "una cuenta desactivada entró");

        let t1 = std::time::Instant::now();
        intentar(&e, "no-existe@x.co", CLAVE, "198.51.100.4").await;
        let inexistente = t1.elapsed();

        assert!(
            desactivada * 3 >= inexistente && inexistente * 3 >= desactivada,
            "desactivada {desactivada:?} vs inexistente {inexistente:?}"
        );
    }

    /// Extrae el `Actor` de una petición que lleva esta cookie, como haría
    /// cualquier extremo protegido.
    async fn actor_con(e: &Estado, cookie: &str) -> Result<Actor, Error> {
        let req = axum::http::Request::builder()
            .header(header::COOKIE, format!("{}={cookie}", sesion::NOMBRE_COOKIE))
            .body(axum::body::Body::empty())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        Actor::from_request_parts(&mut parts, e).await
    }

    #[tokio::test]
    async fn subir_token_version_invalida_las_sesiones_ya_abiertas() {
        // El caso de #194: «acabo de cambiar la contraseña, echa a quien entrara
        // con la vieja». La cookie sigue firmada y sin caducar; lo que cambió es
        // el número que lleva dentro.
        let e = estado_con_usuario().await;
        let r = intentar(&e, "ada@x.co", CLAVE, "203.0.113.9").await;
        assert_eq!(r.status(), StatusCode::OK);
        let cookie = r
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .split_once('=')
            .unwrap()
            .1
            .to_string();

        assert!(actor_con(&e, &cookie).await.is_ok(), "la sesión recién abierta no valía");

        sqlx::query("UPDATE usuarios SET token_version = token_version + 1 WHERE id = 1")
            .execute(&e.bd)
            .await
            .unwrap();
        assert!(
            actor_con(&e, &cookie).await.is_err(),
            "subir token_version no echó a la sesión abierta: la revocación no revoca"
        );

        // Y la sesión NUEVA sí entra: la revocación no puede dejar fuera también
        // a quien vuelve a autenticarse, que sería un bloqueo con otro nombre.
        let r = intentar(&e, "ada@x.co", CLAVE, "10.0.0.5").await;
        assert_eq!(r.status(), StatusCode::OK);
        let nueva = r
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .split_once('=')
            .unwrap()
            .1
            .to_string();
        assert!(actor_con(&e, &nueva).await.is_ok(), "tras revocar no se puede volver a entrar");
    }

    #[tokio::test]
    async fn un_correo_imposiblemente_largo_no_llega_a_escribirse() {
        // Sin este corte, cada intento mete en la base el correo tecleado —hasta
        // los 2 MB que admite el cuerpo por defecto— sin necesidad de
        // autenticarse. No es un problema de criptografía: es llenar el disco del
        // servidor desde fuera, y se cierra donde se puede, en la puerta.
        let e = estado_con_usuario().await;
        let largo = format!("{}@x.co", "a".repeat(400));
        let r = intentar(&e, &largo, "loquesea", "203.0.113.9").await;
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);

        let filas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM intento_acceso")
            .fetch_one(&e.bd)
            .await
            .unwrap();
        assert_eq!(filas, 0, "una dirección imposible se guardó igual");
        let auditadas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auditoria")
            .fetch_one(&e.bd)
            .await
            .unwrap();
        assert_eq!(auditadas, 0, "una dirección imposible llegó a la auditoría");

        // Y una dirección larga PERO posible sigue entrando al circuito normal:
        // el corte rechaza lo imposible, no lo inusual.
        let posible = format!("{}@x.co", "a".repeat(60));
        assert_eq!(
            intentar(&e, &posible, "loquesea", "203.0.113.9").await.status(),
            StatusCode::UNAUTHORIZED
        );
        let filas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM intento_acceso")
            .fetch_one(&e.bd)
            .await
            .unwrap();
        assert_eq!(filas, 1, "el corte se llevó por delante una dirección legítima");
    }

    #[tokio::test]
    async fn sin_origen_el_login_falla_ruidosamente_y_no_pasa_de_largo() {
        // El camino que NO existe: si el origen no se puede saber, no se sigue
        // con un valor de relleno. Un freno que cuenta a todo el mundo en la
        // misma casilla deja que un desconocido eche al dueño de la cuenta.
        let e = estado_con_usuario().await;
        let credenciales =
            Credenciales { email: "ada@x.co".into(), clave: CLAVE.into() };
        let r = abrir_sesion(
            State(e.clone()),
            crate::origen::OrigenCliente(None),
            Json(credenciales),
        )
        .await;
        let Err(err) = r else {
            panic!("entró sin que se supiera de dónde venía");
        };
        assert_eq!(err.0, StatusCode::INTERNAL_SERVER_ERROR);
    }

    /// Directiva #8: 120 intentos concurrentes contra la misma pareja. Ninguno se
    /// pierde, no hay interbloqueo, y al final el freno está puesto.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn ciento_veinte_intentos_a_la_vez_dejan_el_freno_puesto() {
        let e = estado_con_usuario().await;
        let plazo = std::time::Duration::from_secs(120);

        let mut tareas = Vec::new();
        for _ in 0..120 {
            let bd = e.bd.clone();
            tareas.push(tokio::spawn(async move {
                crate::acceso::anotar(&bd, "ada@x.co", "203.0.113.9", false, chrono::Utc::now())
                    .await
            }));
        }
        for t in tareas {
            tokio::time::timeout(plazo, t)
                .await
                .expect("un intento no respondió a tiempo: ¿interbloqueo?")
                .expect("la tarea no debía entrar en panic")
                .expect("una anotación falló");
        }

        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM intento_acceso")
            .fetch_one(&e.bd)
            .await
            .unwrap();
        assert_eq!(n, 120, "se perdieron intentos bajo concurrencia");
        assert_eq!(
            intentar(&e, "ada@x.co", CLAVE, "203.0.113.9").await.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "120 fallos seguidos no dejaron el freno puesto"
        );
    }

    // --- «Cerrar sesión en todos los dispositivos» (#194) ---------------------
    //
    // Hasta esta tanda, `token_version` solo lo subía una prueba con un `UPDATE`
    // a mano: el mecanismo estaba entero y sin gatillo. Lo que se prueba aquí es
    // el gatillo, y sobre todo que apunte SOLO a quien lo aprieta.

    /// El valor de la cookie que trae una respuesta de login.
    fn cookie_de(r: &Response) -> String {
        r.headers()
            .get(header::SET_COOKIE)
            .expect("la respuesta no trae Set-Cookie")
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .split_once('=')
            .unwrap()
            .1
            .to_string()
    }

    /// Un segundo usuario, para poder comprobar que la revocación NO le llega.
    async fn anadir_segundo_usuario(e: &Estado) {
        use argon2::password_hash::{PasswordHasher, SaltString};
        let sal = SaltString::from_b64("c2Fsc2VndW5kb3VzdWFyaW9wcnU").unwrap();
        let hash = Argon2::default().hash_password(CLAVE.as_bytes(), &sal).unwrap().to_string();
        sqlx::query(
            "INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) \
             VALUES (2,'Bruno','bruno@x.co',?,'OPERADOR',1)",
        )
        .bind(&hash)
        .execute(&e.bd)
        .await
        .unwrap();
    }

    async fn salir_todo_con(e: &Estado, actor: Actor) -> Response {
        match salir_todo(State(e.clone()), actor).await {
            Ok(r) => r,
            Err(err) => err.into_response(),
        }
    }

    #[tokio::test]
    async fn cerrar_en_todos_los_dispositivos_echa_tambien_a_las_otras_sesiones() {
        // El caso que da nombre al botón: se entra desde tres sitios, se pulsa en
        // uno, y los tres quedan fuera. Si solo cayera el que pulsa, el botón
        // sería un «salir» con otro nombre — y quien lo pulsa porque cree que le
        // robaron el portátil se quedaría tranquilo sin motivo.
        let e = estado_con_usuario().await;
        let mut cookies = Vec::new();
        for origen in ["203.0.113.9", "198.51.100.4", "10.0.0.5"] {
            let r = intentar(&e, "ada@x.co", CLAVE, origen).await;
            assert_eq!(r.status(), StatusCode::OK);
            cookies.push(cookie_de(&r));
        }
        for (i, c) in cookies.iter().enumerate() {
            assert!(actor_con(&e, c).await.is_ok(), "la sesión {i} no valía ya antes de revocar");
        }

        let actor = actor_con(&e, &cookies[0]).await.unwrap();
        assert_eq!(salir_todo_con(&e, actor).await.status(), StatusCode::OK);

        for (i, c) in cookies.iter().enumerate() {
            assert!(
                actor_con(&e, c).await.is_err(),
                "la sesión {i} siguió valiendo tras «cerrar en todos los dispositivos»"
            );
        }
    }

    #[tokio::test]
    async fn cerrar_en_todos_los_dispositivos_no_toca_a_los_demas() {
        // La prueba que DISCRIMINA. Sin ella, un manejador que subiera la versión
        // de TODAS las filas —un `UPDATE` sin `WHERE`, que es un carácter de
        // distancia— pasaría la anterior tan campante, y el fallo se vería en
        // producción como «a todo el mundo se le cerró la sesión de golpe».
        let e = estado_con_usuario().await;
        anadir_segundo_usuario(&e).await;

        let de_ada = cookie_de(&intentar(&e, "ada@x.co", CLAVE, "203.0.113.9").await);
        let de_bruno = cookie_de(&intentar(&e, "bruno@x.co", CLAVE, "198.51.100.4").await);
        assert!(actor_con(&e, &de_bruno).await.is_ok(), "Bruno no llegó a entrar");

        let ada = actor_con(&e, &de_ada).await.unwrap();
        assert_eq!(salir_todo_con(&e, ada).await.status(), StatusCode::OK);

        assert!(actor_con(&e, &de_ada).await.is_err(), "Ada siguió dentro");
        assert!(
            actor_con(&e, &de_bruno).await.is_ok(),
            "revocar las sesiones de Ada echó también a Bruno: el UPDATE no lleva WHERE"
        );
    }

    #[tokio::test]
    async fn cerrar_en_todos_los_dispositivos_borra_la_cookie_de_este_navegador() {
        // La sesión ya no vale, pero el navegador la seguiría mandando: la
        // pantalla enseñaría a alguien dentro hasta que una petición fallara.
        let e = estado_con_usuario().await;
        let cookie = cookie_de(&intentar(&e, "ada@x.co", CLAVE, "203.0.113.9").await);
        let actor = actor_con(&e, &cookie).await.unwrap();

        let r = salir_todo_con(&e, actor).await;
        let set = r.headers().get(header::SET_COOKIE).expect("no borra la cookie de aquí");
        assert!(set.to_str().unwrap().contains("Max-Age=0"), "la cookie no se borra: {set:?}");
    }

    #[tokio::test]
    async fn sin_sesion_no_se_puede_cerrar_en_todos_los_dispositivos() {
        // El extractor es la única puerta. Que este extremo no reciba un id del
        // cliente es lo que impide usarlo para echar a otro, así que sin cookie
        // no hay a quién revocar.
        let e = estado_con_usuario().await;
        let req = axum::http::Request::builder().body(axum::body::Body::empty()).unwrap();
        let (mut parts, _) = req.into_parts();
        assert!(Actor::from_request_parts(&mut parts, &e).await.is_err());
    }

    #[tokio::test]
    async fn si_la_revocacion_no_toca_ninguna_fila_no_se_finge_el_cierre() {
        // Camino de error inyectado: un actor autenticado cuyo usuario ya no está
        // en la base (borrado entre la lectura de la cookie y esta escritura).
        // El `UPDATE` no falla —cero filas no es un error de SQL—, así que sin la
        // comprobación del booleano esto respondería «listo, cerré todas tus
        // sesiones» sin haber cerrado ninguna.
        let e = estado_con_usuario().await;
        let fantasma = Actor {
            id: 9999,
            nombre: "Nadie".into(),
            email: "nadie@x.co".into(),
            rol: crate::autz::Rol::Admin,
            stakeholder_id: None,
        };
        assert_eq!(
            salir_todo_con(&e, fantasma).await.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "se fingió un cierre de sesiones que no ocurrió"
        );
    }

    /// El extremo existe **en el router**, no solo como función.
    ///
    /// Las pruebas de arriba llaman a `salir_todo` directamente, y por eso no
    /// verían el fallo más fácil de cometer: que la línea del `.route(...)` no
    /// esté. Borrarla las dejaría a todas en verde y el botón daría 404 — el
    /// mismo patrón por el que un DELETE sin ruta pasó una revisión entera.
    /// Aquí entra una petición HTTP por la puerta y sale una respuesta.
    #[tokio::test]
    async fn el_extremo_de_salir_de_todo_esta_cableado_en_el_router() {
        use tower::ServiceExt;

        let e = estado_con_usuario().await;
        let cookie = cookie_de(&intentar(&e, "ada@x.co", CLAVE, "203.0.113.9").await);
        let app = router(e.clone());

        let peticion = |c: Option<&str>| {
            let mut b = axum::http::Request::builder().method("POST").uri("/api/salir-todo");
            if let Some(c) = c {
                b = b.header(header::COOKIE, format!("{}={c}", sesion::NOMBRE_COOKIE));
            }
            b.body(axum::body::Body::empty()).unwrap()
        };

        // Sin cookie: 401, y NO 404. La diferencia es la que importa — un 404
        // diría que la ruta no existe, y es justo lo que esta prueba vigila.
        let r = app.clone().oneshot(peticion(None)).await.unwrap();
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED, "¿está la ruta en el router?");

        let r = app.clone().oneshot(peticion(Some(&cookie))).await.unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        assert!(actor_con(&e, &cookie).await.is_err(), "la sesión sobrevivió a su propia revocación");
    }

    /// Directiva #8: 100 revocaciones concurrentes del mismo usuario.
    ///
    /// Sujeta la afirmación del doc-comment de `revocar_sesiones`: el `+1` lo hace
    /// SQLite dentro de la sentencia, no un número leído antes en Rust. Si se
    /// calculara aquí, dos revocaciones simultáneas leerían la misma versión y
    /// escribirían la misma, y el contador subiría menos de 100 — que es la forma
    /// que tiene esta carrera de aparecer: no un error, un incremento que se pierde.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn cien_revocaciones_a_la_vez_no_pierden_ni_un_incremento() {
        let e = estado_con_usuario().await;
        let cookie = cookie_de(&intentar(&e, "ada@x.co", CLAVE, "203.0.113.9").await);
        let antes: i64 = sqlx::query_scalar("SELECT token_version FROM usuarios WHERE id = 1")
            .fetch_one(&e.bd)
            .await
            .unwrap();

        let plazo = std::time::Duration::from_secs(120);
        let mut tareas = Vec::new();
        for _ in 0..100 {
            let bd = e.bd.clone();
            tareas.push(tokio::spawn(
                async move { crate::acceso::revocar_sesiones(&bd, 1).await },
            ));
        }
        for t in tareas {
            assert!(tokio::time::timeout(plazo, t)
                .await
                .expect("una revocación no respondió a tiempo: ¿interbloqueo?")
                .expect("la tarea no debía entrar en panic")
                .expect("una revocación falló"));
        }

        let despues: i64 = sqlx::query_scalar("SELECT token_version FROM usuarios WHERE id = 1")
            .fetch_one(&e.bd)
            .await
            .unwrap();
        assert_eq!(despues - antes, 100, "se perdieron incrementos bajo concurrencia");
        assert!(actor_con(&e, &cookie).await.is_err(), "la sesión sobrevivió a 100 revocaciones");
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

        let mut cfg = Config::de_prueba();
        cfg.clave_reposo = Some(frase.to_owned());
        let Documento::Claro(claro) = leer_en_claro(&ruta, true, &cfg).await else {
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
        let con = |f: Option<&str>| {
            let mut c = Config::de_prueba();
            c.clave_reposo = f.map(str::to_owned);
            c
        };
        assert_eq!(leer_en_claro(&ruta, true, &con(None)).await, Documento::NoDescifra);
        assert_eq!(
            leer_en_claro(&ruta, true, &con(Some("frase-equivocada"))).await,
            Documento::NoDescifra
        );
        assert_eq!(
            leer_en_claro(&dir.join("no-existe.bin"), true, &con(Some(frase))).await,
            Documento::NoEsta
        );
        assert_eq!(
            leer_en_claro(&ruta, true, &con(Some(frase))).await,
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
            leer_en_claro(&ruta, false, &{
                let mut c = Config::de_prueba();
                c.clave_reposo = Some("hay-frase-pero-la-fila-dice-claro".into());
                c
            })
            .await,
            Documento::Claro(b"documento viejo en claro".to_vec())
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn el_servidor_lleno_dice_ocupado_y_no_no_descifra() {
        // LA prueba de este cambio, y la que DISCRIMINA: con el semáforo agotado,
        // un archivo PERFECTO tiene que salir por `Ocupado`. Si saliera por
        // `NoDescifra`, `verificar` acusaría de alteración a una entrega intacta
        // solo porque el servidor tenía tráfico — la acusación falsa que el resto
        // de este módulo se dedica a evitar.
        let frase = "frase-de-reposo-correcta-y-larga-del-despliegue";
        let dir = dir_tmp("ocupado");
        let ruta = dir.join("e.bin");
        std::fs::write(&ruta, guaca::reposo::cifrar(b"documento intacto", frase)).unwrap();

        let mut cfg = Config::de_prueba();
        cfg.clave_reposo = Some(frase.to_owned());
        cfg.cripto = std::sync::Arc::new(tokio::sync::Semaphore::new(1));

        // Con el permiso libre, el mismo archivo se lee perfectamente. Sin esta
        // mitad, la prueba pasaría aunque `leer_en_claro` devolviera `Ocupado`
        // siempre.
        assert_eq!(
            leer_en_claro(&ruta, true, &cfg).await,
            Documento::Claro(b"documento intacto".to_vec()),
            "con turno libre el archivo se lee"
        );

        // Y ahora sin turno: se ocupa el único permiso y no se suelta.
        let ocupado = cfg.cripto.clone().acquire_owned().await.unwrap();
        let cfg_rapido = cfg.clone();
        // `tokio::time::pause` hace que el plazo de 30 s venza al instante en el
        // reloj virtual: la prueba mide la DECISIÓN, no la espera.
        tokio::time::pause();
        let leyendo = tokio::spawn(async move { leer_en_claro(&ruta, true, &cfg_rapido).await });
        tokio::time::advance(ESPERA_CRIPTO + std::time::Duration::from_secs(1)).await;
        assert_eq!(
            leyendo.await.unwrap(),
            Documento::Ocupado,
            "sin turno, un archivo intacto NO puede salir como «no descifra»"
        );
        drop(ocupado);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cortar_la_descarga_no_devuelve_el_turno_antes_que_la_memoria() {
        // El fallo que esto cierra, y que no se ve leyendo el código deprisa: si el
        // permiso se soltara al terminar el `await` de `spawn_blocking`, un cliente
        // que corta la conexión a mitad de descarga haría que axum tirase la función
        // —liberando el turno— mientras el Argon2id sigue corriendo con sus 64 MiB.
        // Repetido deprisa, el tope no acotaría nada. El turno tiene que soltarse
        // con la MEMORIA, no con la petición.
        let frase = "frase-de-reposo-correcta-y-larga-del-despliegue";
        let dir = dir_tmp("cortar");
        let ruta = dir.join("e.bin");
        std::fs::write(&ruta, guaca::reposo::cifrar(b"documento", frase)).unwrap();

        let mut cfg = Config::de_prueba();
        cfg.clave_reposo = Some(frase.to_owned());
        cfg.cripto = std::sync::Arc::new(tokio::sync::Semaphore::new(1));
        let sem = cfg.cripto.clone();

        let leyendo = tokio::spawn(async move { leer_en_claro(&ruta, true, &cfg).await });
        // Esperar a que el turno esté REALMENTE tomado: abortar antes no probaría
        // nada, porque no habría permiso que devolver.
        for _ in 0..2000 {
            if sem.available_permits() == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        assert_eq!(sem.available_permits(), 0, "el descifrado nunca tomó su turno");

        leyendo.abort();
        // Y ESPERAR a que el aborto de verdad ocurra. `abort()` solo lo señala: si
        // se comprueba el semáforo inmediatamente después, se mide la latencia de la
        // cancelación y la prueba pasa con el código roto — pasó, y por eso está
        // esta línea. Al `await` del handle abortado se vuelve cuando la función ya
        // fue tirada, que es el instante en que un permiso mal colocado se soltaría.
        let _ = leyendo.await;

        // El descifrado dura ~162 ms, así que sigue en marcha con su memoria. Si el
        // permiso viviera en la función tirada, ya estaría libre y el tope sería
        // decorativo.
        assert!(
            sem.try_acquire().is_err(),
            "abortar la petición devolvió el turno mientras el Argon2id seguía usando su memoria"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn sin_frase_no_se_consume_un_turno() {
        // Esperar un permiso para después descubrir que no hay frase gastaría el
        // turno de alguien que sí puede usarlo, y convertiría un despliegue mal
        // configurado en una cola.
        let dir = dir_tmp("sinfrase");
        let ruta = dir.join("e.bin");
        std::fs::write(&ruta, guaca::reposo::cifrar(b"x", "una-frase-larga-cualquiera-1234")).unwrap();

        let mut cfg = Config::de_prueba();
        cfg.clave_reposo = None;
        cfg.cripto = std::sync::Arc::new(tokio::sync::Semaphore::new(1));
        let retenido = cfg.cripto.clone().acquire_owned().await.unwrap();

        // Con CERO permisos disponibles responde igual y en el acto: si esperase
        // turno, esto colgaría los 30 s del plazo.
        assert_eq!(leer_en_claro(&ruta, true, &cfg).await, Documento::NoDescifra);
        drop(retenido);
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

    // --- Bitácora de acceso al documento (#186) ------------------------------

    /// Los detalles anotados bajo un evento, en orden. Se lee la TABLA, no el
    /// valor de retorno de `auditar`: lo que un ente de control abre es la base.
    async fn anotados(e: &Estado, evento: &str) -> Vec<String> {
        sqlx::query_scalar::<_, String>(
            "SELECT COALESCE(detalle, '') FROM auditoria WHERE evento = ? ORDER BY id",
        )
        .bind(evento)
        .fetch_all(&e.bd)
        .await
        .unwrap()
    }

    fn operador(id: i64) -> Actor {
        Actor {
            id,
            nombre: "Op".into(),
            email: format!("op{id}@x.co"),
            rol: Rol::Operador,
            stakeholder_id: None,
        }
    }

    #[tokio::test]
    async fn descargar_el_documento_deja_quien_lo_leyo_en_la_bitacora() {
        // La asimetría que esto cierra: subir se anotaba («entrega_registrada») y
        // bajar no. El producto vende rastro auditable, y quién SACÓ el documento
        // es justo lo que un ente de control pregunta.
        let (e, dir) = entrega_firmada_y_cifrada(Some(FRASE_REPOSO)).await;
        let r = descargar_archivo(State(e.clone()), auditor(), RutaParam(9)).await;
        assert!(r.is_ok(), "el documento tiene que salir para que esto mida algo");

        assert_eq!(anotados(&e, "entrega_descargada").await, vec!["9: e9.pdf".to_string()]);
        let quien: Vec<String> = sqlx::query_scalar(
            "SELECT email FROM auditoria WHERE evento = 'entrega_descargada'",
        )
        .fetch_all(&e.bd)
        .await
        .unwrap();
        assert_eq!(quien, vec!["adm@x.co".to_string()], "la bitácora sin quién no sirve");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn el_documento_que_no_sale_no_se_anota_como_descargado() {
        // El evento afirma que el documento SALIÓ. Sin la frase de reposo no sale
        // —`Documento::NoDescifra`—, y anotarlo igual pondría en el rastro una
        // lectura que nadie hizo: la bitácora dejaría de ser prueba de nada.
        let (e, dir) = entrega_firmada_y_cifrada(None).await;
        let r = descargar_archivo(State(e.clone()), auditor(), RutaParam(9)).await;
        assert!(r.is_err(), "sin la frase el documento no puede salir");
        assert!(anotados(&e, "entrega_descargada").await.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn se_anota_la_entrega_ajena_y_no_la_inexistente() {
        // La prueba de que DISCRIMINA, que es lo único que hace útil este evento.
        // La entrega 9 es del usuario 1; el operador 7 pide esa y una que no
        // existe. Las dos le contestan 404 idéntico, pero solo una dice algo.
        let (e, dir) = entrega_firmada_y_cifrada(Some(FRASE_REPOSO)).await;

        assert!(descargar_archivo(State(e.clone()), operador(7), RutaParam(9)).await.is_err());
        assert_eq!(
            anotados(&e, "entrega_ajena_solicitada").await,
            vec!["9".to_string()],
            "probar el número de otro es la señal que esta bitácora existe para dar"
        );

        // Un id que no existe es un enlace viejo o una entrega borrada. Si se
        // anotara, la bitácora se llenaría de falsos positivos hasta que nadie la
        // mirase (directiva 23).
        assert!(descargar_archivo(State(e.clone()), operador(7), RutaParam(404)).await.is_err());
        assert_eq!(anotados(&e, "entrega_ajena_solicitada").await, vec!["9".to_string()]);

        // Y el administrador, que lo ve todo, tampoco genera ruido: si su
        // consulta no encontró la entrega es que no existe, así que su 404 es un
        // 404 de verdad y anotarlo sería acusarle de sondear.
        assert!(descargar_archivo(State(e.clone()), auditor(), RutaParam(404)).await.is_err());
        assert_eq!(anotados(&e, "entrega_ajena_solicitada").await, vec!["9".to_string()]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn verificar_una_entrega_ajena_tambien_se_anota() {
        // Verificar lee la misma entrega por la misma puerta. Si solo se vigilara
        // la descarga, el que prueba números se pasaría a este extremo.
        let (e, dir) = entrega_firmada_y_cifrada(Some(FRASE_REPOSO)).await;
        assert!(verificar_entrega(State(e.clone()), operador(7), RutaParam(9)).await.is_err());
        assert_eq!(anotados(&e, "entrega_ajena_solicitada").await, vec!["9".to_string()]);

        assert!(verificar_entrega(State(e.clone()), operador(7), RutaParam(404)).await.is_err());
        assert_eq!(anotados(&e, "entrega_ajena_solicitada").await, vec!["9".to_string()]);

        // Y por la otra rama: `Alcance::Ninguna` sale antes de tocar la base, y
        // ese retorno temprano es un segundo sitio donde se olvida el registro.
        // Sin este caso, quitarlo de aquí no lo caza nadie — medido.
        let huerfano = Actor {
            id: 3,
            nombre: "Ter".into(),
            email: "ter@x.co".into(),
            rol: Rol::Tercero,
            stakeholder_id: None,
        };
        assert!(verificar_entrega(State(e.clone()), huerfano, RutaParam(9)).await.is_err());
        assert_eq!(
            anotados(&e, "entrega_ajena_solicitada").await,
            vec!["9".to_string(), "9".to_string()]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn un_tercero_sin_entidad_no_se_lleva_el_documento_ni_en_silencio() {
        // `Alcance::Ninguna` sale antes de tocar la base, y ese retorno temprano es
        // exactamente donde se olvida el registro.
        let (e, dir) = entrega_firmada_y_cifrada(Some(FRASE_REPOSO)).await;
        let huerfano = Actor {
            id: 3,
            nombre: "Ter".into(),
            email: "ter@x.co".into(),
            rol: Rol::Tercero,
            stakeholder_id: None,
        };
        assert!(descargar_archivo(State(e.clone()), huerfano, RutaParam(9)).await.is_err());
        assert_eq!(anotados(&e, "entrega_ajena_solicitada").await, vec!["9".to_string()]);
        assert!(anotados(&e, "entrega_descargada").await.is_empty());
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
