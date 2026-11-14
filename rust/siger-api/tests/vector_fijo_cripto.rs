// SPDX-License-Identifier: LicenseRef-Xiliux-Comercial
//! **Vector fijo PROPIO de SIGER** (ancla de compatibilidad hacia atrás).
//!
//! Un blob de `guaca::reposo` (cifrado en reposo de entregas y secretos TOTP) y una
//! firma de `guaca::firma` (firma de entregas, Ley 527) REALES, capturados bajo quipu
//! 0.15.0 el 2026-09-11 y pegados como literales (archivos de `vectores/`). Su prueba
//! compara contra ellos: si se pone ROJA significa que lo cifrado/firmado hasta hoy
//! **dejó de leerse** —entregas cuya firma pasa a leerse como forjada, archivos que no
//! descifran—.
//!
//! Existe porque el resto de las pruebas de SIGER CIFRAN y VERIFICAN dentro de la MISMA
//! versión, así que un cambio de formato pasaría verde; la cobertura de dato-viejo vivía
//! en OTRO crate (guaca), que el verde de SIGER no ejecuta. Con esto, el próximo salto
//! de quipu que rompa reposo o firma se caza AQUÍ. Es la observación <8 del concilio de
//! la migración a 0.15 (2026-09-11).
//!
//! Cada aguja va con su pareja roja (directiva 33).

fn de_hex(s: &str) -> Vec<u8> {
    let s = s.trim();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

const REPOSO_FRASE: &str = include_str!("vectores/reposo_frase.txt");
const REPOSO_DATOS_HEX: &str = include_str!("vectores/reposo_datos.hex");
const REPOSO_BLOB_HEX: &str = include_str!("vectores/reposo_blob.hex");
const FIRMA_PUBLICA: &str = include_str!("vectores/firma_publica.hex");
const FIRMA_PREIMAGEN_HEX: &str = include_str!("vectores/firma_preimagen.hex");
const FIRMA: &str = include_str!("vectores/firma.txt");

#[test]
fn un_blob_de_reposo_de_2026_sigue_descifrando() {
    let blob = de_hex(REPOSO_BLOB_HEX);
    let claro = guaca::reposo::descifrar(&blob, REPOSO_FRASE.trim())
        .expect("el blob fijo de 2026 debe descifrar bajo el quipu de hoy");
    assert_eq!(
        claro,
        de_hex(REPOSO_DATOS_HEX),
        "el claro recuperado no coincide: el formato de reposo cambió"
    );
}

#[test]
fn el_blob_de_reposo_discrimina() {
    let mut blob = de_hex(REPOSO_BLOB_HEX);
    let n = blob.len() / 2;
    blob[n] ^= 0x01;
    assert!(
        guaca::reposo::descifrar(&blob, REPOSO_FRASE.trim()).is_err(),
        "un blob alterado NO debe descifrar (control de la aguja)"
    );
    assert!(
        guaca::reposo::descifrar(&de_hex(REPOSO_BLOB_HEX), "frase-incorrecta-de-mas-de-30-bytes").is_err(),
        "la frase equivocada NO debe descifrar"
    );
}

#[test]
fn una_firma_de_entrega_de_2026_sigue_verificando() {
    let pre = de_hex(FIRMA_PREIMAGEN_HEX);
    assert!(
        matches!(
            guaca::firma::verificar(&pre, FIRMA.trim(), FIRMA_PUBLICA.trim()),
            guaca::firma::Verificacion::Valida
        ),
        "la firma fija de 2026 dejó de verificar: el formato de firma cambió"
    );
}

#[test]
fn la_firma_discrimina() {
    let mut pre = de_hex(FIRMA_PREIMAGEN_HEX);
    pre[0] ^= 0x01;
    assert!(
        !matches!(
            guaca::firma::verificar(&pre, FIRMA.trim(), FIRMA_PUBLICA.trim()),
            guaca::firma::Verificacion::Valida
        ),
        "una preimagen alterada NO debe dar Valida (control de la aguja)"
    );
    let (otra_pub, _) = guaca::firma::generar_claves();
    assert!(
        !matches!(
            guaca::firma::verificar(&de_hex(FIRMA_PREIMAGEN_HEX), FIRMA.trim(), otra_pub.trim()),
            guaca::firma::Verificacion::Valida
        ),
        "otra clave pública NO debe validar la firma"
    );
}
