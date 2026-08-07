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

// De `argon2` aquí solo se LEE: `PasswordHash` analiza el PHC guardado para
// decidir si se quedó por debajo de la política. Quien CALCULA es
// `guaca::claves` — ver [`hashear`]. Las pruebas sí importan el hasher completo,
// porque necesitan fabricar hashes con parámetros elegidos, y eso es lo que
// `guaca::claves` no ofrece ni debe ofrecer.
use argon2::PasswordHash;
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
    /// Si la cuenta tiene el segundo factor puesto. Va como `bool` derivado y no
    /// como la fecha: la administración solo necesita saber si hay algo que
    /// retirar —el botón de rescate no puede ofrecerse en las filas donde no
    /// significa nada—, y la fecha de enrolamiento no es asunto de nadie más.
    pub segundo_factor: bool,
}

const COLUMNAS: &str =
    "id, nombre, email, rol, activo, stakeholder_id, totp_confirmado_en IS NOT NULL AS segundo_factor";

#[derive(Deserialize)]
pub struct CuentaNueva {
    pub(crate) nombre: String,
    pub(crate) email: String,
    pub(crate) clave: String,
    pub(crate) rol: String,
    #[serde(default)]
    pub(crate) stakeholder_id: Option<i64>,
}

#[derive(Deserialize)]
pub struct CuentaCambio {
    nombre: String,
    rol: String,
    activo: bool,
    #[serde(default)]
    stakeholder_id: Option<i64>,
}

impl CuentaCambio {
    /// Para el panel. Los campos siguen privados: `rol_y_entidad` decide si la
    /// combinación tiene sentido, y armarla desde fuera se saltaría esa
    /// comprobación al construir.
    pub(crate) fn nuevo(
        nombre: String,
        rol: String,
        activo: bool,
        stakeholder_id: Option<i64>,
    ) -> Self {
        Self { nombre, rol, activo, stakeholder_id }
    }
}

#[derive(Deserialize)]
pub struct ClaveDeOtro {
    pub(crate) clave_nueva: String,
}

#[derive(Deserialize)]
pub struct MiClave {
    pub(crate) clave_actual: String,
    pub(crate) clave_nueva: String,
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
pub(crate) async fn con_turno<T, F>(e: &Estado, que: &str, trabajo: F) -> Result<T, Error>
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
/// No es una preferencia, y falla de **dos** maneras que conviene no confundir
/// —lo estaban en este comentario hasta el 2026-08-04—:
///
/// - Con otro **algoritmo o formato**, esa persona se queda fuera del sistema y
///   el fallo aparece lejos de su causa: el alta termina bien, anuncia unas
///   credenciales y no se puede entrar con ellas. Ya pasó en la versión de
///   Python, y es ruidoso — alguien llama el primer día.
/// - Con los mismos y **otros parámetros**, no se queda fuera nadie: el PHC de
///   Argon2 lleva su `m`, su `t` y su `p` dentro, y el verificador los lee de
///   ahí. Entra igual, con un hash que puede costar 2400 veces menos, y **no hay
///   ningún síntoma**. Es el caso peligroso, precisamente por callado, y es el
///   que se le escapaba al banco: medido por mutación, hashear con `m=8 t=1`
///   dejaba todas las pruebas en verde.
///
/// Con `guaca::claves` los dos dejan de poder ocurrir por construcción, que es
/// mejor que cazarlos.
///
/// **Quien calcula es `guaca::claves`, no este módulo**, y lo que aporta SIGER es
/// solo lo que guaca no puede saber: el turno del semáforo y el `spawn_blocking`.
/// Hasta el 2026-08-04 estas dos líneas estaban copiadas aquí, otra vez en
/// [`renovar_hash_si_hace_falta`] y una tercera en el subcomando `hashear` de
/// `main.rs` — tres copias que hoy coinciden por casualidad de tres literales
/// iguales, no porque nada lo obligue. Es justo el modo de fallo del que nació
/// guaca: «medico e informes copiaban la misma sesión y el mismo endurecimiento
/// de secreto, y las copias YA habían divergido». Cuatro sitios que afirman ser
/// el mismo Argon2 son cuatro sitios donde subir `Params::DEFAULT` puede
/// olvidarse en uno.
pub(crate) async fn hashear(e: &Estado, clave: String) -> Result<String, Error> {
    let hecho = con_turno(e, "hashear", move || guaca::claves::hashear(&clave)).await?;
    hecho.ok_or_else(|| {
        // `guaca::claves::hashear` devuelve `Option`, no el error del hasher: la
        // única rama que lo produce es que la sal aleatoria falle, y no ocurre.
        tracing::error!("no se pudo cifrar la clave");
        Error(StatusCode::INTERNAL_SERVER_ERROR, "error interno".into())
    })
}

/// Verifica contra el hash guardado. Un hash ilegible cuenta como «no coincide»,
/// que es lo que hace `guaca::claves::verificar`: a quien intenta entrar no se le
/// dice en qué se equivocó. Los hashes ilegibles se cuentan aparte, en
/// `revisar-hashes`, porque ahí sí son una avería y no un fallo de quien teclea.
pub(crate) async fn clave_correcta(e: &Estado, hash_texto: String, clave: String) -> Result<bool, Error> {
    con_turno(e, "verificar clave", move || {
        guaca::claves::verificar(&clave, &hash_texto)
    })
    .await
}

// --- El hash se pone al día cuando su dueño acierta ---------------------------

/// El trabajo que cuesta calcular un Argon2: memoria × iteraciones.
///
/// `p=` no entra. El paralelismo reparte el MISMO trabajo entre más carriles: no
/// lo aumenta ni lo reduce, así que meterlo en la cuenta diría que un hash es más
/// fuerte por estar peor repartido. En `u64` porque `m` va en KiB y `m × t`
/// desborda `u32` mucho antes de llegar a valores absurdos (m=4 GiB con t=2 ya lo
/// hace).
fn trabajo(p: &argon2::Params) -> u64 {
    p.m_cost() as u64 * p.t_cost() as u64
}

/// ¿Este hash está por debajo de lo que hoy produciría [`hashear`]?
///
/// La regla, en una frase: **solo se reescribe un hash al que el de hoy DOMINA —le
/// gana o iguala en memoria Y en trabajo total— y solo si hay algo que ganar.**
///
/// **La dominancia son DOS ejes** —memoria y trabajo total— y los dos tienen que
/// estar a favor. **Lo que hay que ganar son CINCO**: esos dos, la variante, la
/// versión y la longitud del digest.
///
/// | hash guardado | ¿se reescribe? | por qué |
/// |---|---|---|
/// | menos memoria y menos o igual trabajo | **sí** | es para lo que existe esto |
/// | igual memoria e igual trabajo, pero `argon2i`/`argon2d` o `v=0x10` | **sí** | no se pierde nada y se gana la variante |
/// | igual en todo, pero digest más corto de 32 bytes | **sí** | mismo coste, más salida: gratis |
/// | igual en todo y `Argon2id` al día | no | no hay nada que ganar |
/// | **más memoria** que hoy | **NO, jamás** | aunque su trabajo total sea menor |
/// | **más trabajo** que hoy | **NO, jamás** | aunque su memoria sea menor |
///
/// La longitud del digest está en la segunda lista y **no** en la primera a
/// propósito: ver [`digest_corto`].
///
/// # Dos rondas de auditoría, y el fallo cambió de lado las dos veces
///
/// **Primera versión**: cinco preguntas —algoritmo, versión, `m`, `t`, `p`— unidas
/// por un OR. Un hash más fuerte en conjunto pero más flojo en un eje salía «por
/// debajo» y se destruía: `Argon2id v19 m=2GiB t=1 p=4` —la **primera opción
/// recomendada por el RFC 9106**— tiene `t=1 < 2`, así que se rebajaba a
/// `m=19MiB t=2`, **53 veces menos trabajo**.
///
/// **Segunda versión**: una sola pregunta, `m × t`. Arreglaba lo anterior y abría
/// lo contrario, porque **el producto no pone suelo a la memoria**: un
/// `Argon2id m=8 t=4864` da exactamente el mismo `m × t` que hoy (38 912), así que
/// se certificaba «al día» — y 8 KiB **caben en la caché L1**. Era una credencial
/// de calidad PBKDF2 declarada conforme a una política de 19 MiB, y lo peor es que
/// la primera versión sí la cazaba: fue una regresión.
///
/// La lección, que es la que vale más que las dos correcciones: **un criterio
/// único sobre magnitudes que no son intercambiables no es «simple», es ciego.**
/// El trabajo total y la dureza de memoria son cosas distintas —lo que hace de
/// Argon2 algo mejor que PBKDF2 es la segunda, y la capacidad de una granja de
/// craqueo escala con `1/m`—, así que cambiar `m` por `t` a producto constante es
/// seguro hacia arriba y ruinoso hacia abajo. De ahí la **dominancia**: hoy tiene
/// que ganar o igualar en los dos ejes antes de tocar nada.
///
/// # Lo que queda sin resolver, y se dice en vez de certificarse
///
/// Un hash con **menos memoria pero más trabajo total** —`m=4096 t=10`, típico de
/// un sistema afinado a 100 ms en una máquina con poca RAM— no lo domina hoy ni
/// domina a hoy. No hay elección silenciosa honesta: reescribirlo pierde memoria y
/// dejarlo lo deja por debajo de la política. Así que **no se reescribe y se
/// anota**, con su id, en vez de dar por bueno lo que no lo está (directiva 20).
/// El camino honesto para esas cuentas es el mismo que para la variante: **subir
/// `Params::DEFAULT`**, no rebajar a los demás para igualarlos.
///
/// Devuelve `false` ante un hash que no se puede leer o cuyos parámetros no se
/// entienden: si no se sabe qué tiene, no se afirma que está por debajo
/// (directiva 20 — la ausencia de dato no se sustituye por una suposición, y aquí
/// suponer «está mal» significaría reescribir el hash de alguien sobre nada).
fn por_debajo_de_la_politica(h: &PasswordHash) -> bool {
    use argon2::{Algorithm, Params, Version};

    let Ok(alg) = Algorithm::try_from(h.algorithm) else { return false };
    let Ok(p) = Params::try_from(h) else { return false };
    let hoy = Params::DEFAULT;

    // Hoy tiene que ganar o igualar en LOS DOS ejes. Si el guardado tiene más
    // memoria, o más trabajo total, se deja en paz — sin mirar la variante: la
    // preferencia por `Argon2id` no vale ni un factor de dos de memoria.
    let domina = hoy.m_cost() >= p.m_cost() && trabajo(&hoy) >= trabajo(&p);
    if !domina {
        return false;
    }
    // Y tiene que haber algo que ganar, o esto reescribiría el hash de todo el
    // mundo en cada acceso para dejarlo igual.
    hoy.m_cost() > p.m_cost()
        || trabajo(&hoy) > trabajo(&p)
        || alg != Algorithm::Argon2id
        || h.version.is_some_and(|v| v < Version::default() as u32)
        || digest_corto(&p)
}

/// ¿El digest guardado es más corto de lo que hoy se emite?
///
/// Es el quinto eje, y va en «lo que hay que ganar» y **no en la dominancia**. La
/// diferencia importa: metido en `domina`, un hash con digest de 64 bytes dejaría
/// de poder subirse nunca aunque su `m` fuera de 8 KiB, porque hoy no lo domina en
/// ESE eje. Conservar un digest más largo no cuesta nada; lo que hay que subir es
/// uno corto.
///
/// Un `None` no cuenta como corto: `Params::try_from` rellena este campo con la
/// longitud del digest que trae el PHC, así que llegar aquí sin él significa que no
/// se sabe — y no se supone (directiva 20).
///
/// # El único eje en que el reescrito puede quedar por debajo, y por qué se acepta
///
/// El reescrito sale siempre de `Argon2::default()`, o sea con 32 bytes. Un hash
/// con digest de 48 o 64 bytes **y** parámetros flojos se reescribe por los otros
/// motivos y pierde longitud de digest en el camino: 512 bits pasan a 256. Se
/// acepta, y no es una excepción escondida — 256 bits siguen siendo inalcanzables,
/// mientras que lo que se gana en el mismo movimiento puede ser un factor de 2432
/// en memoria. Preservar el máximo sería posible, pero exigiría emitir con
/// parámetros distintos de los del resto del sistema para una población que no
/// existe: el `hashear` de SIGER emite 32 bytes siempre y el volcado venía de
/// passlib, que también. Lo fija
/// `un_digest_mas_largo_no_impide_subir_los_parametros`.
fn digest_corto(p: &argon2::Params) -> bool {
    p.output_len().unwrap_or(argon2::Params::DEFAULT_OUTPUT_LEN)
        < argon2::Params::DEFAULT_OUTPUT_LEN
}

/// En qué situación está un hash guardado respecto de la política de hoy.
///
/// Existe porque la pregunta que de verdad se hace alguien no es «¿este hash está
/// bien?» sino **«¿puedo subir `Params::DEFAULT` sin dejar a nadie con un hash que
/// no se pueda poner al día?»**, y eso es un recuento del padrón, no un aviso por
/// acceso. Ver el subcomando `revisar-hashes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SituacionHash {
    /// A la altura de la política, o por encima. Nada que hacer.
    AlDia,
    /// Hoy lo domina y hay algo que ganar: se pone al día en el próximo acceso de
    /// su dueño, solo. No hace falta hacer nada.
    SeSubeAlEntrar,
    /// Menos memoria que la política pero más trabajo total. No se puede
    /// reescribir sin rebajarle la dureza de memoria, así que **se queda como
    /// está**: lo que lo pone al día es subir `Params::DEFAULT`.
    FlojoEnMemoria,
    /// Más memoria que la política pero menos trabajo total. El espejo del
    /// anterior, y por mucho tiempo el cuadrante que no se contaba en ninguna
    /// parte: su déficit está acotado por debajo de 2× —si `m > m_hoy` y
    /// `m·t < m_hoy·t_hoy`, entonces `t < t_hoy`— mientras la memoria está por
    /// encima, así que no es más débil en conjunto. Se cuenta igual, porque «no
    /// aparece en ningún sitio» y «no pasa nada» no son lo mismo.
    FlojoEnTrabajo,
    /// Los parámetros están a la altura —o por encima—, pero el digest es más corto
    /// de los 32 bytes que hoy se emiten.
    ///
    /// **No se reescribe**, y ahí está la diferencia con [`Self::SeSubeAlEntrar`]:
    /// llegar aquí significa que su memoria o su trabajo están POR ENCIMA de la
    /// política, así que reescribirlo con `Params::DEFAULT` los rebajaría para ganar
    /// unos bytes de salida — un trato malo. Se cuenta, que es lo que faltaba: hasta
    /// el 2026-08-04 este caso salía como `AlDia`, o sea **certificado como
    /// conforme**, porque el quinto eje solo se consultaba dentro de la dominancia y
    /// el `return false` de arriba lo dejaba en la sombra. Lo cazó la auditoría de
    /// este mismo cambio: la invariante que el módulo declara seguía siendo falsa,
    /// en un cuadrante más estrecho que antes.
    DigestCorto,
    /// El PHC no se puede leer, no es Argon2, o no trae digest contra el que
    /// comparar. Desde aquí no se puede decir nada de él — y quien no puede entrar
    /// con él ya lo sabe.
    Ilegible,
}

/// Clasifica un hash guardado. La lógica de decisión es la MISMA que usa el
/// acceso; esto solo la nombra para poder contarla.
pub(crate) fn situacion_de(texto: &str) -> SituacionHash {
    let Ok(h) = PasswordHash::new(texto) else { return SituacionHash::Ilegible };
    if argon2::Algorithm::try_from(h.algorithm).is_err() {
        return SituacionHash::Ilegible;
    }
    // Sin digest no hay nada contra lo que comparar: el PHC describe unos parámetros
    // y su dueño no puede entrar con él. `Ilegible` es la etiqueta honesta — antes
    // caía en `AlDia`, o sea que el recuento decía «nada que hacer» de una cuenta
    // que no abre. Es la misma clase de mentira que `clave_argon2` cierra en el
    // volcado, por otra puerta.
    if h.hash.is_none() {
        return SituacionHash::Ilegible;
    }
    let Ok(p) = argon2::Params::try_from(&h) else { return SituacionHash::Ilegible };
    if por_debajo_de_la_politica(&h) {
        return SituacionHash::SeSubeAlEntrar;
    }
    let hoy = argon2::Params::DEFAULT;
    if p.m_cost() < hoy.m_cost() {
        SituacionHash::FlojoEnMemoria
    } else if trabajo(&p) < trabajo(&hoy) {
        SituacionHash::FlojoEnTrabajo
    } else if digest_corto(&p) {
        // FUERA de la sombra del `return false` de la dominancia, y por eso está
        // aquí y no dentro de `por_debajo_de_la_politica`: para REESCRIBIR hay que
        // dominar —si no, se rebajaría la memoria—, pero para CONTAR no. Un hash con
        // 64 MiB de memoria y 80 bits de salida no está «al día» por mucho que no se
        // pueda arreglar reescribiéndolo.
        SituacionHash::DigestCorto
    } else {
        SituacionHash::AlDia
    }
}

/// ¿Está por debajo de la política en MEMORIA, sin que hoy lo domine?
///
/// Es el caso que [`por_debajo_de_la_politica`] no puede resolver: menos memoria
/// que hoy pero más trabajo total, así que ni reescribir ni dejarlo es
/// gratis. No se decide en silencio — se anota, para que exista en algún sitio
/// distinto de la cabeza de quien lo leyó (directiva 20).
fn flojo_en_memoria_pero_no_dominado(h: &PasswordHash) -> bool {
    let Ok(p) = argon2::Params::try_from(h) else { return false };
    let hoy = argon2::Params::DEFAULT;
    p.m_cost() < hoy.m_cost() && trabajo(&p) > trabajo(&hoy)
}

/// Vuelve a escribir el hash de quien acaba de acertar su contraseña, si el que
/// tenía guardado se quedó por debajo de la política de hoy.
///
/// # Por qué existe
///
/// El sistema anterior lo hacía y al migrar a Rust se perdió sin decirlo. Sin
/// esto, un hash que entró con un coste bajo lo conserva **para siempre**: la
/// cuenta sigue entrando, así que nadie se entera nunca. El caso que lo destapó
/// fue el volcado (#231), que acepta cualquier Argon2 válido — pero el que de
/// verdad importa es el general: **el día que se suban los parámetros del KDF,
/// esto es lo único que pone al día las cuentas que ya existían.** Sin él, subir
/// la política solo protegería a quien se dé de alta después.
///
/// Solo puede hacerse aquí, en el acierto: reescribir un hash exige la contraseña
/// en claro, y este es el único momento en que el sistema la tiene y sabe que es
/// la buena.
///
/// # Tres cosas que se rompen solas si alguien las "simplifica"
///
/// 1. **El `UPDATE` va condicionado al hash viejo.** Entre la verificación y esta
///    escritura pasan un turno de semáforo y un Argon2id completo (19 MiB, ~35 ms
///    medidos en release), y en ese rato
///    la persona puede haber cambiado su contraseña desde otra pestaña. Sin el
///    `AND clave_hash = ?`, esto escribiría el hash de la contraseña VIEJA encima
///    de la nueva: la nueva dejaría de servir y la vieja volvería a valer. Es el
///    mismo patrón que el contador del TOTP y que `usado_en IS NULL` en los
///    códigos de recuperación, y por la misma razón.
/// 2. **No se toca `token_version`.** La regla 2 de la administración de cuentas
///    dice que todo cambio de contraseña lo sube, y esto **no es un cambio de
///    contraseña**: es la misma contraseña escrita de otra forma. Subirlo echaría
///    a la persona de sus otras sesiones al entrar, sin que nada se lo explique.
/// 3. **Nada de esto puede impedir entrar NI HACER ESPERAR.** El acierto ya
///    ocurrió; la sesión se abre igual. Si el Argon2 falla o si el `UPDATE` no
///    encuentra su fila, se anota en el log y se sigue — modo degradado (#71). Y
///    el turno de cripto se pide **sin esperar**: con el semáforo lleno esto se
///    salta en vez de retener medio minuto a quien ya tiene derecho a entrar. Por
///    eso no devuelve `Result`: no hay nada que el llamante pueda decidir, y
///    devolverlo invitaría a un `?` que convertiría una mejora silenciosa en un
///    fallo de acceso.
pub(crate) async fn renovar_hash_si_hace_falta(
    e: &Estado,
    id: i64,
    clave: String,
    hash_viejo: &str,
) {
    let Ok(analizado) = PasswordHash::new(hash_viejo) else { return };
    if !por_debajo_de_la_politica(&analizado) {
        if flojo_en_memoria_pero_no_dominado(&analizado) {
            // `debug!` y no `warn!`, y el cambio es deliberado: nada resuelve este
            // estado —a diferencia de la reescritura, que ocurre una vez y
            // converge—, así que en `warn!` esta línea sonaba en CADA acceso de las
            // mismas cuentas, para siempre. Una alarma que se repite sin que nada la
            // cierre es una alarma que a la tercera semana nadie mira, y se lleva
            // por delante las que sí son nuevas (directiva 23) — incluso siendo
            // cierta, que es lo que la hacía tentadora.
            //
            // El recuento del padrón, que es la pregunta que alguien se hace de
            // verdad —«¿puedo subir `Params::DEFAULT` sin dejar a nadie atrás?»—,
            // vive en el subcomando `revisar-hashes`: se consulta cuando se decide,
            // y cubre los DOS cuadrantes no dominados en vez de uno.
            tracing::debug!(
                "el hash del usuario {id} usa menos memoria que la política actual, pero \
                 más trabajo total: no se reescribe para no rebajarle la dureza de memoria. \
                 Recuento del padrón en `siger-api revisar-hashes`"
            );
        }
        return;
    }

    // El turno se pide SIN ESPERAR, y esa es la diferencia con todo lo demás que
    // usa `con_turno`.
    //
    // `con_turno` aguarda hasta `ESPERA_CRIPTO` —30 s— porque en sus otros usos no
    // hay respuesta que dar sin el resultado del Argon2. Aquí sí la hay: la sesión
    // ya está ganada. Esperar por un turno significaría que quien acertó su
    // contraseña se queda medio minuto mirando una pantalla en blanco para que
    // ocurra algo que no ha pedido y que no le cambia nada — y justo en el momento
    // de más carga, que es cuando el semáforo está lleno. Si no hay sitio, no se
    // hace: el hash sigue donde estaba y el próximo acceso vuelve a intentarlo.
    let Ok(permiso) = e.cfg.cripto.clone().try_acquire_owned() else {
        tracing::info!("sin turno libre de cripto: el hash de {id} se renovará en otro acceso");
        return;
    };
    let hecho = tokio::task::spawn_blocking(move || {
        let r = guaca::claves::hashear(&clave);
        drop(permiso); // el turno se libera con la memoria, no antes
        r
    })
    .await;
    let Ok(Some(nuevo)) = hecho else {
        tracing::warn!("no se pudo renovar el hash del usuario {id}; entra igual");
        return;
    };

    match sqlx::query("UPDATE usuarios SET clave_hash = ? WHERE id = ? AND clave_hash = ?")
        .bind(&nuevo)
        .bind(id)
        .bind(hash_viejo)
        .execute(&e.bd)
        .await
    {
        // Cero filas no es un error: significa que la contraseña cambió mientras
        // esto trabajaba, y entonces lo correcto es NO escribir.
        Ok(r) if r.rows_affected() == 0 => {
            tracing::info!("el hash del usuario {id} cambió mientras se renovaba; se deja el nuevo")
        }
        Ok(_) => tracing::info!("hash del usuario {id} puesto al día con la política actual"),
        Err(err) => tracing::warn!("no se pudo guardar el hash renovado de {id}: {err}"),
    }
}

// --- Extremos -----------------------------------------------------------------

/// `GET /api/cuentas` — todas, **incluidas las desactivadas**.
///
/// El selector de partes (`/api/usuarios`) filtra por `activo = 1` porque a un
/// flujo no se le asigna alguien que ya no está. Aquí es al revés: si la
/// administración no viera las desactivadas, reactivar a alguien exigiría el
/// `sqlite3` que esta pantalla viene a quitar.
/// El padrón de cuentas, para la administración.
///
/// Aparte del manejador porque lo usan `GET /api/cuentas` y la pantalla del
/// panel. Se lleva DENTRO las dos cosas que no pueden quedarse fuera:
/// `solo_admin` —y es SOLO `Admin`: `Supervisor` no, aunque su alcance sea
/// `Todas`, porque eso es para LEER entregas; si administrar colgara del
/// alcance, un rol de consulta podría fabricarse un administrador— y la línea de
/// bitácora. Un panel que leyera el padrón sin anotarlo sería la vía para
/// leerlo sin dejar rastro, que es justo lo que esa línea existe para impedir.
pub(crate) async fn listar_de(e: &Estado, actor: &Actor) -> Result<Vec<FichaCuenta>, Error> {
    solo_admin(actor)?;
    let filas = sqlx::query_as::<_, FichaCuenta>(&format!(
        "SELECT {COLUMNAS} FROM usuarios ORDER BY activo DESC, nombre"
    ))
    .fetch_all(&e.bd)
    .await?;

    // El padrón entero de personas —nombre, correo, rol y estado de todas— es la
    // única lectura del sistema que devuelve dato personal en bloque, y sale de
    // aquí. Se anota con cuántas filas salieron y SIN los ids: la bitácora no
    // puede volverse una segunda copia de lo que vigila.
    //
    // Los otros dos listados que traen nombres —`/api/usuarios` y
    // `/api/stakeholders`— no se anotan a propósito: los pide cada pintado del
    // editor de partes, y una bitácora que crece con cada pantalla ahoga la señal
    // que esta línea existe para dar (directiva 23).
    auditar(
        &e.bd,
        "cuentas_listadas",
        Some(&actor.email),
        Some(&format!("{} cuentas", filas.len())),
    )
    .await;

    Ok(filas)
}

pub async fn listar(State(e): State<Estado>, actor: Actor) -> Result<Json<Vec<FichaCuenta>>, Error> {
    Ok(Json(listar_de(&e, &actor).await?))
}

/// `POST /api/cuentas` — alta.
/// Da de alta una cuenta. Aparte del manejador porque la usan `POST /api/cuentas`
/// y el formulario del panel, con `solo_admin` y las cinco validaciones DENTRO:
/// nombre, correo, clave, rol y la coherencia rol/entidad. Repetirlas en el panel
/// sería crear una segunda puerta con otras reglas.
pub(crate) async fn crear_de(
    e: &Estado,
    actor: &Actor,
    nueva: &CuentaNueva,
) -> Result<FichaCuenta, Error> {
    solo_admin(actor)?;
    let nombre = nombre_valido(&nueva.nombre)?;
    let email = email_valido(&nueva.email)?;
    let clave = clave_valida(&nueva.clave)?;
    let rol = rol_valido(&nueva.rol)?;
    rol_y_entidad(rol, nueva.stakeholder_id)?;

    let hash = hashear(e, clave).await?;

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
    Ok(fila)
}

pub async fn crear(
    State(e): State<Estado>,
    actor: Actor,
    Json(nueva): Json<CuentaNueva>,
) -> Result<Response, Error> {
    let fila = crear_de(&e, &actor, &nueva).await?;
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
/// Edita una cuenta. Aparte del manejador porque la usan `PUT /api/cuentas/{id}`
/// y el panel, y se lleva DENTRO lo que no puede repetirse: `solo_admin`, la
/// validación del rol y su coherencia con la entidad, y —lo que de verdad
/// importa— la comprobación del ÚLTIMO administrador activo, que va en la MISMA
/// transacción que la escritura. Comprobarla fuera no valdría nada: dos
/// administradores degradándose a la vez leerían «queda otro» los dos.
pub(crate) async fn actualizar_de(
    e: &Estado,
    actor: &Actor,
    id: i64,
    cambio: &CuentaCambio,
) -> Result<FichaCuenta, Error> {
    solo_admin(actor)?;
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
    Ok(fila)
}

pub async fn actualizar(
    State(e): State<Estado>,
    actor: Actor,
    Path(id): Path<i64>,
    Json(cambio): Json<CuentaCambio>,
) -> Result<Json<FichaCuenta>, Error> {
    Ok(Json(actualizar_de(&e, &actor, id, &cambio).await?))
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
/// Restablece la contraseña de OTRA persona. Aparte del manejador porque la usan
/// `POST /api/cuentas/{id}/clave` y el panel, con `solo_admin` y la desviación
/// del caso propio DENTRO.
pub(crate) async fn restablecer_clave_de(
    e: &Estado,
    actor: &Actor,
    id: i64,
    cuerpo: &ClaveDeOtro,
) -> Result<(), Error> {
    solo_admin(actor)?;
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
    let hash = hashear(e, clave).await?;

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
    Ok(())
}

pub async fn restablecer_clave(
    State(e): State<Estado>,
    actor: Actor,
    Path(id): Path<i64>,
    Json(cuerpo): Json<ClaveDeOtro>,
) -> Result<Json<serde_json::Value>, Error> {
    restablecer_clave_de(&e, &actor, id, &cuerpo).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `DELETE /api/cuentas/{id}/2fa` — quitarle el segundo factor a OTRA persona.
///
/// # El caso que no tenía salida
///
/// Quien pierde el teléfono **y** los diez códigos de recuperación se queda
/// fuera de su cuenta para siempre: `POST /api/2fa/desactivar` exige teclear la
/// contraseña *y* estar dentro, y restablecerle la contraseña no ayuda —el
/// segundo paso sigue en medio—. La única salida era abrir la base con `sqlite3`,
/// que es exactamente la situación que este módulo existe para eliminar. Y no es
/// un caso raro: el teléfono se pierde, se moja y se cambia de número.
///
/// Tampoco es una grieta nueva. Un administrador ya podía restablecer la
/// contraseña de cualquiera, así que ya podía entrar a cualquier cuenta; lo que
/// faltaba era poder hacerlo **sabiendo que lo hace**, con su línea en la
/// bitácora, en vez de con un cliente de SQLite y sin rastro.
///
/// # Por qué NO se puede usar sobre uno mismo
///
/// Porque sería la puerta de atrás de la regla 6 de #212: quitar el propio
/// segundo factor exige la contraseña actual, y si un administrador pudiera
/// quitárselo por aquí, una sesión suya robada lo apagaría sin saber ninguna
/// contraseña. Para el propio está `POST /api/2fa/desactivar`.
/// Le quita el segundo factor a OTRA persona: el rescate de quien perdió el
/// teléfono Y los diez códigos. Aparte del manejador porque la usan
/// `DELETE /api/cuentas/{id}/2fa` y el panel.
pub(crate) async fn retirar_segundo_factor_de(
    e: &Estado,
    actor: &Actor,
    id: i64,
) -> Result<(), Error> {
    solo_admin(actor)?;
    if id == actor.id {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "para quitar el suyo use «verificación en dos pasos», que le pedirá su contraseña".into(),
        ));
    }

    let mut tx = e.bd.begin().await?;
    // El secreto, el contador y los códigos se van JUNTOS. Dejar los códigos
    // vivos sin secreto sería peor que no hacer nada: diez credenciales de un
    // solo uso, en un papel, que abren una cuenta que ya no tiene segundo factor.
    let email: Option<String> = sqlx::query_scalar(
        "UPDATE usuarios SET totp_secreto_cifrado = NULL, totp_confirmado_en = NULL, \
         totp_ultimo_contador = NULL WHERE id = ? AND totp_confirmado_en IS NOT NULL \
         RETURNING email",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;

    // Cero filas son DOS cosas distintas y se contestan distinto: la cuenta no
    // existe, o existe y no tenía segundo factor. Responder «hecho» en el segundo
    // caso mandaría al administrador a buscar por otro lado un bloqueo que él
    // creería resuelto.
    let Some(email) = email else {
        let existe: Option<i64> = sqlx::query_scalar("SELECT id FROM usuarios WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        return Err(match existe {
            Some(_) => Error(
                StatusCode::CONFLICT,
                "esa cuenta no tiene segundo factor activo".into(),
            ),
            None => Error(StatusCode::NOT_FOUND, "esa cuenta no existe".into()),
        });
    };

    sqlx::query("DELETE FROM codigo_recuperacion WHERE usuario_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    auditar(&e.bd, "2fa_retirado_por_admin", Some(&actor.email), Some(&email)).await;
    Ok(())
}

pub async fn retirar_segundo_factor(
    State(e): State<Estado>,
    actor: Actor,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, Error> {
    retirar_segundo_factor_de(&e, &actor, id).await?;
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
/// Devuelve las cabeceras con la cookie REEMITIDA. Aparte del manejador porque
/// la usan `POST /api/mi-clave` y el formulario del panel, y aquí no puede haber
/// dos versiones: el salto de `token_version` y la reemisión de la cookie son
/// una pareja —sin el salto, cambiar la contraseña no echa a quien entró con la
/// vieja; sin la reemisión, te echa a ti y nadie usa el botón—.
pub(crate) async fn cambiar_mi_clave_de(
    e: &Estado,
    actor: &Actor,
    cuerpo: &MiClave,
) -> Result<HeaderMap, Error> {
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
    if !clave_correcta(e, hash_actual, cuerpo.clave_actual.clone()).await? {
        auditar(&e.bd, "clave_propia_rechazada", Some(&actor.email), None).await;
        return Err(Error(StatusCode::UNAUTHORIZED, "la contraseña actual no es correcta".into()));
    }

    let hash = hashear(e, nueva).await?;

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
    Ok(cabeceras)
}

pub async fn cambiar_mi_clave(
    State(e): State<Estado>,
    actor: Actor,
    Json(cuerpo): Json<MiClave>,
) -> Result<Response, Error> {
    let cabeceras = cambiar_mi_clave_de(&e, &actor, &cuerpo).await?;
    Ok((cabeceras, Json(serde_json::json!({ "ok": true }))).into_response())
}

#[cfg(test)]
mod pruebas {
    use super::*;
    // El hasher completo vive aquí y no arriba: la producción calcula por
    // `guaca::claves`, y estas pruebas necesitan además FABRICAR hashes con
    // parámetros y sales elegidos —flojos, ajenos, de otra variante— que
    // `guaca::claves` no puede producir. Y verificar con `argon2` directamente,
    // en vez de con `guaca::claves::verificar`, es a propósito: así el que
    // comprueba no es el mismo que calculó (directiva 33).
    use argon2::password_hash::{PasswordHasher, SaltString};
    use argon2::{Argon2, PasswordVerifier};
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
            ("DELETE", "/api/cuentas/1/2fa", None),
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
        // Que las seis rutas ESTÉN en el router. Las pruebas que llaman al
        // manejador no lo verían: borrar un `.route(...)` las dejaría verdes y el
        // extremo daría 404. Un 401 dice «existe y te rechaza»; un 404, «no está».
        let (app, _) = servidor().await;
        for (m, u) in [
            ("GET", "/api/cuentas"),
            ("POST", "/api/cuentas"),
            ("PUT", "/api/cuentas/1"),
            ("POST", "/api/cuentas/1/clave"),
            ("DELETE", "/api/cuentas/1/2fa"),
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

    // --- El rescate del segundo factor -----------------------------------------

    /// Le pone a Bruno un segundo factor activo con un código de recuperación.
    ///
    /// Se escriben las columnas a mano y no se enrola de verdad: lo que se prueba
    /// aquí es RETIRARLO, y enrolar cuesta dos Argon2id —64 MiB el del secreto,
    /// que va por `guaca::reposo`, y 19 MiB el del código— que no afirman nada sobre
    /// lo que estas pruebas dicen.
    async fn con_segundo_factor_puesto(e: &crate::rutas::Estado) {
        sqlx::query(
            "UPDATE usuarios SET totp_secreto_cifrado = X'00', \
             totp_confirmado_en = '2026-08-02T00:00:00Z', totp_ultimo_contador = 59000000 \
             WHERE id = 2",
        )
        .execute(&e.bd)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO codigo_recuperacion (usuario_id, hash, creado_en) \
             VALUES (2, 'no-importa', '2026-08-02T00:00:00Z')",
        )
        .execute(&e.bd)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn un_administrador_rescata_a_quien_perdio_el_telefono_y_los_codigos() {
        // El caso sin salida antes de esto: `desactivar` exige estar DENTRO y
        // teclear la contraseña, y restablecerle la contraseña no ayuda porque el
        // segundo paso sigue en medio. La única puerta era `sqlite3` — justo lo
        // que este módulo existe para eliminar.
        let (app, e) = servidor().await;
        con_segundo_factor_puesto(&e).await;

        // La mitad que hace de control: con el factor puesto, la contraseña sola
        // no abre nada. Lo que `entrar` devuelve entonces NO es una sesión sino el
        // token pendiente —el login responde 200 y pone `siger_pendiente`—, y con
        // él `/api/yo` sigue cerrada. Sin este control, la prueba de abajo pasaría
        // igual aunque el segundo factor nunca hubiera estado puesto.
        let pendiente = entrar(&app, "bruno@x.co", CLAVE).await.expect("el login responde 200");
        assert!(
            !sigue_dentro(&app, &pendiente).await,
            "con segundo factor, la contraseña sola no puede dejar a nadie dentro"
        );

        let admin = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        let (s, _) = llamar(&app, peticion("DELETE", "/api/cuentas/2/2fa", Some(&admin), None)).await;
        assert_eq!(s, StatusCode::OK);

        // Y ahora sí entra, que es lo único que le importa a quien llamó por
        // teléfono a decir que perdió el suyo.
        let suya = entrar(&app, "bruno@x.co", CLAVE).await.expect("Bruno recuperó su cuenta");
        assert!(sigue_dentro(&app, &suya).await);

        // Los códigos se van CON el factor. Dejarlos vivos sería peor que no
        // hacer nada: credenciales de un solo uso, en un papel, que abren una
        // cuenta que ya no tiene segundo factor.
        let quedan: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM codigo_recuperacion WHERE usuario_id = 2")
            .fetch_one(&e.bd)
            .await
            .unwrap();
        assert_eq!(quedan, 0, "quedaron códigos de recuperación de una cerradura que ya no existe");
    }

    #[tokio::test]
    async fn un_administrador_no_se_quita_el_suyo_por_esta_puerta() {
        // Sería la puerta de atrás de la regla 6 de #212: quitarse el propio
        // segundo factor exige la contraseña actual, y por aquí una sesión robada
        // lo apagaría sin saber ninguna.
        let (app, e) = servidor().await;
        // La sesión se abre ANTES de ponerle el factor: con él puesto el login es
        // de dos pasos y no entrega cookie, y lo que esta prueba mira es la
        // guarda del extremo, no cómo se llegó a tener una sesión válida.
        let admin = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        sqlx::query("UPDATE usuarios SET totp_confirmado_en = '2026-08-02T00:00:00Z' WHERE id = 1")
            .execute(&e.bd)
            .await
            .unwrap();

        let (s, _) = llamar(&app, peticion("DELETE", "/api/cuentas/1/2fa", Some(&admin), None)).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let sigue: Option<String> =
            sqlx::query_scalar("SELECT totp_confirmado_en FROM usuarios WHERE id = 1")
                .fetch_one(&e.bd)
                .await
                .unwrap();
        assert!(sigue.is_some(), "el administrador se quitó el suyo saltándose la contraseña");
    }

    #[tokio::test]
    async fn retirar_un_factor_que_no_esta_no_responde_hecho() {
        // Cero filas son DOS cosas y se contestan distinto. Decir «hecho» cuando
        // no había nada que quitar manda al administrador a buscar por otro lado
        // un bloqueo que él creería resuelto (directiva 20).
        let (app, _) = servidor().await;
        let admin = entrar(&app, "ada@x.co", CLAVE).await.unwrap();

        let (s, _) = llamar(&app, peticion("DELETE", "/api/cuentas/2/2fa", Some(&admin), None)).await;
        assert_eq!(s, StatusCode::CONFLICT, "existe y no tenía factor");

        let (s, _) = llamar(&app, peticion("DELETE", "/api/cuentas/99/2fa", Some(&admin), None)).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "esa cuenta no existe");
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

    /// Leer el padrón de personas deja rastro; el intento rechazado, no (#186).
    #[tokio::test]
    async fn sacar_la_lista_de_cuentas_queda_anotado_y_el_intento_rechazado_no() {
        let (app, e) = servidor().await;
        let admin = entrar(&app, "ada@x.co", CLAVE).await.unwrap();
        let (s, _) = llamar(&app, peticion("GET", "/api/cuentas", Some(&admin), None)).await;
        assert_eq!(s, StatusCode::OK);

        let filas: Vec<(String, String)> = sqlx::query_as(
            "SELECT email, COALESCE(detalle,'') FROM auditoria WHERE evento = 'cuentas_listadas'",
        )
        .fetch_all(&e.bd)
        .await
        .unwrap();
        assert_eq!(filas, vec![("ada@x.co".to_string(), "2 cuentas".to_string())]);

        // El operador no pasa de `solo_admin`, así que no leyó nada: anotarlo
        // como lectura del padrón sería anotar lo que no ocurrió. Que alguien lo
        // intente es otro hecho — y hoy el sistema no lo registra: lo dice
        // `CLAUDE.md`, no una suposición de quien lea esta prueba.
        let operador = entrar(&app, "bruno@x.co", CLAVE).await.unwrap();
        let (s, _) = llamar(&app, peticion("GET", "/api/cuentas", Some(&operador), None)).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auditoria WHERE evento = 'cuentas_listadas'")
            .fetch_one(&e.bd)
            .await
            .unwrap();
        assert_eq!(n, 1);
    }

    // --- El hash se pone al día cuando su dueño acierta -----------------------

    /// Una CADENA PHC con los parámetros que se le pidan, sin calcular nada.
    ///
    /// `por_debajo_de_la_politica` solo LEE la cabecera del hash: no verifica
    /// ninguna contraseña, así que el digest puede ser cualquiera. Y esa
    /// diferencia no es cosmética — fabricar de verdad un `m=2GiB` reserva dos
    /// gigas y llevó esta prueba de 15 a 83 segundos antes de darse cuenta. Para
    /// los casos que sí necesitan entrar por HTTP está [`hash_con`], que calcula.
    fn phc_con(m: u32, t: u32, p: u32, alg: argon2::Algorithm) -> String {
        phc_con_digest(m, t, p, alg, argon2::Params::DEFAULT_OUTPUT_LEN)
    }

    /// Igual que [`phc_con`], con la longitud de digest que se le pida. `bytes`
    /// tiene que caer entre 10 y 64: `password_hash::Output` rechaza el resto, así
    /// que un montaje fuera de rango se delataría al parsear.
    fn phc_con_digest(
        m: u32,
        t: u32,
        p: u32,
        alg: argon2::Algorithm,
        bytes: usize,
    ) -> String {
        use argon2::password_hash::Output;
        let relleno: Vec<u8> = (0..bytes).map(|i| i as u8).collect();
        let digest = Output::new(&relleno).expect("longitud de digest admitida");
        // Sal de relleno con la longitud que el parseador exige.
        format!("${alg}$v=19$m={m},t={t},p={p}$c2FsZGVsYXNwcnVlYmFzZGVjdWVudA${digest}")
    }

    /// Un hash de la MISMA contraseña pero con los parámetros que se le pidan.
    /// CALCULA, así que solo para parámetros baratos y cuando hace falta que la
    /// contraseña entre de verdad. Para leer parámetros, [`phc_con`].
    fn hash_con(clave: &str, m: u32, t: u32, p: u32, alg: argon2::Algorithm) -> String {
        let params = argon2::Params::new(m, t, p, None).expect("parámetros válidos");
        let a2 = Argon2::new(alg, argon2::Version::V0x13, params);
        let sal = SaltString::from_b64("c2FsZGVsYXNwcnVlYmFzZGVjdWVudA").unwrap();
        a2.hash_password(clave.as_bytes(), &sal).unwrap().to_string()
    }

    /// Los parámetros de un hash guardado, para afirmar sobre ellos.
    async fn params_de(e: &crate::rutas::Estado, id: i64) -> (String, u32, u32, u32) {
        let texto: String = sqlx::query_scalar("SELECT clave_hash FROM usuarios WHERE id = ?")
            .bind(id)
            .fetch_one(&e.bd)
            .await
            .unwrap();
        let h = PasswordHash::new(&texto).expect("el hash guardado se lee");
        let p = argon2::Params::try_from(&h).expect("y sus parámetros también");
        (texto, p.m_cost(), p.t_cost(), p.p_cost())
    }

    #[test]
    fn la_politica_solo_sube_y_nunca_baja() {
        use argon2::{Algorithm, Params};
        let hoy = Params::DEFAULT;

        // Lo que hoy produce `hashear` no está por debajo de sí mismo.
        let al_dia = phc_con(hoy.m_cost(), hoy.t_cost(), hoy.p_cost(), Algorithm::Argon2id);
        assert!(!por_debajo_de_la_politica(&PasswordHash::new(&al_dia).unwrap()));

        // Cada parámetro por separado, para que la regla no pase por casualidad
        // de que los tres bajen a la vez.
        for (m, t, p) in [
            (8, hoy.t_cost(), hoy.p_cost()),               // memoria de risa
            (hoy.m_cost(), 1, hoy.p_cost()),               // una sola iteración
            (hoy.m_cost(), hoy.t_cost(), 1),               // sin paralelismo, si hoy hay
        ] {
            if m == hoy.m_cost() && t == hoy.t_cost() && p == hoy.p_cost() {
                continue; // ese eje ya está en el mínimo hoy: no dice nada
            }
            let flojo = phc_con(m, t, p, Algorithm::Argon2id);
            assert!(
                por_debajo_de_la_politica(&PasswordHash::new(&flojo).unwrap()),
                "m={m} t={t} p={p} debería estar por debajo de m={} t={} p={}",
                hoy.m_cost(),
                hoy.t_cost(),
                hoy.p_cost()
            );
        }

        // Otra variante de Argon2 verifica bien, pero no es lo que hoy se emite.
        let otra = phc_con(hoy.m_cost(), hoy.t_cost(), hoy.p_cost(), Algorithm::Argon2i);
        assert!(por_debajo_de_la_politica(&PasswordHash::new(&otra).unwrap()));

        // Y la otra mitad, que es la que impide que esto se vuelva una excusa para
        // reescribir hashes ajenos: MÁS fuerte que hoy se deja en paz.
        let fuerte = phc_con(hoy.m_cost() * 2, hoy.t_cost() + 1, hoy.p_cost(), Algorithm::Argon2id);
        assert!(
            !por_debajo_de_la_politica(&PasswordHash::new(&fuerte).unwrap()),
            "se estaba rebajando un hash mejor que el actual"
        );

        // EL EJE MIXTO, que es donde la primera versión de esta regla destruía
        // hashes buenos: más fuerte en conjunto, más flojo en UN eje. Sin estos
        // casos la afirmación del nombre no se comprueba — el «más fuerte» de
        // arriba lo es en todos los ejes a la vez, así que pasaría igual con la
        // regla rota.
        for (m, t, p, alg, que_es) in [
            // La PRIMERA opción recomendada por el RFC 9106: t=1 con memoria enorme.
            (2 * 1024 * 1024, 1, 4, Algorithm::Argon2id, "RFC 9106, opción 1"),
            (256 * 1024, 1, 1, Algorithm::Argon2id, "256 MiB con una iteración"),
            // Variante peor, pero 107 veces más trabajo: cambiarla saldría carísimo.
            (1024 * 1024, 4, 8, Algorithm::Argon2i, "argon2i con 1 GiB"),
        ] {
            let h = phc_con(m, t, p, alg);
            let leido = PasswordHash::new(&h).unwrap();
            let suyo = argon2::Params::try_from(&leido).unwrap();
            assert!(
                trabajo(&suyo) > trabajo(&hoy),
                "el montaje de la prueba no es más fuerte que hoy: {que_es}"
            );
            assert!(
                !por_debajo_de_la_politica(&leido),
                "se iba a rebajar «{que_es}» (m={m} t={t}) a m={} t={}: {}× menos trabajo",
                hoy.m_cost(),
                hoy.t_cost(),
                trabajo(&suyo) / trabajo(&hoy)
            );
        }

        // Y al revés: IGUAL en los dos ejes pero variante o versión anticuadas SÍ se
        // reescribe, porque no se pierde nada. Es lo que impide que la corrección de
        // arriba se pase de frenada y deje de subir lo que sí conviene subir.
        let mismo_todo_peor_variante =
            phc_con(hoy.m_cost(), hoy.t_cost(), hoy.p_cost(), Algorithm::Argon2i);
        assert!(
            por_debajo_de_la_politica(&PasswordHash::new(&mismo_todo_peor_variante).unwrap()),
            "un argon2i con los mismos parámetros debería subirse a argon2id: es gratis"
        );
        // La versión, que es la mitad gemela de la anterior. Sin este caso se puede
        // borrar la comprobación de `v=` sin que nada se caiga — lo comprobó la
        // segunda auditoría, y por eso está aquí.
        let mismo_todo_version_vieja = phc_con(hoy.m_cost(), hoy.t_cost(), hoy.p_cost(), Algorithm::Argon2id)
            .replace("$v=19$", "$v=16$");
        assert!(
            por_debajo_de_la_politica(&PasswordHash::new(&mismo_todo_version_vieja).unwrap()),
            "un v=16 con los mismos parámetros debería subirse a v=19: es gratis"
        );

        // EL SUELO DE MEMORIA. Estos tienen el MISMO `m × t` que hoy —o más— y una
        // memoria irrisoria: si la regla mira solo el producto, los certifica «al
        // día» y esa cuenta se queda con 8 KiB de dureza para siempre. 8 KiB caben
        // en la caché L1: es PBKDF2 con otro nombre.
        for (m, t) in [(8u32, 4864u32), (16, 2432), (64, 608), (1024, 38), (9728, 4)] {
            let h = phc_con(m, t, 1, Algorithm::Argon2id);
            let leido = PasswordHash::new(&h).unwrap();
            let suyo = argon2::Params::try_from(&leido).unwrap();
            assert!(
                trabajo(&suyo) >= trabajo(&hoy),
                "el montaje no iguala el trabajo de hoy: m={m} t={t}"
            );
            assert!(
                por_debajo_de_la_politica(&leido),
                "m={m} t={t} tiene el mismo trabajo que hoy pero {}× menos memoria, \
                 y se estaba dando por bueno",
                hoy.m_cost() / m
            );
        }

        // Y el que NO se puede resolver: menos memoria, más trabajo. Ni se reescribe
        // —perdería dureza de memoria— ni se certifica: se anota.
        let ni_uno_ni_otro = phc_con(4096, 10, 1, Algorithm::Argon2id);
        let leido = PasswordHash::new(&ni_uno_ni_otro).unwrap();
        assert!(!por_debajo_de_la_politica(&leido), "se iba a rebajar la memoria de m=4096 t=10");
        assert!(
            flojo_en_memoria_pero_no_dominado(&leido),
            "y tampoco se estaba anotando: quedaría certificado en silencio"
        );
        // La mitad que discrimina: lo que está al día no se anota, o el aviso sonaría
        // en cada acceso de todo el mundo y a la tercera nadie lo miraría.
        assert!(!flojo_en_memoria_pero_no_dominado(&PasswordHash::new(&al_dia).unwrap()));

        // `p=` NO entra en el trabajo: reparte el mismo, no lo aumenta. El caso está
        // elegido para que meter `p` en el producto CAMBIE la decisión — la mitad de
        // memoria que hoy y el mismo trabajo real (9728 × 4 = 38912), pero con `p=2`
        // el producto inflado daría 77 824 y esto se daría por bueno. Un caso con la
        // misma decisión en los dos casos no probaría nada.
        let reparto_infla_el_producto = phc_con(9728, 4, 2, Algorithm::Argon2id);
        assert!(
            por_debajo_de_la_politica(&PasswordHash::new(&reparto_infla_el_producto).unwrap()),
            "`p=` se estaba contando como trabajo, y con eso un hash con la mitad de \
             memoria pasaba por «al día»"
        );

        // EL QUINTO EJE: la longitud del digest. Un hash idéntico a la política en
        // todo lo demás pero con salida corta se subía a «al día» y se quedaba así
        // para siempre — 80 bits de digest certificados como conformes con una
        // política de 256. No era explotable (2^80 Argon2id), pero la invariante que
        // este módulo DECLARA —«sin reescribir ⟹ está a la altura, o se anota»— era
        // falsa tal como estaba escrita, y eso es lo que se cierra.
        for bytes in [10usize, 16, 24, 31] {
            let corto = phc_con_digest(hoy.m_cost(), hoy.t_cost(), hoy.p_cost(), Algorithm::Argon2id, bytes);
            let leido = PasswordHash::new(&corto).unwrap();
            assert_eq!(
                argon2::Params::try_from(&leido).unwrap().output_len(),
                Some(bytes),
                "el montaje no produjo un digest de {bytes} bytes"
            );
            assert!(
                por_debajo_de_la_politica(&leido),
                "un digest de {bytes} bytes se estaba certificando «al día»"
            );
        }

        // Y la mitad que impide pasarse de frenada por el otro lado: un digest de 32
        // —lo que hoy se emite— NO es motivo para reescribir nada.
        let justo = phc_con_digest(hoy.m_cost(), hoy.t_cost(), hoy.p_cost(), Algorithm::Argon2id, 32);
        assert!(!por_debajo_de_la_politica(&PasswordHash::new(&justo).unwrap()));

        // EL TECHO DE MEMORIA, que es la pareja del suelo. Un hash con MÁS memoria que
        // hoy no se toca aunque su trabajo total sea menor: reescribirlo cambiaría 38
        // MiB por 19 MiB a trabajo casi idéntico, o sea pura pérdida. Sin estos casos
        // la dominancia se puede reducir a mirar solo el trabajo y nada se cae.
        for (m, t, alg, que_es) in [
            (38911u32, 1u32, Algorithm::Argon2id, "menos trabajo, pero el doble de memoria"),
            (38912, 1, Algorithm::Argon2i, "igual trabajo, doble memoria, variante vieja"),
        ] {
            let h = phc_con(m, t, 1, alg);
            let leido = PasswordHash::new(&h).unwrap();
            assert!(
                !por_debajo_de_la_politica(&leido),
                "«{que_es}» (m={m} t={t}): se iba a rebajar de {m} KiB a {} KiB de memoria",
                hoy.m_cost()
            );
        }

        // Reparto distinto, mismo trabajo y misma variante: no hay nada que ganar.
        if hoy.p_cost() < 4 {
            let otro_reparto = phc_con(hoy.m_cost(), hoy.t_cost(), 4, Algorithm::Argon2id);
            assert!(
                !por_debajo_de_la_politica(&PasswordHash::new(&otro_reparto).unwrap()),
                "`p=` no cambia el trabajo: no es motivo para reescribir"
            );
        }

        // Un hash que no se puede leer NO se declara flojo: no se sabe qué tiene,
        // y suponerlo significaría reescribir el hash de alguien sobre nada.
        assert!(PasswordHash::new("esto-no-es-un-phc").is_err());
        // Un PHC que SÍ parsea y NO es Argon2. Antes aquí había un bcrypt dentro de
        // un `if let Ok(...)`, y era código muerto: `PasswordHash::new` lo rechaza
        // por la sal, así que el cuerpo no se ejecutaba nunca y la afirmación no se
        // comprobaba. Lo señaló la segunda auditoría.
        let ajeno = PasswordHash::new("$scrypt$ln=16,r=8,p=1$c2FsZGVsYXNwcnVlYmFz$AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8")
            .expect("este PHC parsea, aunque no sea Argon2");
        assert!(!por_debajo_de_la_politica(&ajeno), "un hash ajeno a Argon2 no se toca aquí");
    }

    #[test]
    fn un_digest_mas_largo_no_impide_subir_los_parametros() {
        use argon2::{Algorithm, Params};
        let hoy = Params::DEFAULT;

        // Es la razón de que la longitud del digest esté en «lo que hay que ganar» y
        // NO en la dominancia. Metida en `domina`, este hash —8 KiB de memoria, o
        // sea lo que esta función existe para arreglar— dejaría de poder subirse
        // nunca, solo por traer una salida más larga de la que hoy se emite.
        for bytes in [48usize, 64] {
            let flojo_pero_largo = phc_con_digest(8, hoy.t_cost(), hoy.p_cost(), Algorithm::Argon2id, bytes);
            let leido = PasswordHash::new(&flojo_pero_largo).unwrap();
            assert_eq!(Params::try_from(&leido).unwrap().output_len(), Some(bytes));
            assert!(
                por_debajo_de_la_politica(&leido),
                "un digest de {bytes} bytes bloqueó la subida de m=8 KiB a {} KiB",
                hoy.m_cost()
            );
        }

        // Y el precio, dicho en una prueba y no solo en un comentario: al
        // reescribirlo, la salida pasa a 32 bytes. Se acepta porque 256 bits siguen
        // siendo inalcanzables y en el mismo movimiento la memoria sube 2432 veces.
        // Si algún día se decide conservar el máximo, esta afirmación se cae y
        // avisa.
        assert_eq!(hoy.output_len(), None, "Params::DEFAULT no fija salida");
        assert_eq!(Params::DEFAULT_OUTPUT_LEN, 32);

        // Un digest más largo, con todo lo demás al día, NO se toca: no hay nada
        // que ganar y bajarlo sería la única forma de empeorarlo.
        let solo_mas_largo = phc_con_digest(hoy.m_cost(), hoy.t_cost(), hoy.p_cost(), Algorithm::Argon2id, 64);
        assert!(
            !por_debajo_de_la_politica(&PasswordHash::new(&solo_mas_largo).unwrap()),
            "se iba a rebajar un digest de 64 bytes sin ganar nada a cambio"
        );
    }

    #[test]
    fn situacion_de_clasifica_cada_caso() {
        use argon2::{Algorithm, Params};
        let hoy = Params::DEFAULT;

        // Los cinco, y cada uno con un caso que NO cae en los otros cuatro: si dos
        // situaciones compartieran caso, la prueba pasaría con la clasificación
        // fundida en una sola rama.
        let casos = [
            (SituacionHash::AlDia, phc_con(hoy.m_cost(), hoy.t_cost(), hoy.p_cost(), Algorithm::Argon2id)),
            (SituacionHash::AlDia, phc_con(hoy.m_cost() * 2, hoy.t_cost(), hoy.p_cost(), Algorithm::Argon2id)),
            (SituacionHash::SeSubeAlEntrar, phc_con(8, 1, 1, Algorithm::Argon2id)),
            (SituacionHash::SeSubeAlEntrar, phc_con(hoy.m_cost(), hoy.t_cost(), hoy.p_cost(), Algorithm::Argon2i)),
            // Menos memoria, más trabajo: no se puede subir sin rebajar la memoria.
            (SituacionHash::FlojoEnMemoria, phc_con(4096, 10, 1, Algorithm::Argon2id)),
            // Más memoria, menos trabajo: el cuadrante espejo, que no se contaba en
            // ninguna parte hasta que existió este recuento.
            (SituacionHash::FlojoEnTrabajo, phc_con(38911, 1, 1, Algorithm::Argon2id)),
            // Parámetros por ENCIMA de la política y digest corto: no se puede
            // reescribir sin rebajar la memoria, pero tampoco está «al día». Hasta
            // que la auditoría lo midió, este caso caía en `AlDia` y NINGUNA prueba
            // lo notaba — el quinto eje solo se consultaba dentro de la dominancia.
            (SituacionHash::DigestCorto, phc_con_digest(hoy.m_cost() * 2, hoy.t_cost(), hoy.p_cost(), Algorithm::Argon2id, 10)),
            (SituacionHash::DigestCorto, phc_con_digest(hoy.m_cost(), hoy.t_cost() + 1, hoy.p_cost(), Algorithm::Argon2id, 16)),
            // Y la pareja que lo distingue de `SeSubeAlEntrar`: mismo digest corto,
            // pero con los parámetros dominados, SÍ se reescribe.
            (SituacionHash::SeSubeAlEntrar, phc_con_digest(hoy.m_cost(), hoy.t_cost(), hoy.p_cost(), Algorithm::Argon2id, 10)),
            // Un PHC sin digest: describe parámetros y no contiene ninguna
            // contraseña, así que su dueño no puede entrar. Decir «nada que hacer»
            // de una cuenta que no abre era la misma mentira que el volcado cierra.
            (SituacionHash::Ilegible, "$argon2id$v=19$m=19456,t=2,p=1$c2FsZGVsYXNwcnVlYmFzZGVjdWVudA".to_string()),
            (SituacionHash::Ilegible, "$2b$12$KIXQd7yPQhLpqZ0e0O9K0eXhO0LNnRZQ1B3D8dQPmZfJPjMcCsGvC".to_string()),
            (SituacionHash::Ilegible, "esto-no-es-un-phc".to_string()),
            (SituacionHash::Ilegible, "$scrypt$ln=16,r=8,p=1$c2FsZGVsYXNwcnVlYmFz$AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8".to_string()),
        ];
        for (esperada, texto) in casos {
            assert_eq!(situacion_de(&texto), esperada, "mal clasificado: {texto}");
        }

        // Y la coherencia con la decisión del acceso, que es lo que impide que este
        // recuento y el login cuenten historias distintas: `SeSubeAlEntrar` es
        // exactamente lo que `por_debajo_de_la_politica` acepta, y nada más.
        for (_, texto) in [(0, phc_con(8, 1, 1, Algorithm::Argon2id)), (1, phc_con(4096, 10, 1, Algorithm::Argon2id))] {
            let h = PasswordHash::new(&texto).unwrap();
            assert_eq!(
                situacion_de(&texto) == SituacionHash::SeSubeAlEntrar,
                por_debajo_de_la_politica(&h),
                "el recuento y el acceso no están de acuerdo sobre {texto}"
            );
        }
    }

    #[tokio::test]
    async fn un_hash_flojo_se_pone_al_dia_al_entrar_y_la_clave_sigue_sirviendo() {
        let (app, e) = servidor().await;
        let hoy = argon2::Params::DEFAULT;

        // Ada queda con un hash de coste ridículo, como el que podría haber
        // entrado por el volcado (#231).
        let flojo = hash_con(CLAVE, 8, 1, 1, argon2::Algorithm::Argon2id);
        sqlx::query("UPDATE usuarios SET clave_hash = ? WHERE id = 1")
            .bind(&flojo)
            .execute(&e.bd)
            .await
            .unwrap();
        let (_, m, _, _) = params_de(&e, 1).await;
        assert_eq!(m, 8, "el montaje de la prueba no dejó el hash flojo");

        // Entra, y entra de verdad.
        assert!(entrar(&app, "ada@x.co", CLAVE).await.is_some(), "no pudo entrar con el hash flojo");

        let (texto, m, t, p) = params_de(&e, 1).await;
        assert_ne!(texto, flojo, "el hash no se reescribió");
        assert_eq!((m, t, p), (hoy.m_cost(), hoy.t_cost(), hoy.p_cost()));

        // Y lo que de verdad importa: el hash nuevo es de la MISMA contraseña. Si
        // esto no se comprobara, un re-hash que guardara cualquier otra cosa
        // pasaría las dos afirmaciones de arriba y dejaría la cuenta perdida.
        assert!(
            entrar(&app, "ada@x.co", CLAVE).await.is_some(),
            "tras poner el hash al día, la contraseña dejó de servir"
        );
        assert!(
            entrar(&app, "ada@x.co", "otra-cualquiera").await.is_none(),
            "y cualquier contraseña abría"
        );
    }

    #[tokio::test]
    async fn un_hash_que_ya_esta_al_dia_no_se_reescribe() {
        // La mitad que discrimina. Sin ella, una regla que devolviera siempre
        // `true` pasaría la prueba de arriba — y todo el mundo pagaría un Argon2id
        // de 19 MiB extra en CADA entrada, para nada.
        let (app, e) = servidor().await;
        let (antes, _, _, _) = params_de(&e, 1).await;
        assert!(entrar(&app, "ada@x.co", CLAVE).await.is_some());
        let (despues, _, _, _) = params_de(&e, 1).await;
        assert_eq!(antes, despues, "se reescribió un hash que ya estaba al día");
    }

    #[tokio::test]
    async fn cambiar_la_contrasena_mientras_se_renueva_el_hash_no_la_pierde() {
        // El caso que convierte la mejora en pérdida de la cuenta. Entre verificar
        // y reescribir pasan un turno de semáforo y un Argon2id de 19 MiB, y en ese
        // rato la persona puede cambiar su contraseña desde otra pestaña. Sin el
        // `AND clave_hash = ?` del UPDATE, aquí se escribiría el hash de la
        // contraseña VIEJA encima de la nueva.
        //
        // Se simula llamando a la renovación con el hash viejo en la mano DESPUÉS
        // de que la fila ya lleve el nuevo, que es exactamente el estado en que la
        // carrera deja al segundo escritor.
        //
        // El hash viejo tiene que ser FLOJO, y eso no es un detalle del montaje: si
        // se le pasa uno que ya está al día, `renovar_hash_si_hace_falta` sale por
        // el `return` de la política y no llega nunca al `UPDATE` — la prueba queda
        // verde sin haber ejercido la condición que dice vigilar. Escrita así la
        // primera vez, el banco de mutación lo destapó: quitar el
        // `AND clave_hash = ?` no ponía roja ninguna prueba.
        let (_app, e) = servidor().await;
        let viejo = hash_con(CLAVE, 8, 1, 1, argon2::Algorithm::Argon2id);
        sqlx::query("UPDATE usuarios SET clave_hash = ? WHERE id = 1")
            .bind(&viejo)
            .execute(&e.bd)
            .await
            .unwrap();

        const NUEVA: &str = "la-contraseña-nueva-de-ada";
        let hash_nuevo = hashear(&e, NUEVA.to_string()).await.unwrap();
        sqlx::query("UPDATE usuarios SET clave_hash = ? WHERE id = 1")
            .bind(&hash_nuevo)
            .execute(&e.bd)
            .await
            .unwrap();

        // El rezagado llega con el hash viejo y la contraseña vieja.
        renovar_hash_si_hace_falta(&e, 1, CLAVE.to_string(), &viejo).await;

        let (ahora, _, _, _) = params_de(&e, 1).await;
        assert_eq!(ahora, hash_nuevo, "el rezagado pisó la contraseña nueva");
        assert!(
            Argon2::default()
                .verify_password(NUEVA.as_bytes(), &PasswordHash::new(&ahora).unwrap())
                .is_ok(),
            "la contraseña nueva dejó de valer"
        );
        assert!(
            Argon2::default()
                .verify_password(CLAVE.as_bytes(), &PasswordHash::new(&ahora).unwrap())
                .is_err(),
            "la contraseña vieja volvió a valer"
        );
    }

    #[tokio::test]
    async fn si_la_renovacion_no_puede_escribir_se_entra_igual() {
        // Modo degradado (#71): esto es una mejora silenciosa, no un requisito
        // para entrar. Se le da un id que no existe, así que el UPDATE afecta a
        // cero filas — el mismo camino que un fallo de escritura— y se comprueba
        // que no explota y que la puerta sigue abriendo.
        let (app, e) = servidor().await;
        let flojo = hash_con(CLAVE, 8, 1, 1, argon2::Algorithm::Argon2id);
        renovar_hash_si_hace_falta(&e, 9999, CLAVE.to_string(), &flojo).await;
        assert!(entrar(&app, "ada@x.co", CLAVE).await.is_some(), "la puerta dejó de abrir");
    }
}
