//! La sesión vive en una cookie firmada, no en un token que el JavaScript lea.
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
    let mut mac = HmacSha256::new_from_slice(secreto).expect("HMAC acepta cualquier longitud");
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
    let crudo = cabeceras.get(header::COOKIE)?.to_str().ok()?;
    crudo
        .split(';')
        .filter_map(|p| p.trim().split_once('='))
        .find(|(k, _)| *k == NOMBRE_COOKIE)
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

    #[test]
    fn una_carga_con_campos_de_mas_no_entra() {
        let caduca = ahora() + 3600;
        let carga = format!("42|1|{caduca}|sobra");
        let rara = format!("{carga}|{}", firmar(SECRETO, &carga));
        assert!(verificar(SECRETO, &rara).is_none());
    }
}
