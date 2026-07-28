//! Firma post-cuántica de las entregas con Quipu (port de `signing_service.py`).
//!
//! Da **integridad** y **no repudio** de los campos auditables de una entrega
//! —lo que un ente de control pregunta—, no confidencialidad: la firma va en
//! claro (Ed25519 + ML-DSA-87). Se firma DENTRO de la transacción que registra
//! la entrega, por la razón de siempre: una segunda pasada puede no llegar a
//! ocurrir y dejaría entregas firmables sin firmar sin que nadie lo note.
//!
//! # Custodia
//!
//! Perfil `entorno` (el implementado): la clave privada vive en
//! `QUIPU_SIGNING_KEY`. Su límite hay que decirlo claro: quien lea el entorno del
//! proceso tiene la clave, y con ella puede firmar entregas que nunca
//! ocurrieron. El perfil `hsm` (PKCS#11) queda para cuando un comité de seguridad
//! lo exija; Quipu ya lo soporta vía `encode_signed_con_custodio`, y el artefacto
//! es idéntico, así que este verificador lo valida sin cambios.
//!
//! Sin clave, el servicio se desactiva CON GRACIA: [`firmar`] devuelve `None` y
//! la entrega se registra sin firma. La ausencia de firma nunca impide cumplir.

use std::collections::BTreeMap;

use quipu::api::{decode_verified, encode_signed};
use quipu::dictionary::Dictionary;
use quipu::pqsign::{generate_keypair, SigningKey, VerifyingKey};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// El alfabeto por defecto de Quipu: ASCII imprimible `0x21..=0x7e`. DEBE ser el
/// mismo que usa la rueda de Python (`default_dict`), o las firmas no se
/// verifican entre implementaciones.
fn dict() -> Dictionary {
    Dictionary::new((0x21u8..=0x7e).map(|b| b as char).collect())
        .expect("el alfabeto ASCII por defecto es válido")
}

/// SHA-256 en hex del contenido del archivo. Así la firma cubre el DOCUMENTO, no
/// solo los metadatos.
pub fn sha256_hex(datos: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(datos);
    format!("{:x}", h.finalize())
}

fn de_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

fn a_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Los campos auditables de una entrega. Es lo que se firma; cualquier cambio
/// posterior en cualquiera de ellos invalida la firma.
#[derive(Debug, Clone)]
pub struct Auditable<'a> {
    pub id: i64,
    pub informe_id: i64,
    pub usuario_id: Option<i64>,
    pub fecha_limite: &'a str,
    pub fecha_entrega: Option<&'a str>,
    pub estado: &'a str,
    pub archivo_sha256: Option<&'a str>,
    pub fecha_vencimiento_nominal: &'a str,
    pub tipo_ajuste: &'a str,
}

/// Representación DETERMINISTA (v2) de los campos auditables. Mismo esquema que
/// el `canonico` de Python: JSON con claves ORDENADAS y separadores compactos.
/// Un `BTreeMap` garantiza el orden; `serde_json` compacta sin espacios y no
/// escapa el UTF-8 — los mismos bytes que
/// `json.dumps(sort_keys=True, separators=(",",":"), ensure_ascii=False)`.
///
/// El `"v":2` viaja DENTRO de lo firmado: no se puede cambiar de versión sin
/// romper la firma. v2 añade a qué obligación pertenece y bajo qué ajuste vencía,
/// sin lo cual la firma acreditaría una fecha suelta, no un cumplimiento.
pub fn canonico(a: &Auditable) -> Vec<u8> {
    let mut m: BTreeMap<&str, Value> = BTreeMap::new();
    m.insert("id", Value::from(a.id));
    m.insert("informe_id", Value::from(a.informe_id));
    m.insert("usuario_id", a.usuario_id.map(Value::from).unwrap_or(Value::Null));
    m.insert("fecha_limite", Value::from(a.fecha_limite));
    m.insert("fecha_entrega", a.fecha_entrega.map(Value::from).unwrap_or(Value::Null));
    m.insert("estado", Value::from(a.estado));
    m.insert("archivo_sha256", a.archivo_sha256.map(Value::from).unwrap_or(Value::Null));
    m.insert("v", Value::from(2));
    m.insert("fecha_vencimiento_nominal", Value::from(a.fecha_vencimiento_nominal));
    m.insert("tipo_ajuste", Value::from(a.tipo_ajuste));
    serde_json::to_vec(&m).expect("el canónico serializa")
}

/// Firma los campos auditables con la clave del perfil `entorno`. `None` si no
/// hay clave (desactivado con gracia) o si la clave configurada no es válida.
pub fn firmar(a: &Auditable, sk_hex: Option<&str>) -> Option<String> {
    let sk = SigningKey::from_bytes(&de_hex(sk_hex?)?)?;
    Some(encode_signed(&canonico(a), &sk, &dict()))
}

/// Verifica que `firma` corresponde a los campos auditables ACTUALES. Distingue
/// dos fallos que no son lo mismo: firma inválida o forjada, y entrega alterada
/// tras firmarse.
pub fn verificar(a: &Auditable, firma: &str, vk_hex: Option<&str>) -> (bool, &'static str) {
    let Some(vk_hex) = vk_hex else {
        return (false, "no hay clave de verificación configurada");
    };
    let Some(vk) = de_hex(vk_hex).and_then(|b| VerifyingKey::from_bytes(&b)) else {
        return (false, "clave de verificación inválida");
    };
    match decode_verified(firma, &vk, &dict()) {
        Ok(firmado) if firmado == canonico(a) => (true, "verificada"),
        Ok(_) => (false, "la entrega fue alterada tras la firma"),
        Err(_) => (false, "firma inválida o forjada"),
    }
}

/// Genera un par de claves de firma: `(verifying_key_hex, signing_key_hex)`. La
/// segunda es SECRETA y va en `QUIPU_SIGNING_KEY`.
pub fn generar_claves() -> (String, String) {
    let (vk, sk) = generate_keypair();
    (a_hex(&vk.to_bytes()), a_hex(&sk.to_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ejemplo() -> Auditable<'static> {
        Auditable {
            id: 7,
            informe_id: 3,
            usuario_id: Some(11),
            fecha_limite: "2026-01-15",
            fecha_entrega: Some("2026-01-14T10:00:00+00:00"),
            estado: "CUMPLIDO",
            archivo_sha256: Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
            fecha_vencimiento_nominal: "2026-01-15",
            tipo_ajuste: "NINGUNO",
        }
    }

    #[test]
    fn el_canonico_va_ordenado_y_compacto_como_el_de_python() {
        // Byte a byte lo que produce `json.dumps(sort_keys=True,
        // separators=(",",":"), ensure_ascii=False)`. Si esto cambia, una firma
        // hecha por la otra implementación deja de verificar.
        let a = Auditable {
            id: 1,
            informe_id: 2,
            usuario_id: None,
            fecha_limite: "2026-01-15",
            fecha_entrega: None,
            estado: "PENDIENTE",
            archivo_sha256: None,
            fecha_vencimiento_nominal: "2026-01-15",
            tipo_ajuste: "NINGUNO",
        };
        let c = String::from_utf8(canonico(&a)).unwrap();
        assert_eq!(
            c,
            r#"{"archivo_sha256":null,"estado":"PENDIENTE","fecha_entrega":null,"fecha_limite":"2026-01-15","fecha_vencimiento_nominal":"2026-01-15","id":1,"informe_id":2,"tipo_ajuste":"NINGUNO","usuario_id":null,"v":2}"#
        );
    }

    #[test]
    fn firma_y_verifica_con_cripto_real() {
        let (vk, sk) = generar_claves();
        let a = ejemplo();
        let firma = firmar(&a, Some(&sk)).expect("firma con clave válida");
        assert_eq!(verificar(&a, &firma, Some(&vk)), (true, "verificada"));
    }

    #[test]
    fn una_entrega_alterada_tras_firmar_no_verifica() {
        let (vk, sk) = generar_claves();
        let a = ejemplo();
        let firma = firmar(&a, Some(&sk)).unwrap();
        // Cambiar el estado tras firmar es exactamente lo que un ente de control
        // teme: la firma tiene que delatarlo, y decir que fue ALTERADA, no que es
        // inválida — son acusaciones distintas.
        let alterada = Auditable { estado: "INCUMPLIDO", ..a };
        assert_eq!(verificar(&alterada, &firma, Some(&vk)), (false, "la entrega fue alterada tras la firma"));
    }

    #[test]
    fn una_firma_de_otra_clave_no_verifica() {
        let (_vk, sk) = generar_claves();
        let (otra_vk, _otra_sk) = generar_claves();
        let a = ejemplo();
        let firma = firmar(&a, Some(&sk)).unwrap();
        let (ok, _) = verificar(&a, &firma, Some(&otra_vk));
        assert!(!ok, "una firma no puede verificar contra una clave ajena");
    }

    #[test]
    fn sin_clave_no_se_firma_pero_no_se_rompe() {
        assert!(firmar(&ejemplo(), None).is_none());
        assert!(!verificar(&ejemplo(), "loquesea", None).0);
    }
}
