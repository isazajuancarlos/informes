//! La sesión vive en una cookie firmada, no en un token que el JavaScript lea.
//!
//! # Y sí, esto duplica `guaca::sesion` — pero al revés de como parece
//!
//! `guaca::sesion` existe justo para que esto no estuviera copiado, y el propio
//! `guaca` lo dice: «medico e informes copiaban la misma sesión […] y las copias
//! YA habían divergido». Así que la tentación correcta —borrar este módulo y
//! llamar a guaca— hay que resistirla, porque **la copia débil es la de guaca**:
//!
//! | | `guaca::sesion` | este módulo |
//! |---|---|---|
//! | carga | `id\|caducidad` | `id\|version\|caducidad` |
//! | revocación selectiva (#194) | **no la tiene** | `version`, ver abajo |
//! | campos exigidos | los que salgan | exactamente tres |
//! | dominio de firma | uno solo | dos, y separa el 2FA (#212) |
//! | cabeceras `Set-Cookie` | no (no depende de axum) | sí |
//!
//! Cambiar este módulo por el de guaca hoy no simplificaría nada: quitaría la
//! revocación —o sea la regla 2 de `cuentas.rs` («cambiar la contraseña echa a
//! quien entró con la vieja») y `POST /api/salir-todo` enteros— y dejaría el
//! token pendiente del segundo factor sin la separación de dominio que es toda
//! su seguridad. Sería un cambio que se ve como limpieza y es una regresión.
//!
//! Medido el 2026-08-04: **medico también le añadió `token_version` a SU copia**
//! (`crates/medico-api/src/sesion.rs`), por su cuenta y con otro parser. O sea
//! que hay tres versiones de esto y la que vive en la librería común es la única
//! que no sirve para lo que las dos aplicaciones necesitan. La deduplicación va
//! en la dirección contraria a la obvia: **subir `version` (y el dominio de
//! firma) a `guaca::sesion` y que los dos consuman de ahí**. Eso es trabajo en el
//! repositorio de guaca —otra propiedad, otra instancia, y aquí se consume por
//! `rev` fijado—, así que queda registrado como tarea y no se hace desde aquí.
//! Lo que NO se hace mientras tanto es fingir que no está duplicado.
//!
//! Lo que sí se dedujo el 2026-08-04, porque ahí guaca sí es la buena: el hash de
//! contraseñas, que era otra copia y hoy es `guaca::claves` (ver `cuentas.rs`).
//!
//! # La decisión, y por qué es esta
//!
//! Al separar interfaz y servidor aparece la pregunta de dónde guardar la
//! sesión. La respuesta habitual —un JWT en `localStorage`— es la peor de las
//! disponibles aquí: cualquier XSS lo exfiltra, y el servidor no puede
//! invalidarlo antes de que caduque.
//!
//! Como el binario **sirve la propia interfaz**, no hay segundo origen: la
//! cookie viaja sola, se marca `HttpOnly` (el JavaScript no puede leerla) y
//! `SameSite=Strict` (no viaja desde otro sitio, así que no hay CSRF que
//! resolver). Esto no es una mitigación añadida: es una consecuencia de servir
//! el SPA desde el mismo binario, y por eso esa decisión de despliegue es
//! también una decisión de seguridad.
//!
//! La cookie lleva `id|version|caducidad|firma`. La firma es HMAC-SHA256 con el
//! secreto del servidor, comparada en tiempo constante. No guarda ni el rol ni el
//! nombre: esos se leen de la base en cada petición, porque un rol dentro de la
//! cookie sobrevive a que un administrador lo revoque.
//!
//! # Por qué `version` (revocación selectiva, #194)
//!
//! Un token firmado sin estado tiene una pega conocida: emitido, vale hasta que
//! caduca, y el servidor no puede retirarlo. Desactivar al usuario ya lo echa
//! —`activo` se relee en cada petición—, pero eso es revocar a la PERSONA, y el
//! caso que importa es otro: «acabo de cambiar la contraseña, echa a quien
//! entrara con la vieja» y «cerrar sesión en todos los dispositivos».
//!
//! `version` es un entero por usuario que viaja firmado dentro del token y se
//! compara con el de la base en la MISMA relectura que ya se hacía para el rol.
//! Subirlo invalida de golpe todas las sesiones abiertas de esa persona. No hay
//! lista de tokens revocados ni recolección de basura: el estado es un entero en
//! la fila del usuario.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::http::{header, HeaderMap};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub const NOMBRE_COOKIE: &str = "siger_sesion";

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sesion {
    pub usuario_id: i64,
    /// La versión de token con la que se emitió. Vale mientras coincida con la
    /// del usuario en la base; ver el módulo.
    pub version: i64,
    pub caduca_en: i64,
}

fn ahora() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn firmar(secreto: &[u8], carga: &str) -> String {
    firmar_en(secreto, "", carga)
}

/// Firma dentro de un DOMINIO.
///
/// El dominio entra en lo que se firma y no viaja en la cookie: dos tokens con la
/// misma carga y distinto dominio tienen firmas que no se parecen, así que
/// ninguno de los dos verificadores acepta el token del otro. Es lo que separa la
/// sesión de verdad del token «pendiente de segundo factor» — ver
/// [`emitir_pendiente`].
///
/// La sesión normal usa el dominio VACÍO, no uno con nombre: así las cookies ya
/// emitidas siguen valiendo. Ponerle nombre habría sido más simétrico y habría
/// echado a todo el mundo el día del despliegue, a cambio de nada.
fn firmar_en(secreto: &[u8], dominio: &str, carga: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secreto).expect("HMAC acepta cualquier longitud");
    mac.update(dominio.as_bytes());
    mac.update(carga.as_bytes());
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

/// Construye el valor de la cookie para un usuario.
pub fn emitir(secreto: &[u8], usuario_id: i64, version: i64, minutos: i64) -> String {
    let caduca = ahora() + minutos * 60;
    let carga = format!("{usuario_id}|{version}|{caduca}");
    let firma = firmar(secreto, &carga);
    format!("{carga}|{firma}")
}

/// Lee y **verifica**. Devuelve `None` ante cualquier duda: firma que no cuadra,
/// formato raro o caducada. No hay camino intermedio ni mensaje distinto para
/// cada caso — decirle al cliente cuál de los tres falló solo le sirve a quien
/// está probando.
pub fn verificar(secreto: &[u8], valor: &str) -> Option<Sesion> {
    let (carga, firma) = valor.rsplit_once('|')?;
    let esperada = firmar(secreto, carga);

    // Comparación en tiempo constante: `==` sobre cadenas termina en la primera
    // diferencia y filtra, byte a byte, cuánto acertó quien lo intenta.
    if !constante_iguales(esperada.as_bytes(), firma.as_bytes()) {
        return None;
    }

    // Exactamente tres campos. Un `split_once` dejaría pasar una carga con
    // campos de más —lo sobrante acabaría dentro del último `parse`, o peor,
    // ignorado— y un token viejo de dos campos entraría con la versión leída de
    // la caducidad. La firma cuadra con lo que se firmó; el FORMATO también
    // tiene que cuadrar.
    let campos: Vec<&str> = carga.split('|').collect();
    let [id, version, caduca] = campos[..] else {
        return None;
    };
    let sesion = Sesion {
        usuario_id: id.parse().ok()?,
        version: version.parse().ok()?,
        caduca_en: caduca.parse().ok()?,
    };
    if sesion.caduca_en <= ahora() {
        return None;
    }
    Some(sesion)
}

// --- El paso intermedio del segundo factor (#212) ------------------------------

/// El dominio de firma del token pendiente. Ver [`firmar_en`].
const DOMINIO_PENDIENTE: &str = "pendiente-de-segundo-factor|";

/// Cookie del paso intermedio. Es OTRA cookie y no la de sesión con un campo
/// más: mientras el segundo factor no se comprueba, esto no es una sesión, y
/// dejarlas viajar en el mismo sitio invita a que un día una valga por la otra.
pub const NOMBRE_COOKIE_PENDIENTE: &str = "siger_pendiente";

/// Lo que dura el paso intermedio. Cinco minutos: lo que tarda alguien en sacar
/// el teléfono, no lo que dura una jornada. Es un token que ya pasó la
/// contraseña, así que su vida corta ES la mitigación.
pub const MINUTOS_PENDIENTE: i64 = 5;

/// El token que se emite cuando la contraseña es correcta pero falta el código.
///
/// **No sirve para nada más que para `POST /api/2fa/verificar`.** No lo garantiza
/// una comprobación en cada extremo —eso se olvida en el extremo número doce—
/// sino que ningún extractor de sesión lo mira: viaja en otra cookie y su firma
/// pertenece a otro dominio, así que para [`verificar`] es basura.
pub fn emitir_pendiente(secreto: &[u8], usuario_id: i64, version: i64) -> String {
    let caduca = ahora() + MINUTOS_PENDIENTE * 60;
    let carga = format!("{usuario_id}|{version}|{caduca}");
    let firma = firmar_en(secreto, DOMINIO_PENDIENTE, &carga);
    format!("{carga}|{firma}")
}

/// Lee y verifica un token pendiente. Mismas reglas que [`verificar`]: ante
/// cualquier duda, `None`.
pub fn verificar_pendiente(secreto: &[u8], valor: &str) -> Option<Sesion> {
    let (carga, firma) = valor.rsplit_once('|')?;
    let esperada = firmar_en(secreto, DOMINIO_PENDIENTE, carga);
    if !constante_iguales(esperada.as_bytes(), firma.as_bytes()) {
        return None;
    }
    let campos: Vec<&str> = carga.split('|').collect();
    let [id, version, caduca] = campos[..] else {
        return None;
    };
    let s = Sesion {
        usuario_id: id.parse().ok()?,
        version: version.parse().ok()?,
        caduca_en: caduca.parse().ok()?,
    };
    if s.caduca_en <= ahora() {
        return None;
    }
    Some(s)
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
/// freno, que cubre los dos pasos. Lo que sí no puede sobrevivir es el pendiente
/// YA USADO — si valiera después de abrir sesión sería un segundo intento gratis
/// con el código siguiente—, y de eso se ocupan esta cabecera y el contador
/// quemado en `segundofactor::verificar`.
pub fn cabecera_pendiente_cerrar() -> String {
    format!("{NOMBRE_COOKIE_PENDIENTE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0")
}

/// Extrae el valor de la cookie pendiente.
pub fn pendiente_de_cabeceras(cabeceras: &HeaderMap) -> Option<String> {
    de_cabeceras_llamada(cabeceras, NOMBRE_COOKIE_PENDIENTE)
}

fn constante_iguales(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// La cabecera `Set-Cookie` de una sesión abierta.
///
/// `Secure` se deja fuera a propósito y se añade en el proxy: el perfil de una
/// sola máquina se sirve por HTTP en `localhost`, y una cookie `Secure` ahí
/// **no se guarda** — el cliente vería una pantalla de acceso que no falla ni
/// deja entrar, que es la peor forma de fallar.
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

/// Extrae el valor de la cookie de las cabeceras.
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

    #[test]
    fn una_sesion_emitida_se_verifica() {
        let v = emitir(SECRETO, 42, 7, 60);
        let s = verificar(SECRETO, &v).unwrap();
        assert_eq!(s.usuario_id, 42);
        assert_eq!(s.version, 7);
    }

    #[test]
    fn cambiar_el_usuario_invalida_la_firma() {
        // La prueba que discrimina: sin ella, un verificador que siempre
        // devuelve `Some` pasaría la anterior.
        let v = emitir(SECRETO, 42, 1, 60);
        let manipulada = v.replacen("42|", "1|", 1);
        assert!(verificar(SECRETO, &manipulada).is_none());
    }

    #[test]
    fn bajar_la_version_a_mano_invalida_la_firma() {
        // La versión va DENTRO de lo firmado. Si alguien pudiera bajarla en el
        // navegador, la revocación no revocaría nada — sería un campo decorativo.
        let v = emitir(SECRETO, 42, 9, 60);
        let manipulada = v.replacen("42|9|", "42|1|", 1);
        assert!(verificar(SECRETO, &manipulada).is_none());
    }

    #[test]
    fn otro_secreto_no_vale() {
        let v = emitir(SECRETO, 42, 1, 60);
        assert!(verificar(b"otro secreto igualmente largo de 32+", &v).is_none());
    }

    #[test]
    fn una_sesion_caducada_no_entra() {
        let v = emitir(SECRETO, 42, 1, -1);
        assert!(verificar(SECRETO, &v).is_none());
    }

    #[test]
    fn una_cookie_del_formato_viejo_no_entra() {
        // El caso que obliga a exigir tres campos. Una cookie `id|caduca|firma`
        // emitida ANTES de esta migración lleva una firma que cuadra —se firmó
        // justo esa carga—, así que la comprobación criptográfica la deja pasar
        // y solo el formato la detiene. Si entrara, lo haría sin versión: una
        // sesión inmune a la revocación.
        let caduca = ahora() + 3600;
        let carga = format!("42|{caduca}");
        let vieja = format!("{carga}|{}", firmar(SECRETO, &carga));
        assert!(verificar(SECRETO, &vieja).is_none(), "una cookie de dos campos entró");
    }

    /// **La prueba del segundo factor entero.**
    ///
    /// Si un token pendiente valiera como sesión, teclear la contraseña bastaría
    /// para entrar y el código sería decoración. La ficha #212 proponía meter el
    /// estado pendiente como un campo más dentro de la carga firmada; aquí no se
    /// puede —`verificar` exige exactamente tres campos, y eso tiene su prueba—,
    /// así que la separación es de DOMINIO DE FIRMA: misma carga, firma distinta.
    #[test]
    fn un_token_pendiente_no_vale_como_sesion_ni_al_reves() {
        let pendiente = emitir_pendiente(SECRETO, 42, 3);
        assert!(
            verificar(SECRETO, &pendiente).is_none(),
            "un token pendiente entró como sesión: el segundo factor no pinta nada"
        );

        let sesion = emitir(SECRETO, 42, 3, 60);
        assert!(
            verificar_pendiente(SECRETO, &sesion).is_none(),
            "una sesión completa pasó por el extremo del segundo factor"
        );

        // Y la mitad que evita que esto pase por la razón equivocada: cada uno
        // SÍ vale en su sitio, con los mismos datos dentro.
        let p = verificar_pendiente(SECRETO, &pendiente).expect("el pendiente vale en su extremo");
        assert_eq!((p.usuario_id, p.version), (42, 3));
        let s = verificar(SECRETO, &sesion).expect("la sesión vale en el suyo");
        assert_eq!((s.usuario_id, s.version), (42, 3));
    }

    #[test]
    fn el_pendiente_caduca_en_minutos_y_no_en_horas() {
        // Cinco minutos, no la vida de una sesión: ya pasó la contraseña, así que
        // lo único que lo contiene es que dure poco.
        let p = emitir_pendiente(SECRETO, 42, 1);
        let s = verificar_pendiente(SECRETO, &p).unwrap();
        let vida = s.caduca_en - ahora();
        assert!(vida <= MINUTOS_PENDIENTE * 60, "vive {vida} s");
        assert!(vida > MINUTOS_PENDIENTE * 60 - 5, "vive {vida} s");
    }

    #[test]
    fn las_dos_cookies_no_se_confunden_al_leerlas() {
        // Viajan juntas en la misma cabecera durante el paso intermedio, así que
        // el lector tiene que sacar cada una de la suya: si `de_cabeceras`
        // devolviera la pendiente, el extractor de sesión la rechazaría por firma
        // —bien— pero el usuario vería «no autorizado» en vez de la casilla del
        // código.
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            format!("{NOMBRE_COOKIE_PENDIENTE}=PPP; {NOMBRE_COOKIE}=SSS").parse().unwrap(),
        );
        assert_eq!(de_cabeceras(&h).as_deref(), Some("SSS"));
        assert_eq!(pendiente_de_cabeceras(&h).as_deref(), Some("PPP"));
    }

    #[test]
    fn una_carga_con_campos_de_mas_no_entra() {
        let caduca = ahora() + 3600;
        let carga = format!("42|1|{caduca}|sobra");
        let rara = format!("{carga}|{}", firmar(SECRETO, &carga));
        assert!(verificar(SECRETO, &rara).is_none());
    }
}
