//! Administración de cuentas: alta, rol, activación y contraseña.
//!
//! # Por qué existe
//!
//! Hasta ahora la tabla `usuarios` solo se escribía desde el subcomando
//! `importar` (el volcado de la base anterior) y desde el `UPDATE` de
//! `token_version`. Dar de alta a un empleado, cambiarle el rol, desactivarlo o
//! cambiarle la contraseña exigía abrir el SQLite del cliente con `sqlite3` y
//! escribir SQL a mano. Un producto que se instala en casa de alguien y pide eso
//! para una tarea semanal no está terminado, por muy verde que esté el CI.
//!
//! De paso cierra el caso de uso que la revocación selectiva (#194) tenía
//! inalcanzable: «acabo de cambiar la contraseña, echa a quien entrara con la
//! vieja» no se podía dar, porque no había cambio de contraseña.
//!
//! # Por qué «cuentas» y no «usuarios», siendo la misma tabla
//!
//! Porque son dos vocabularios sobre la misma fila y el valor lo fija quien ya
//! lee. `/api/usuarios` existe desde el principio y significa **«quién puede ser
//! parte de un flujo»**: devuelve id, nombre y correo de los activos, y lo
//! consume el selector de partes. Cambiar ese extremo para que sirva también a la
//! administración habría obligado a que el selector empezara a recibir cuentas
//! desactivadas y roles que no le importan.
//!
//! Así que la administración tiene su propio nombre y su propio recurso. La tabla
//! es una; lo que cada extremo promete, no.

use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::autz::{Actor, Rol};
use crate::rutas::{auditar, Error, Estado, ESPERA_CRIPTO};
use crate::sesion;

/// Una cuenta como la ve la administración. **No lleva `clave_hash`**, y la
/// consulta ni siquiera lo selecciona: lo que no se trae no se puede filtrar por
/// descuido al añadir un campo más adelante.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FichaCuenta {
    pub id: i64,
    pub nombre: String,
    pub email: String,
    pub rol: String,
    pub activo: bool,
    pub stakeholder_id: Option<i64>,
}

const COLUMNAS: &str = "id, nombre, email, rol, activo, stakeholder_id";

#[derive(Deserialize)]
pub struct CuentaNueva {
    nombre: String,
    email: String,
    clave: String,
    rol: String,
    #[serde(default)]
    stakeholder_id: Option<i64>,
}

#[derive(Deserialize)]
pub struct CuentaCambio {
    nombre: String,
    rol: String,
    activo: bool,
    #[serde(default)]
    stakeholder_id: Option<i64>,
}

#[derive(Deserialize)]
pub struct ClaveDeOtro {
    clave_nueva: String,
}

#[derive(Deserialize)]
pub struct MiClave {
    clave_actual: String,
    clave_nueva: String,
}

// --- Guardas ------------------------------------------------------------------

/// Administrar cuentas es de administradores, y de nadie más.
///
/// Supervisor NO entra, aunque en el resto del sistema vea lo mismo que un
/// administrador: su alcance es `Todas` para LEER entregas. Poder crear cuentas
/// y repartir roles es otra cosa, y darlo por equivalente es cómo un rol de
/// consulta acaba pudiendo fabricarse un administrador.
fn solo_admin(actor: &Actor) -> Result<(), Error> {
    if matches!(actor.rol, Rol::Admin) {
        Ok(())
    } else {
        Err(Error(StatusCode::FORBIDDEN, "no autorizado".into()))
    }
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

/// El correo se normaliza a minúsculas porque es la CLAVE de acceso: el login
/// busca por él, y `Ana@x.co` y `ana@x.co` tienen que ser la misma cuenta o el
/// `UNIQUE` del esquema deja crear dos que compiten por el mismo inicio de sesión.
///
/// No se valida la forma más allá de eso. Comprobar que un correo «parece un
/// correo» con una regla propia es la vía rápida a rechazar direcciones legítimas
/// —subdominios, `+etiqueta`, TLD nuevos— y aquí el correo no se usa para nada
/// que dependa de su sintaxis: se compara consigo mismo.
fn email_valido(email: &str) -> Result<String, Error> {
    let e = email.trim().to_lowercase();
    if e.is_empty() {
        return Err(Error(StatusCode::BAD_REQUEST, "el correo no puede estar vacío".into()));
    }
    // El mismo tope que el login: por encima de esto no se llega ni a escribir.
    if e.len() > 320 {
        return Err(Error(StatusCode::BAD_REQUEST, "el correo es demasiado largo".into()));
    }
    if !e.contains('@') {
        return Err(Error(StatusCode::BAD_REQUEST, "el correo tiene que llevar «@»".into()));
    }
    Ok(e)
}

/// El mínimo es **8**, que es el suelo de la NIST SP 800-63B, y no un número
/// mayor elegido por prudencia.
///
/// Subirlo «por si acaso» es el sesgo a cerrar de la directiva 21: rechazaría
/// contraseñas legítimas de clientes reales sin que nadie haya medido que eso
/// mejore nada. Lo que sí importa —y ya está— es que el hash sea Argon2id y que
/// el login tenga freno; la longitud sola no sustituye a ninguno de los dos.
///
/// El tope existe para que nadie mande un megabyte a hashear.
fn clave_valida(clave: &str) -> Result<String, Error> {
    if clave.chars().count() < 8 {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "la contraseña tiene que tener al menos 8 caracteres".into(),
        ));
    }
    if clave.len() > 256 {
        return Err(Error(StatusCode::BAD_REQUEST, "la contraseña es demasiado larga".into()));
    }
    Ok(clave.to_string())
}

/// Rol y entidad externa tienen que decir lo mismo que significan.
///
/// `Actor::alcance` solo mira `stakeholder_id` cuando el rol es `Tercero`. Un
/// operador con entidad vinculada guarda un dato que nadie lee jamás y que la
/// pantalla enseñará como si contara: dice algo distinto de lo que significa, que
/// es justo lo que la directiva 21 manda rechazar.
///
/// Al revés NO se rechaza: un tercero SIN entidad es legítimo —su alcance es
/// `Ninguna`, un valor con nombre propio y con su prueba— y puede ser un alta a
/// medias que se completa después.
fn rol_y_entidad(rol: Rol, stakeholder_id: Option<i64>) -> Result<(), Error> {
    if !matches!(rol, Rol::Tercero) && stakeholder_id.is_some() {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "solo un TERCERO puede tener entidad externa vinculada".into(),
        ));
    }
    Ok(())
}

fn rol_valido(texto: &str) -> Result<Rol, Error> {
    Rol::desde_bd(texto).ok_or_else(|| {
        Error(StatusCode::BAD_REQUEST, format!("«{texto}» no es un rol del sistema"))
    })
}

// --- El turno de cripto -------------------------------------------------------

/// Corre un Argon2 fuera del hilo del runtime y CON TURNO.
///
/// Es la misma disciplina que el login y que el descifrado en reposo, y por la
/// misma razón: cada operación reserva memoria de verdad (19 MiB el de acceso), y
/// dejarla suelta en el hilo del runtime bloquea el servidor entero. Que aquí
/// haga falta sesión no lo hace gratis — un operador autenticado que repita el
/// cambio de contraseña pagaría lo mismo que un atacante.
async fn con_turno<T, F>(e: &Estado, que: &str, trabajo: F) -> Result<T, Error>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let permiso = match tokio::time::timeout(ESPERA_CRIPTO, e.cfg.cripto.clone().acquire_owned())
        .await
    {
        Ok(Ok(p)) => p,
        _ => {
            tracing::warn!("{que}: sin turno de cripto en {} s", ESPERA_CRIPTO.as_secs());
            // 503 y no un error sobre la contraseña: habla del servidor, no de
            // la clave de nadie.
            return Err(Error(
                StatusCode::SERVICE_UNAVAILABLE,
                "el servidor está ocupado; reinténtelo".into(),
            ));
        }
    };
    tokio::task::spawn_blocking(move || {
        let r = trabajo();
        drop(permiso); // el turno se libera con la memoria, no antes
        r
    })
    .await
    .map_err(|_| {
        tracing::error!("{que}: la tarea de cripto no terminó");
        Error(StatusCode::INTERNAL_SERVER_ERROR, "error interno".into())
    })
}

/// El hash sale del MISMO Argon2 que después verifica el acceso.
///
/// No es una preferencia: un hash producido por otra herramienta con otros
/// parámetros deja a esa persona fuera del sistema, y el fallo aparece lejos de
/// su causa —el alta termina bien, anuncia unas credenciales, y no se puede
/// entrar con ellas—. Ya pasó una vez en la versión de Python; por eso el
/// subcomando `hashear` existe y por eso esto no lo reimplementa.
async fn hashear(e: &Estado, clave: String) -> Result<String, Error> {
    let hecho = con_turno(e, "hashear", move || {
        let sal = SaltString::generate(&mut OsRng);
        Argon2::default().hash_password(clave.as_bytes(), &sal).map(|h| h.to_string())
    })
    .await?;
    hecho.map_err(|err| {
        tracing::error!("no se pudo cifrar la clave: {err}");
        Error(StatusCode::INTERNAL_SERVER_ERROR, "error interno".into())
    })
}

async fn clave_correcta(e: &Estado, hash_texto: String, clave: String) -> Result<bool, Error> {
    con_turno(e, "verificar clave", move || {
        PasswordHash::new(&hash_texto)
            .map(|h| Argon2::default().verify_password(clave.as_bytes(), &h).is_ok())
            .unwrap_or(false)
    })
    .await
}

// --- Extremos -----------------------------------------------------------------

/// `GET /api/cuentas` — todas, **incluidas las desactivadas**.
///
/// El selector de partes (`/api/usuarios`) filtra por `activo = 1` porque a un
/// flujo no se le asigna alguien que ya no está. Aquí es al revés: si la
/// administración no viera las desactivadas, reactivar a alguien exigiría el
/// `sqlite3` que esta pantalla viene a quitar.
pub async fn listar(State(e): State<Estado>, actor: Actor) -> Result<Json<Vec<FichaCuenta>>, Error> {
    solo_admin(&actor)?;
    let filas = sqlx::query_as::<_, FichaCuenta>(&format!(
        "SELECT {COLUMNAS} FROM usuarios ORDER BY activo DESC, nombre"
    ))
    .fetch_all(&e.bd)
    .await?;
    Ok(Json(filas))
}

/// `POST /api/cuentas` — alta.
pub async fn crear(
    State(e): State<Estado>,
    actor: Actor,
    Json(nueva): Json<CuentaNueva>,
) -> Result<Response, Error> {
    solo_admin(&actor)?;
    let nombre = nombre_valido(&nueva.nombre)?;
    let email = email_valido(&nueva.email)?;
    let clave = clave_valida(&nueva.clave)?;
    let rol = rol_valido(&nueva.rol)?;
    rol_y_entidad(rol, nueva.stakeholder_id)?;

    let hash = hashear(&e, clave).await?;

    let fila = sqlx::query_as::<_, FichaCuenta>(&format!(
        "INSERT INTO usuarios (nombre, email, clave_hash, rol, activo, stakeholder_id) \
         VALUES (?, ?, ?, ?, 1, ?) RETURNING {COLUMNAS}"
    ))
    .bind(&nombre)
    .bind(&email)
    .bind(&hash)
    .bind(rol.a_bd())
    .bind(nueva.stakeholder_id)
    .fetch_one(&e.bd)
    .await
    .map_err(correo_repetido)?;

    auditar(&e.bd, "cuenta_creada", Some(&actor.email), Some(&email)).await;
    Ok((StatusCode::CREATED, Json(fila)).into_response())
}

/// El `UNIQUE(email)` contestado con un 409 legible en vez de un 500.
///
/// Sin esto, dar de alta un correo que ya existe le sale al administrador como
/// «error interno», que es la respuesta que hace pensar que el sistema está roto
/// cuando lo único que pasa es que esa persona ya está.
fn correo_repetido(err: sqlx::Error) -> Error {
    let repetido = matches!(&err, sqlx::Error::Database(d) if d.message().contains("UNIQUE"));
    if repetido {
        Error(StatusCode::CONFLICT, "ya existe una cuenta con ese correo".into())
    } else {
        err.into()
    }
}

/// `PUT /api/cuentas/{id}` — nombre, rol, activación y entidad.
///
/// La contraseña NO se toca aquí: tiene su propio extremo porque cuesta un Argon2
/// y porque revoca sesiones, y mezclarla con un cambio de nombre haría que
/// renombrar a alguien le cerrara la sesión sin que nadie lo hubiera pedido.
pub async fn actualizar(
    State(e): State<Estado>,
    actor: Actor,
    Path(id): Path<i64>,
    Json(cambio): Json<CuentaCambio>,
) -> Result<Json<FichaCuenta>, Error> {
    solo_admin(&actor)?;
    let nombre = nombre_valido(&cambio.nombre)?;
    let rol = rol_valido(&cambio.rol)?;
    rol_y_entidad(rol, cambio.stakeholder_id)?;

    // Todo en una transacción: la comprobación del último administrador no vale
    // nada si entre contar y escribir cabe otra petición. Dos administradores
    // degradándose a la vez leerían «queda otro» los dos y el sistema se quedaría
    // sin ninguno.
    let mut tx = e.bd.begin().await?;

    let actual: Option<(String, i64)> =
        sqlx::query_as("SELECT rol, activo FROM usuarios WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((rol_actual, activo_actual)) = actual else {
        return Err(Error(StatusCode::NOT_FOUND, "esa cuenta no existe".into()));
    };

    let era_admin_activo = rol_actual == Rol::Admin.a_bd() && activo_actual != 0;
    let sigue_admin_activo = matches!(rol, Rol::Admin) && cambio.activo;
    if era_admin_activo && !sigue_admin_activo {
        ultimo_administrador(&mut tx, id).await?;
    }

    let fila = sqlx::query_as::<_, FichaCuenta>(&format!(
        "UPDATE usuarios SET nombre = ?, rol = ?, activo = ?, stakeholder_id = ? \
         WHERE id = ? RETURNING {COLUMNAS}"
    ))
    .bind(&nombre)
    .bind(rol.a_bd())
    .bind(i64::from(cambio.activo))
    .bind(cambio.stakeholder_id)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    auditar(&e.bd, "cuenta_actualizada", Some(&actor.email), Some(&fila.email)).await;
    Ok(Json(fila))
}

/// Se niega a dejar el sistema sin administradores activos.
///
/// Es un **imposible**, no un inusual, y por eso se rechaza (directiva 21): no
/// hay recuperación dentro del producto. Sin un administrador no se puede
/// ascender a nadie, así que la única salida sería abrir el SQLite del cliente a
/// mano — exactamente lo que este módulo existe para no tener que hacer.
async fn ultimo_administrador(
    tx: &mut sqlx::SqliteConnection,
    excepto: i64,
) -> Result<(), Error> {
    let quedan: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM usuarios WHERE rol = ? AND activo = 1 AND id <> ?",
    )
    .bind(Rol::Admin.a_bd())
    .bind(excepto)
    .fetch_one(&mut *tx)
    .await?;
    if quedan == 0 {
        return Err(Error(
            StatusCode::CONFLICT,
            "es el último administrador activo: ascienda a otro antes de quitarle el papel".into(),
        ));
    }
    Ok(())
}

/// `POST /api/cuentas/{id}/clave` — restablecer la de OTRA persona.
///
/// Sube `token_version` en la misma transacción, así que quien estuviera dentro
/// con la contraseña vieja cae en su siguiente petición. **No reemite cookie**:
/// el afectado no es quien llama.
pub async fn restablecer_clave(
    State(e): State<Estado>,
    actor: Actor,
    Path(id): Path<i64>,
    Json(cuerpo): Json<ClaveDeOtro>,
) -> Result<Json<serde_json::Value>, Error> {
    solo_admin(&actor)?;
    if id == actor.id {
        // No es una restricción: es mandar al camino correcto. Cambiar la propia
        // por aquí saltaría la comprobación de la contraseña actual, y además
        // dejaría al administrador con la sesión muerta sin avisarle.
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "para cambiar su propia contraseña use «cambiar mi contraseña»".into(),
        ));
    }
    let clave = clave_valida(&cuerpo.clave_nueva)?;
    let hash = hashear(&e, clave).await?;

    let email: Option<String> = sqlx::query_scalar(
        "UPDATE usuarios SET clave_hash = ?, token_version = token_version + 1 \
         WHERE id = ? RETURNING email",
    )
    .bind(&hash)
    .bind(id)
    .fetch_optional(&e.bd)
    .await?;

    // Cero filas no es un error de SQL. Sin mirarlo, restablecer la contraseña de
    // una cuenta que no existe respondería «hecho».
    let Some(email) = email else {
        return Err(Error(StatusCode::NOT_FOUND, "esa cuenta no existe".into()));
    };

    auditar(&e.bd, "clave_restablecida", Some(&actor.email), Some(&email)).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `POST /api/mi-clave` — cambiar la propia. Cualquier rol.
///
/// # Las dos cosas que hace y por qué van juntas
///
/// Sube `token_version` —o sea, echa a todas las sesiones abiertas de esta
/// persona— **y reemite la cookie de aquí con la versión nueva**. El resultado es
/// el que se espera: quien cambia su contraseña sigue dentro en el navegador
/// donde la cambió, y cae en todos los demás.
///
/// Es el caso de uso que da sentido a la columna (#194) y hasta ahora era
/// inalcanzable. Sin el bump, cambiar la contraseña no echaría a quien entró con
/// la vieja, que es justo de lo que uno se quiere proteger al cambiarla; sin la
/// reemisión, cambiarla te expulsaría a ti, y nadie usa un botón que le echa.
pub async fn cambiar_mi_clave(
    State(e): State<Estado>,
    actor: Actor,
    Json(cuerpo): Json<MiClave>,
) -> Result<Response, Error> {
    let nueva = clave_valida(&cuerpo.clave_nueva)?;

    let hash_actual: String = sqlx::query_scalar("SELECT clave_hash FROM usuarios WHERE id = ?")
        .bind(actor.id)
        .fetch_optional(&e.bd)
        .await?
        .ok_or_else(|| Error(StatusCode::NOT_FOUND, "esa cuenta no existe".into()))?;

    // La contraseña ACTUAL se comprueba siempre. Sin esto, una sesión robada
    // —una cookie prestada, un navegador abierto— podría cambiar la contraseña y
    // quedarse con la cuenta para siempre, que es la escalada que este extremo
    // no puede permitir por muy autenticado que esté quien pide.
    if !clave_correcta(&e, hash_actual, cuerpo.clave_actual.clone()).await? {
        auditar(&e.bd, "clave_propia_rechazada", Some(&actor.email), None).await;
        return Err(Error(StatusCode::UNAUTHORIZED, "la contraseña actual no es correcta".into()));
    }

    let hash = hashear(&e, nueva).await?;

    // La escritura del hash y el salto de versión, en la misma sentencia: si
    // fueran dos, entre ellas habría un instante con la contraseña nueva puesta y
    // las sesiones viejas todavía válidas.
    let version: Option<i64> = sqlx::query_scalar(
        "UPDATE usuarios SET clave_hash = ?, token_version = token_version + 1 \
         WHERE id = ? RETURNING token_version",
    )
    .bind(&hash)
    .bind(actor.id)
    .fetch_optional(&e.bd)
    .await?;
    let Some(version) = version else {
        tracing::error!("cambio de clave sin efecto: el usuario {} ya no existe", actor.id);
        return Err(Error(StatusCode::INTERNAL_SERVER_ERROR, "error interno".into()));
    };

    auditar(&e.bd, "clave_cambiada", Some(&actor.email), None).await;

    let valor = sesion::emitir(&e.cfg.secreto_sesion, actor.id, version, e.cfg.minutos_sesion);
    let mut cabeceras = HeaderMap::new();
    cabeceras.insert(
        header::SET_COOKIE,
        sesion::cabecera_abrir(&valor, e.cfg.minutos_sesion)
            .parse()
            .expect("la cabecera es ASCII"),
    );
    Ok((cabeceras, Json(serde_json::json!({ "ok": true }))).into_response())
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use serde_json::Value;
    use tower::ServiceExt;

    const CLAVE: &str = "la-clave-de-la-prueba";

    /// Un servidor con un administrador (1) y un operador (2). Las claves salen
    /// del mismo Argon2 que verifica el acceso: si saliera de otro sitio, la
    /// prueba de que se puede entrar con la cuenta nueva no probaría nada.
    async fn servidor() -> (axum::Router, crate::rutas::Estado) {
        let bd = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&bd).await.unwrap();
        let sal = SaltString::from_b64("c2FsZGVsYXNwcnVlYmFzZGVjdWVudA").unwrap();
        let hash = Argon2::default().hash_password(CLAVE.as_bytes(), &sal).unwrap().to_string();
        sqlx::query(
            "INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) VALUES \
             (1,'Ada','ada@x.co',?,'ADMIN',1), (2,'Bruno','bruno@x.co',?,'OPERADOR',1)",
        )
        .bind(&hash)
        .bind(&hash)
        .execute(&bd)
        .await
        .unwrap();

        let estado = crate::rutas::Estado {
            bd,
            cfg: crate::config::Config::para_pruebas(),
            motor: std::sync::Arc::new(siger_core::Motor::nuevo(
                siger_core::Calendario::de_pais("CO", 2024, 2030).unwrap(),
            )),
        };
        (crate::rutas::router(estado.clone()), estado)
    }

    fn peticion(metodo: &str, uri: &str, cookie: Option<&str>, cuerpo: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method(metodo).uri(uri).extension(
            axum::extract::ConnectInfo("127.0.0.1:5555".parse::<std::net::SocketAddr>().unwrap()),
        );
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, c);
        }
        match cuerpo {
            Some(j) => b
                .header("content-type", "application/json")
                .body(Body::from(j.to_string()))
                .unwrap(),
            None => b.body(Body::empty()).unwrap(),
        }
    }

    async fn llamar(app: &axum::Router, r: Request<Body>) -> (StatusCode, Value) {
        let res = app.clone().oneshot(r).await.unwrap();
        let estado = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
        (estado, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn entrar(app: &axum::Router, email: &str, clave: &str) -> Option<String> {
        let res = app
            .clone()
            .oneshot(peticion(
                "POST",
                "/api/sesion",
                None,
                Some(&format!(r#"{{"email":"{email}","clave":"{clave}"}}"#)),
            ))
            .await
            .unwrap();
        if res.status() != StatusCode::OK {
            return None;
        }
        Some(
            res.headers()
                .get(header::SET_COOKIE)?
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_string(),
        )
    }

    /// ¿Sigue valiendo esta cookie? `/api/yo` es el extremo más barato que la
    /// ejerce entera: firma, relectura del usuario y comparación de versión.
    async fn sigue_dentro(app: &axum::Router, cookie: &str) -> bool {
        app.clone()
            .oneshot(peticion("GET", "/api/yo", Some(cookie), None))
            .await
            .unwrap()
            .status()
            == StatusCode::OK
    }

    // --- Quién puede administrar ---------------------------------------------

    #[tokio::test]
    async fn un_operador_no_administra_cuentas() {
        let (app, _) = servidor().await;
        let c = entrar(&app, "bruno@x.co", CLAVE).await.unwrap();
        let alta = r#"{"nombre":"X","email":"x@x.co","clave":"clave-larga-1","rol":"ADMIN"}"#;
        for (m, u, cuerpo) in [
            ("GET", "/api/cuentas", None),
            ("POST", "/api/cuentas", Some(alta)),
            ("PUT", "/api/cuentas/1", Some(r#"{"nombre":"X","rol":"TERCERO","activo":false}"#)),
            ("POST", "/api/cuentas/1/clave", Some(r#"{"clave_nueva":"otra-clave-larga"}"#)),
        ] {
            let (s, _) = llamar(&app, peticion(m, u, Some(&c), cuerpo)).await;
            assert_eq!(s, StatusCode::FORBIDDEN, "un operador pasó por {m} {u}");
        }
    }

    #[tokio::test]
    async fn un_supervisor_tampoco_administra_aunque_lo_vea_todo() {
        // Explícita porque es la confusión fácil: el alcance de un supervisor es
        // `Todas`, igual que el de un administrador. Pero eso es para LEER
        // entregas. Si administrar cuentas se colgara de `puede_leer`, un rol de
        // consulta podría fabricarse un administrador y salir por la puerta.
        let (app, estado) = servidor().await;
        sqlx::query("UPDATE usuarios SET rol = 'SUPERVISOR' WHERE id = 2")
            .execute(&estado.bd)
            .await
            .unwrap();
        let c = entrar(&app, "bruno@x.co", CLAVE).await.unwrap();
        let (s, _) = llamar(&app, peticion("GET", "/api/cuentas", Some(&c), None)).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn sin_sesion_la_administracion_es_401_y_no_404() {
        // Que las cinco rutas ESTÉN en el router. Las pruebas que llaman al
        // manejador no lo verían: borrar un `.route(...)` las dejaría verdes y el
        // extremo daría 404. Un 401 dice «existe y te rechaza»; un 404, «no está».
        let (app, _) = servidor().await;
        for (m, u) in [
            ("GET", "/api/cuentas"),
            ("POST", "/api/cuentas"),
            ("PUT", "/api/cuentas/1"),
            ("POST", "/api/cuentas/1/clave"),
            ("POST", "/api/mi-clave"),
        ] {
            let res = app.clone().oneshot(peticion(m, u, None, Some("{}"))).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "¿está {m} {u} en el router?");
        }
    }

    // --- Alta ------------------------------------------------------------------

    #[tokio::test]
    async fn con_la_cuenta_recien_creada_se_puede_entrar() {
        // La prueba que de verdad importa del alta, y la que solo se ve cruzando
        // el sistema entero: el hash tiene que salir del MISMO Argon2 que después
        // verifica el acceso. Si no, el alta termina bien, anuncia unas
        // credenciales y no se puede entrar con ellas — y el fallo aparece lejos
        // de su causa. Ya pasó una vez en la versión de Python.
        let (app, _) = servidor().await;
        let admin = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        let (s, cuerpo) = llamar(
            &app,
            peticion(
                "POST",
                "/api/cuentas",
                Some(&admin),
                Some(r#"{"nombre":"Celia","email":"Celia@X.co","clave":"su-clave-nueva","rol":"OPERADOR"}"#),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{cuerpo}");
        // El correo se guardó en minúsculas: es la clave de acceso y `Celia@X.co`
        // tiene que ser la misma cuenta que `celia@x.co`.
        assert_eq!(cuerpo["email"], "celia@x.co");
        assert!(entrar(&app, "celia@x.co", "su-clave-nueva").await.is_some(), "no se pudo entrar");
    }

    #[tokio::test]
    async fn la_administracion_no_devuelve_el_hash_de_nadie() {
        let (app, _) = servidor().await;
        let admin = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        let (_, lista) = llamar(&app, peticion("GET", "/api/cuentas", Some(&admin), None)).await;
        let texto = lista.to_string();
        assert!(!texto.contains("clave_hash"), "la lista trae el campo del hash");
        assert!(!texto.contains("$argon2"), "la lista trae un hash de verdad: {texto}");
    }

    #[tokio::test]
    async fn un_correo_repetido_es_409_y_no_error_interno() {
        let (app, _) = servidor().await;
        let admin = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        let (s, _) = llamar(
            &app,
            peticion(
                "POST",
                "/api/cuentas",
                Some(&admin),
                Some(r#"{"nombre":"Otra Ada","email":"ada@x.co","clave":"clave-bastante-larga","rol":"OPERADOR"}"#),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::CONFLICT, "un correo repetido salió como otra cosa");
    }

    #[tokio::test]
    async fn el_alta_rechaza_lo_que_no_se_puede_guardar() {
        let (app, _) = servidor().await;
        let admin = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        for (que, cuerpo) in [
            ("rol inventado", r#"{"nombre":"N","email":"a@b.co","clave":"clave-larga-11","rol":"JEFE"}"#),
            ("nombre vacío", r#"{"nombre":"  ","email":"a@b.co","clave":"clave-larga-11","rol":"OPERADOR"}"#),
            ("clave corta", r#"{"nombre":"N","email":"a@b.co","clave":"corta","rol":"OPERADOR"}"#),
            ("correo sin arroba", r#"{"nombre":"N","email":"nada","clave":"clave-larga-11","rol":"OPERADOR"}"#),
            // Entidad externa en quien no la usa: `Actor::alcance` solo la mira
            // para un TERCERO, así que aquí diría algo distinto de lo que significa.
            ("entidad en un operador", r#"{"nombre":"N","email":"a@b.co","clave":"clave-larga-11","rol":"OPERADOR","stakeholder_id":1}"#),
        ] {
            let (s, _) = llamar(&app, peticion("POST", "/api/cuentas", Some(&admin), Some(cuerpo))).await;
            assert_eq!(s, StatusCode::BAD_REQUEST, "se aceptó: {que}");
        }
    }

    // --- El último administrador ----------------------------------------------

    #[tokio::test]
    async fn no_se_puede_dejar_el_sistema_sin_administradores() {
        // Es un IMPOSIBLE, no un inusual: sin administrador no se puede ascender
        // a nadie, y la única salida sería el `sqlite3` que este módulo viene a
        // quitar. Las dos formas de llegar ahí —degradar y desactivar— se prueban
        // por separado porque son dos ramas distintas de la misma guarda.
        let (app, _) = servidor().await;
        let admin = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        for (que, cuerpo) in [
            ("degradarlo", r#"{"nombre":"Ada","rol":"OPERADOR","activo":true}"#),
            ("desactivarlo", r#"{"nombre":"Ada","rol":"ADMIN","activo":false}"#),
        ] {
            let (s, _) = llamar(&app, peticion("PUT", "/api/cuentas/1", Some(&admin), Some(cuerpo))).await;
            assert_eq!(s, StatusCode::CONFLICT, "se pudo {que} siendo el último administrador");
        }

        // Y la mitad que evita que esto pase por la razón equivocada: con OTRO
        // administrador activo, degradarse SÍ se puede. Sin esto, una guarda que
        // rechazara siempre pasaría la prueba de arriba.
        let (s, _) = llamar(
            &app,
            peticion(
                "POST",
                "/api/cuentas",
                Some(&admin),
                Some(r#"{"nombre":"Dos","email":"dos@x.co","clave":"clave-larga-22","rol":"ADMIN"}"#),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
        let (s, _) = llamar(
            &app,
            peticion("PUT", "/api/cuentas/1", Some(&admin), Some(r#"{"nombre":"Ada","rol":"OPERADOR","activo":true}"#)),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "con otro administrador activo, degradarse tiene que poder");
    }

    // --- Contraseñas -----------------------------------------------------------

    #[tokio::test]
    async fn cambiar_mi_clave_echa_a_las_otras_sesiones_y_deja_esta_dentro() {
        // El caso que da sentido a `token_version` (#194) y que hasta ahora era
        // inalcanzable, porque no había cambio de contraseña. Las dos mitades
        // importan: si no echara a las otras, cambiar la clave no serviría de
        // nada contra quien ya entró; si echara también a esta, nadie usaría un
        // botón que le expulsa.
        let (app, _) = servidor().await;
        let aqui = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        let otro_sitio = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        assert!(sigue_dentro(&app, &otro_sitio).await);

        let res = app
            .clone()
            .oneshot(peticion(
                "POST",
                "/api/mi-clave",
                Some(&aqui),
                Some(&format!(r#"{{"clave_actual":"{CLAVE}","clave_nueva":"la-nueva-larga"}}"#)),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let renovada = res
            .headers()
            .get(header::SET_COOKIE)
            .expect("no reemitió la cookie: quien cambia su clave se quedaría fuera")
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();

        assert!(!sigue_dentro(&app, &otro_sitio).await, "la otra sesión sobrevivió al cambio");
        assert!(!sigue_dentro(&app, &aqui).await, "la cookie VIEJA de aquí tendría que morir también");
        assert!(sigue_dentro(&app, &renovada).await, "la cookie reemitida no vale");
        assert!(entrar(&app, "ada@x.co", "la-nueva-larga").await.is_some(), "la clave nueva no entra");
        assert!(entrar(&app, "ada@x.co", CLAVE).await.is_none(), "la clave VIEJA sigue entrando");
    }

    #[tokio::test]
    async fn sin_la_contrasena_actual_no_se_cambia_la_propia() {
        // Una sesión robada —cookie prestada, navegador abierto— no puede
        // quedarse con la cuenta para siempre. Es la escalada que este extremo no
        // puede permitir por muy autenticado que esté quien pide.
        let (app, _) = servidor().await;
        let c = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        let (s, _) = llamar(
            &app,
            peticion(
                "POST",
                "/api/mi-clave",
                Some(&c),
                Some(r#"{"clave_actual":"no-es-la-suya","clave_nueva":"la-nueva-larga"}"#),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        assert!(sigue_dentro(&app, &c).await, "un intento fallido no puede tumbar la sesión");
        assert!(entrar(&app, "ada@x.co", CLAVE).await.is_some(), "la clave cambió pese al rechazo");
    }

    #[tokio::test]
    async fn restablecer_la_de_otro_lo_echa_a_el_y_no_al_administrador() {
        let (app, _) = servidor().await;
        let admin = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        let bruno = entrar(&app, "bruno@x.co", CLAVE).await.unwrap();

        let (s, _) = llamar(
            &app,
            peticion("POST", "/api/cuentas/2/clave", Some(&admin), Some(r#"{"clave_nueva":"puesta-por-el-admin"}"#)),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert!(!sigue_dentro(&app, &bruno).await, "a Bruno no lo echó su propio restablecimiento");
        assert!(sigue_dentro(&app, &admin).await, "el restablecimiento echó al administrador");
        assert!(entrar(&app, "bruno@x.co", "puesta-por-el-admin").await.is_some());
    }

    #[tokio::test]
    async fn restablecer_una_cuenta_que_no_existe_no_dice_que_si() {
        // Cero filas NO es un error de SQL. Sin mirarlo, esto respondería «hecho».
        let (app, _) = servidor().await;
        let admin = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        let (s, _) = llamar(
            &app,
            peticion("POST", "/api/cuentas/9999/clave", Some(&admin), Some(r#"{"clave_nueva":"da-igual-cual"}"#)),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn el_admin_no_restablece_la_suya_por_la_puerta_de_atras() {
        // Por ahí se saltaría la comprobación de la contraseña actual, y además
        // se quedaría con la sesión muerta sin que nada se lo dijera.
        let (app, _) = servidor().await;
        let admin = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        let (s, _) = llamar(
            &app,
            peticion("POST", "/api/cuentas/1/clave", Some(&admin), Some(r#"{"clave_nueva":"la-mia-nueva-11"}"#)),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert!(sigue_dentro(&app, &admin).await);
    }

    // --- Las dos listas --------------------------------------------------------

    #[tokio::test]
    async fn la_administracion_ve_las_desactivadas_y_el_selector_no() {
        // Las dos listas salen de la misma tabla y prometen cosas distintas. Si
        // la administración filtrara por activo, reactivar a alguien exigiría el
        // `sqlite3` que esto viene a quitar; si el selector las trajera, se podría
        // asignar un flujo a quien ya no está.
        let (app, _) = servidor().await;
        let admin = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        let (s, _) = llamar(
            &app,
            peticion("PUT", "/api/cuentas/2", Some(&admin), Some(r#"{"nombre":"Bruno","rol":"OPERADOR","activo":false}"#)),
        )
        .await;
        assert_eq!(s, StatusCode::OK);

        let (_, cuentas) = llamar(&app, peticion("GET", "/api/cuentas", Some(&admin), None)).await;
        assert!(cuentas.to_string().contains("bruno@x.co"), "la administración perdió al desactivado");

        let (_, selector) = llamar(&app, peticion("GET", "/api/usuarios", Some(&admin), None)).await;
        assert!(!selector.to_string().contains("bruno@x.co"), "el selector ofrece a un desactivado");

        // Y el desactivado no entra, que es lo que «activo» significa.
        assert!(entrar(&app, "bruno@x.co", CLAVE).await.is_none());
    }
}
