//! La sesión vive en una cookie firmada, no en un token que el JavaScript lea.
//!
//! # El TOKEN lo hace `guaca::sesion`; esto es solo la capa de cookies (2026-08-31)
//!
//! Hasta hoy este módulo tenía su propia copia del token firmado. Ya no: la
//! criptografía —emitir, verificar, `version`, dominio de firma— vive en
//! `guaca::sesion`, que es un SUPERCONJUNTO estricto de lo que había aquí (misma
//! revocación por `version`, mismo formato de tres campos, y además el prefijo de
//! longitud del dominio que cierra una ambigüedad que la copia local tenía). Lo
//! único que queda aquí es lo que guaca no hace, porque no depende de ningún
//! framework web: las cabeceras `Set-Cookie`, los nombres de cookie y la
//! extracción desde la petición. `emitir`/`verificar`/`emitir_pendiente`/
//! `verificar_pendiente` son envoltorios finos sobre guaca.
//!
//! # DESPLIEGUE: este cambio cierra la sesión de TODOS, una vez, a propósito
//!
//! guaca firma `HMAC([len(dominio) en 8 bytes]‖dominio‖carga)`; la copia vieja
//! firmaba `HMAC(dominio‖carga)` SIN la longitud. Con el dominio vacío de la sesión
//! eso ya diverge —guaca firma `HMAC([8 ceros]‖carga)`, la copia firmaba
//! `HMAC(carga)`—, así que las cookies emitidas ANTES de este despliegue no
//! verifican y todos vuelven a entrar. **No es una regresión** (guaca es más
//! estricto, no menos): es el coste aceptado de consolidar en una sola
//! implementación, y se elige el logout único frente a una ventana de doble
//! verificación que habría que desmontar después y que prolongaría el esquema
//! débil (directivas 1 y 37). Está agendado y anunciado como decisión de
//! despliegue. Lo fija el caso rojo `una_cookie_del_esquema_viejo_no_entra`: si
//! alguien lo pone verde reintroduciendo el esquema sin prefijo, habrá revivido la
//! ambigüedad que guaca arregló.
//!
//! # La decisión de fondo (por qué cookie y no JWT en `localStorage`)
//!
//! Al separar interfaz y servidor aparece la pregunta de dónde guardar la sesión.
//! La respuesta habitual —un JWT en `localStorage`— es la peor disponible aquí:
//! cualquier XSS lo exfiltra, y el servidor no puede invalidarlo antes de que
//! caduque. Como el binario **sirve la propia interfaz**, no hay segundo origen: la
//! cookie viaja sola, se marca `HttpOnly` (el JavaScript no la lee) y
//! `SameSite=Strict` (no viaja desde otro sitio, así que no hay CSRF que resolver).
//! No es una mitigación añadida: es consecuencia de servir el SPA desde el mismo
//! binario, y por eso esa decisión de despliegue es también de seguridad.
//!
//! No guarda ni el rol ni el nombre: esos se leen de la base en cada petición,
//! porque un rol dentro de la cookie sobrevive a que un administrador lo revoque.
//!
//! # Por qué `version` (revocación selectiva, #194)
//!
//! Un token firmado sin estado vale hasta que caduca y el servidor no puede
//! retirarlo. Desactivar al usuario ya lo echa —`activo` se relee en cada
//! petición—, pero eso revoca a la PERSONA, y el caso que importa es «acabo de
//! cambiar la contraseña, echa a quien entrara con la vieja» y «cerrar sesión en
//! todos los dispositivos». `version` es un entero por usuario que viaja firmado
//! dentro del token y se compara con el de la base en la MISMA relectura que ya se
//! hacía para el rol. Subirlo invalida de golpe todas las sesiones abiertas de esa
//! persona (regla 2 de `cuentas.rs`, y `POST /api/salir-todo`). El estado es un
//! entero en la fila del usuario; no hay lista de revocados ni recolección.

use axum::http::{header, HeaderMap};

/// El tipo de sesión ya verificada. Es el de guaca —mismos campos
/// (`usuario_id`, `version`, `caduca_en`)—, reexportado para que los llamantes no
/// tengan que nombrar el crate.
pub use guaca::sesion::Sesion;

pub const NOMBRE_COOKIE: &str = "siger_sesion";

/// El dominio de firma del token pendiente del segundo factor. Va como argumento a
/// `guaca::sesion::{emitir,verificar}_en`. **DEBE seguir siendo exactamente esta
/// cadena**: es lo que separa el token intermedio de una sesión de verdad —misma
/// carga, firma de otro dominio—, y guaca ancló su propio vector fijo a ella.
const DOMINIO_PENDIENTE: &str = "pendiente-de-segundo-factor|";

/// Construye el valor de la cookie de sesión para un usuario. Delega en guaca.
pub fn emitir(secreto: &[u8], usuario_id: i64, version: i64, minutos: i64) -> String {
    guaca::sesion::emitir(secreto, usuario_id, version, minutos)
}

/// Lee y **verifica** una cookie de sesión. `None` ante cualquier duda (firma,
/// formato o caducidad). Delega en guaca.
pub fn verificar(secreto: &[u8], valor: &str) -> Option<Sesion> {
    guaca::sesion::verificar(secreto, valor)
}

// --- El paso intermedio del segundo factor (#212) ------------------------------

/// Cookie del paso intermedio. Es OTRA cookie y no la de sesión con un campo más:
/// mientras el segundo factor no se comprueba, esto no es una sesión, y dejarlas
/// viajar en el mismo sitio invita a que un día una valga por la otra.
pub const NOMBRE_COOKIE_PENDIENTE: &str = "siger_pendiente";

/// Lo que dura el paso intermedio. Cinco minutos: lo que tarda alguien en sacar el
/// teléfono, no lo que dura una jornada. Es un token que ya pasó la contraseña, así
/// que su vida corta ES la mitigación.
pub const MINUTOS_PENDIENTE: i64 = 5;

/// El token que se emite cuando la contraseña es correcta pero falta el código.
///
/// **No sirve para nada más que para `POST /api/2fa/verificar`.** No lo garantiza
/// una comprobación en cada extremo —eso se olvida en el extremo número doce— sino
/// que ningún extractor de sesión lo mira: viaja en otra cookie y su firma
/// pertenece a otro dominio, así que para [`verificar`] es basura. Delega en guaca
/// con [`DOMINIO_PENDIENTE`].
pub fn emitir_pendiente(secreto: &[u8], usuario_id: i64, version: i64) -> String {
    guaca::sesion::emitir_en(secreto, DOMINIO_PENDIENTE, usuario_id, version, MINUTOS_PENDIENTE)
}

/// Lee y verifica un token pendiente. Mismas reglas que [`verificar`]: ante
/// cualquier duda, `None`. Delega en guaca con [`DOMINIO_PENDIENTE`].
pub fn verificar_pendiente(secreto: &[u8], valor: &str) -> Option<Sesion> {
    guaca::sesion::verificar_en(secreto, DOMINIO_PENDIENTE, valor)
}

/// La cabecera del paso intermedio.
pub fn cabecera_pendiente(valor: &str) -> String {
    format!(
        "{NOMBRE_COOKIE_PENDIENTE}={valor}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        MINUTOS_PENDIENTE * 60
    )
}

/// La que lo borra. Se manda **al completar** el segundo factor, y solo ahí.
///
/// Al fallar NO se borra, y es a propósito: un dedazo en seis dígitos no puede
/// costar volver a teclear la contraseña, y quien repita de verdad se topa con el
/// freno, que cubre los dos pasos.
///
/// **AVISO, medido el 2026-08-12**: el pendiente YA USADO **sobrevive**. Esto es una
/// cabecera `Set-Cookie`, o sea una instrucción a un NAVEGADOR; un cliente que hable
/// HTTP a secas se queda con el valor crudo y lo reenvía. El contador TOTP quemado
/// bloquea reutilizar el mismo código, no reutilizar el token. Lo que sí lo contiene:
/// el freno cubre los dos pasos y el primer paso NO anota el acierto cuando la cuenta
/// tiene 2FA, así que el presupuesto es el mismo con reenvío y sin él (nueve intentos
/// por token en cinco minutos, contra tres códigos de un millón → ~1 entre 37 000). Y
/// contra quien tiene la contraseña, el reenvío vale CERO: puede pedir un pendiente
/// nuevo cuando quiera. Hacerlo de un solo uso en servidor sería un mecanismo sin
/// entregable detrás (directivas 1 y 37). La salvedad que queda: el freno es por
/// `(correo, origen)` y el pendiente no está atado al origen, así que un token robado
/// da ~5·N intentos desde N direcciones; atarlo rompería al móvil legítimo que cambia
/// de IP (directiva 21).
pub fn cabecera_pendiente_cerrar() -> String {
    format!("{NOMBRE_COOKIE_PENDIENTE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0")
}

/// Extrae el valor de la cookie pendiente.
pub fn pendiente_de_cabeceras(cabeceras: &HeaderMap) -> Option<String> {
    de_cabeceras_llamada(cabeceras, NOMBRE_COOKIE_PENDIENTE)
}

/// La cabecera `Set-Cookie` de una sesión abierta.
///
/// `Secure` se deja fuera a propósito y se añade en el proxy: el perfil de una sola
/// máquina se sirve por HTTP en `localhost`, y una cookie `Secure` ahí **no se
/// guarda** — el cliente vería una pantalla de acceso que no falla ni deja entrar,
/// que es la peor forma de fallar.
pub fn cabecera_abrir(valor: &str, minutos: i64) -> String {
    format!(
        "{NOMBRE_COOKIE}={valor}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        minutos * 60
    )
}

/// La cabecera que cierra la sesión en el navegador.
pub fn cabecera_cerrar() -> String {
    format!("{NOMBRE_COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0")
}

/// Extrae el valor de la cookie de sesión de las cabeceras.
pub fn de_cabeceras(cabeceras: &HeaderMap) -> Option<String> {
    de_cabeceras_llamada(cabeceras, NOMBRE_COOKIE)
}

fn de_cabeceras_llamada(cabeceras: &HeaderMap, nombre: &str) -> Option<String> {
    let crudo = cabeceras.get(header::COOKIE)?.to_str().ok()?;
    crudo
        .split(';')
        .filter_map(|p| p.trim().split_once('='))
        .find(|(k, _)| *k == nombre)
        .map(|(_, v)| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRETO: &[u8] = b"un secreto de al menos treinta y dos bytes";

    fn ahora() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
    }

    #[test]
    fn una_sesion_emitida_se_verifica() {
        let v = emitir(SECRETO, 42, 7, 60);
        let s = verificar(SECRETO, &v).unwrap();
        assert_eq!(s.usuario_id, 42);
        assert_eq!(s.version, 7);
    }

    /// **La separación de dominios, por la vía real de ESTE módulo.** Es lo que hay
    /// que probar aquí (guaca ya prueba su cripto): que los envoltorios pasan el
    /// dominio correcto. Un token pendiente NO vale como sesión, y una sesión NO vale
    /// como pendiente —misma carga, otro dominio—. Si `DOMINIO_PENDIENTE` estuviera
    /// mal escrito, o si `emitir_pendiente` llamara a `emitir`, esto se pondría rojo.
    #[test]
    fn un_pendiente_no_vale_como_sesion_ni_al_reves() {
        let p = emitir_pendiente(SECRETO, 42, 3);
        assert!(verificar_pendiente(SECRETO, &p).is_some(), "el pendiente vale en su extremo");
        assert!(verificar(SECRETO, &p).is_none(), "un pendiente entró como sesión");

        let s = emitir(SECRETO, 42, 3, 60);
        assert!(verificar(SECRETO, &s).is_some(), "la sesión vale en el suyo");
        assert!(verificar_pendiente(SECRETO, &s).is_none(), "una sesión pasó por el 2FA");
    }

    #[test]
    fn el_pendiente_caduca_en_minutos_y_no_en_horas() {
        let p = emitir_pendiente(SECRETO, 42, 1);
        let s = verificar_pendiente(SECRETO, &p).unwrap();
        let vida = s.caduca_en - ahora();
        assert!(vida <= MINUTOS_PENDIENTE * 60, "vive {vida} s");
        assert!(vida > MINUTOS_PENDIENTE * 60 - 5, "vive {vida} s");
    }

    /// **EL CASO ROJO DEL DESPLIEGUE (directiva 33).** Una cookie firmada con el
    /// esquema VIEJO —`HMAC(dominio‖carga)`, sin el prefijo de longitud que guaca
    /// antepone— NO verifica. Es la prueba de que este cambio cierra las sesiones
    /// vivas a propósito: se firma aquí, a mano, con exactamente el algoritmo que
    /// tenía la copia local (dominio vacío → `HMAC(carga)`), y se exige que
    /// `verificar` la rechace. Si alguien reintrodujera el esquema sin prefijo para
    /// "no cerrar sesiones", esto se pondría verde y avisaría de que revivió la
    /// ambigüedad de prefijo que guaca cerró. Su pareja está arriba
    /// (`una_sesion_emitida_se_verifica`: el esquema NUEVO sí entra).
    #[test]
    fn una_cookie_del_esquema_viejo_no_entra() {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        // El algoritmo EXACTO de la copia vieja: dominio (vacío) + carga, SIN el
        // prefijo de longitud. Reproduce un token que un despliegue anterior emitió.
        let caduca = ahora() + 3600;
        let carga = format!("42|0|{caduca}");
        let mut mac = <Hmac<Sha256>>::new_from_slice(SECRETO).unwrap();
        mac.update(b""); // dominio vacío, sin longitud delante
        mac.update(carga.as_bytes());
        let firma = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        let cookie_vieja = format!("{carga}|{firma}");

        assert!(
            verificar(SECRETO, &cookie_vieja).is_none(),
            "una cookie del esquema viejo verificó: la migración no rompió el formato como debía, \
             o alguien reintrodujo la firma sin prefijo de longitud"
        );
    }

    #[test]
    fn las_dos_cookies_no_se_confunden_al_leerlas() {
        // Viajan juntas en la misma cabecera durante el paso intermedio, así que el
        // lector tiene que sacar cada una de la suya.
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            format!("{NOMBRE_COOKIE_PENDIENTE}=PPP; {NOMBRE_COOKIE}=SSS").parse().unwrap(),
        );
        assert_eq!(de_cabeceras(&h).as_deref(), Some("SSS"));
        assert_eq!(pendiente_de_cabeceras(&h).as_deref(), Some("PPP"));
    }
}
