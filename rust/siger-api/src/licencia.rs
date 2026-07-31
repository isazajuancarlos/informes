//! Licencia offline: un archivo firmado que la aplicación verifica en local.
//!
//! Port a Rust de `license_service.py`, retirado en el cutover (#175). Un cliente
//! air-gapped no puede validar contra un servidor; la alternativa es un archivo
//! firmado con la misma criptografía post-cuántica que ya sella las entregas
//! (Ed25519 + ML-DSA-87, vía Quipu): EL LICENCIANTE lo emite con su clave privada,
//! la aplicación lo verifica con la pública, y nadie necesita red.
//!
//! # Lo que este módulo NO hace, y es la decisión más importante
//!
//! **No bloquea nada. Nunca.**
//!
//! Ni sin licencia, ni con el mantenimiento vencido, ni con una manipulada, ni con
//! exceso de usuarios. No es una omisión: es el invariante 2 del proyecto (modo
//! degradado, #71).
//!
//! > La ausencia de una capa OPCIONAL nunca debe impedirle al cliente cumplir una
//! > obligación que tiene con un TERCERO.
//!
//! Este sistema existe para que una organización no incumpla ante Supersalud, una
//! EPS o un ente de control. El día que una licencia vencida impida registrar una
//! entrega, **Xiliux pasa a ser la causa del incumplimiento del cliente** — y de
//! eso responde Xiliux, no el cliente.
//!
//! Lo que sí hace, y es lo que un licenciamiento serio necesita:
//!
//!   - **Visibilidad**: un aviso permanente y visible en la interfaz.
//!   - **Evidencia**: el estado es consultable y auditable.
//!
//! El remedio ante el exceso de uso es CONTRACTUAL (Cláusula Tercera y Anexo A del
//! EULA), no técnico. El software aporta la prueba; el contrato, la consecuencia.
//! Un DRM que se rodea con `chmod` da una falsa sensación de control y, a cambio,
//! regala el riesgo de tumbarle la operación a un hospital un lunes a las siete.
//!
//! # La licencia NO caduca, y eso no es un descuido
//!
//! `mantenimiento_hasta` NO es la vigencia de la licencia: es el fin del **Período
//! de Mantenimiento**. Confundirlos sería un error con consecuencias comerciales.
//!
//!   - El derecho de uso es **PERPETUO** (Cláusulas 3.1 y 3.2) y el no pago de la
//!     cuota anual **no lo afecta** (7.6).
//!   - La Cláusula 3.4 declara además que la validación es local y **no está
//!     sujeta a caducidad** — es argumento de venta y obligación técnica.
//!
//! Por eso, pasada esa fecha, la situación es [`Situacion::MantenimientoVencido`] y
//! NUNCA «licencia vencida»: decirle a un cliente con licencia perpetua que su
//! licencia caducó sería falso y, peor, lo empujaría a pagar por algo que ya tiene.
//!
//! # El formato
//!
//! Los campos son EXACTAMENTE los del Anexo A del EULA, para que el archivo y el
//! contrato no puedan discrepar. Si el Anexo A cambia, este módulo cambia con él.
//! El canónico —lo que se firma— es el JSON de esos diez campos con las claves
//! ORDENADAS y sin espacios, byte a byte el mismo que emitía el Python: una
//! licencia emitida antes del cutover se verifica con este código sin tocarla.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use quipu::api::{decode_verified, encode_signed};
use quipu::dictionary::Dictionary;
use quipu::pqsign::{generate_keypair, SigningKey, VerifyingKey};
use crate::hex::{a_hex, de_hex};
use serde_json::Value;

/// Ediciones válidas (Anexo A del EULA).
pub const EDICIONES: &[&str] = &["esencial", "profesional", "corporativa"];
/// Perfiles de despliegue válidos (Anexo A del EULA).
pub const PERFILES: &[&str] = &["standalone", "onprem", "cloud"];
/// Modalidades de soporte válidas (Anexo A del EULA).
pub const MODALIDADES: &[&str] = &["asistente", "asistida", "air-gapped"];

/// Nombre por defecto del archivo de licencia, junto al binario.
pub const ARCHIVO_POR_DEFECTO: &str = "licencia.quipu";

/// Días de gracia antes de declarar vencido el mantenimiento. **Uno, y es el
/// mecanismo entero de zonas horarias de este módulo.**
///
/// # La historia, porque la primera solución era peor que el problema
///
/// La emisión fechaba con `Local` y el veredicto comparaba con `Utc`. En
/// Colombia (UTC-5) eso hace que el aviso salga a las **19:00 del último día
/// pagado**: el sistema le dice al cliente algo que todavía no es verdad, que es
/// justo lo que la nota de módulo jura no hacer.
///
/// El arreglo obvio —usar `Local` en las dos mitades— **no arreglaba nada**, y lo
/// cazó la segunda revisión: `chrono::Local` sale de la zona del HOST. Si esa
/// zona no se puede resolver, `chrono` no falla: cae a `/etc/localtime` y, si
/// tampoco, a UTC. La unidad de systemd no fija `TZ` y nada en el repositorio lo
/// hace, así que en un servidor en UTC —el defecto de casi toda imagen de nube—
/// `Local` **es** `Utc` y el defecto vuelve entero, con un comentario largo
/// afirmando lo contrario. Es la directiva 20 al revés: un dato ausente
/// sustituido por un defecto, sin manera de notarlo.
///
/// # Por qué un día de gracia y no configurar la zona
///
/// Configurar la zona sería añadir un mecanismo para contener lo que creó otro
/// (directiva 1). Lo que hay que quitar es la SENSIBILIDAD al reloj.
///
/// El desfase horario máximo del planeta es de ±14 h, menos de un día. Con un
/// día de gracia sobre la fecha, **ninguna zona horaria puede ver el aviso antes
/// de que el mantenimiento haya terminado de verdad en la suya**, sea cual sea
/// el reloj del servidor. El precio es que el aviso puede aparecer hasta un día
/// tarde, y ese precio es cero: este aviso no ejecuta nada, no bloquea nada y no
/// tiene efecto contractual. Un día tarde no le cuesta nada a nadie; un día
/// pronto es el sistema mintiéndole a un cliente que está en su pleno derecho.
///
/// Y así el módulo vuelve a comparar en `Utc`, como el resto de la aplicación
/// (planificador, estadísticas, alertas): un reloj menos que explicar.
const DIAS_DE_GRACIA: i64 = 1;

/// El desfase horario máximo que existe en el planeta: UTC+14 (Kiribati) frente
/// a UTC−12. En el peor caso el servidor cree que es un día más tarde que el
/// cliente — nunca más de uno.
const HORAS_MAXIMAS_DE_DESFASE: i64 = 14;

// La gracia TIENE que cubrir ese desfase, y esto se comprueba al COMPILAR, no en
// una prueba: es la afirmación que sostiene el diseño entero —«ninguna zona
// horaria puede ver el aviso antes de tiempo»— y una prueba se puede borrar con
// el archivo que la contiene. Si alguien baja la gracia a cero «para limpiar»,
// no compila.
//
// El tope de arriba es igual de deliberado: la gracia es tolerancia de RELOJ, no
// una prórroga. Con un mes, el aviso llegaría cuando ya no sirve para nada.
const _: () = {
    // En horas, que es como se enuncia el invariante: la gracia tiene que ser al
    // menos tan larga como el desfase máximo. Expresarlo en días obligaba a un
    // `div_euclid` que decía lo mismo peor.
    assert!(DIAS_DE_GRACIA * 24 >= HORAS_MAXIMAS_DE_DESFASE);
    assert!(DIAS_DE_GRACIA <= 2);
};

/// El día de hoy. `Utc`, como el resto de la aplicación — la tolerancia a la
/// zona horaria del cliente no vive aquí, vive en [`DIAS_DE_GRACIA`].
pub fn hoy() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

/// El mismo alfabeto que `firma.rs`: ASCII imprimible `0x21..=0x7e`. Tiene que
/// coincidir con el de la rueda de Python (`default_dict`) o el artefacto emitido
/// por una implementación no se lee con la otra.
fn dict() -> Dictionary {
    Dictionary::new((0x21u8..=0x7e).map(|b| b as char).collect())
        .expect("el alfabeto ASCII por defecto es válido")
}

/// Los diez campos del Anexo A. Ni uno más: lo que no está en el Anexo no puede
/// estar aquí, o el archivo y el contrato dirían cosas distintas.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Licencia {
    /// Razón social de EL LICENCIATARIO.
    pub licenciatario: String,
    pub nit: String,
    /// Una de [`EDICIONES`].
    pub edicion: String,
    /// Uno de [`PERFILES`].
    pub perfil: String,
    /// La Versión Licenciada (Cláusula 3.1).
    pub version: String,
    /// Usuarios Autorizados.
    pub usuarios: u32,
    /// Sedes / entornos autorizados.
    pub sedes: u32,
    /// Una de [`MODALIDADES`].
    pub modalidad: String,
    /// Fecha de emisión, `AAAA-MM-DD`.
    pub emitida: String,
    /// Fin del PERÍODO DE MANTENIMIENTO, `AAAA-MM-DD`. **No** es la vigencia de
    /// la licencia: ver la nota de módulo.
    pub mantenimiento_hasta: String,
}

impl Licencia {
    /// Los bytes deterministas que se firman: los diez campos con las claves
    /// ordenadas y sin espacios. Se construye con un `BTreeMap` a propósito y no
    /// serializando la estructura — el orden de declaración es el del Anexo A,
    /// que no es el alfabético, y el canónico tiene que ser el ordenado para
    /// seguir casando con lo que firmaba el Python.
    fn canonico(&self) -> Vec<u8> {
        let mut m: BTreeMap<&'static str, Value> = BTreeMap::new();
        m.insert("licenciatario", Value::from(self.licenciatario.as_str()));
        m.insert("nit", Value::from(self.nit.as_str()));
        m.insert("edicion", Value::from(self.edicion.as_str()));
        m.insert("perfil", Value::from(self.perfil.as_str()));
        m.insert("version", Value::from(self.version.as_str()));
        m.insert("usuarios", Value::from(self.usuarios));
        m.insert("sedes", Value::from(self.sedes));
        m.insert("modalidad", Value::from(self.modalidad.as_str()));
        m.insert("emitida", Value::from(self.emitida.as_str()));
        m.insert("mantenimiento_hasta", Value::from(self.mantenimiento_hasta.as_str()));
        serde_json::to_vec(&m).expect("el canónico serializa")
    }

    /// Qué campos vienen en blanco. Una licencia a medias no puede pasar por
    /// buena: su función es DELIMITAR el alcance, y un alcance incompleto no
    /// delimita nada (directiva 20). El cero cuenta como vacío — una licencia
    /// para cero usuarios o cero sedes no autoriza nada, así que es un error de
    /// emisión, no un límite.
    fn campos_vacios(&self) -> Vec<&'static str> {
        let mut faltan = Vec::new();
        for (nombre, vacio) in [
            ("licenciatario", self.licenciatario.trim().is_empty()),
            ("nit", self.nit.trim().is_empty()),
            ("edicion", self.edicion.trim().is_empty()),
            ("perfil", self.perfil.trim().is_empty()),
            ("version", self.version.trim().is_empty()),
            ("usuarios", self.usuarios == 0),
            ("sedes", self.sedes == 0),
            ("modalidad", self.modalidad.trim().is_empty()),
            ("emitida", self.emitida.trim().is_empty()),
            ("mantenimiento_hasta", self.mantenimiento_hasta.trim().is_empty()),
        ] {
            if vacio {
                faltan.push(nombre);
            }
        }
        faltan
    }
}

/// Las cinco situaciones posibles. Son cinco y no dos porque «no hay licencia»,
/// «no la puedo verificar» y «no es válida» significan cosas muy distintas para
/// quien lo lee, y colapsarlas obligaría a la interfaz a mentir en dos de los tres
/// casos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Situacion {
    /// Firma buena y mantenimiento vigente.
    Valida,
    /// No hay archivo de licencia. Normal en evaluación.
    Ausente,
    /// Firma que no valida, clave equivocada, archivo corrupto o campos en
    /// blanco. Es la ÚNICA que merece color de alarma.
    Invalida,
    /// Firma buena y Período de Mantenimiento terminado. El derecho de uso sigue
    /// intacto: es perpetuo.
    MantenimientoVencido,
    /// Falta la clave pública del licenciante, así que no se puede emitir
    /// veredicto. No se dice «inválida»: no sabemos si lo es.
    SinVerificador,
}

/// El veredicto sobre la licencia instalada.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Estado {
    pub situacion: Situacion,
    /// Texto corto, ya redactado para la interfaz.
    pub mensaje: String,
    /// Los datos si se pudieron leer y verificar, AUNQUE el mantenimiento esté
    /// vencido: una licencia vencida sigue delimitando el alcance contratado.
    pub datos: Option<Licencia>,
}

impl Estado {
    fn de(situacion: Situacion, mensaje: impl Into<String>, datos: Option<Licencia>) -> Self {
        Self { situacion, mensaje: mensaje.into(), datos }
    }

    /// ¿La interfaz tiene que decir algo? Todo lo que no sea vigente se avisa.
    pub fn hay_que_avisar(&self) -> bool {
        self.situacion != Situacion::Valida
    }

    /// ¿Con color de alarma? **Solo** si la licencia no cuadra.
    ///
    /// NO lo es el mantenimiento vencido: el derecho de uso es PERPETUO
    /// (Cláusulas 3.1, 3.2 y 7.6) y lo único que se pierde son actualizaciones y
    /// soporte. Pintarlo de rojo asustaría a un cliente que está en su pleno
    /// derecho y, peor, entrenaría a ignorar el aviso que sí importa. Tampoco lo
    /// es la ausencia de licencia, normal en evaluación.
    pub fn es_grave(&self) -> bool {
        self.situacion == Situacion::Invalida
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorEmitir {
    #[error("faltan campos obligatorios del Anexo A: {0}")]
    Incompleta(String),
    #[error("«{1}» no es un valor de {0} del Anexo A; los válidos son: {2}")]
    FueraDelAnexo(&'static str, String, String),
    #[error("{0} tiene que ser una fecha AAAA-MM-DD, y es «{1}»")]
    Fecha(&'static str, String),
    #[error(
        "la clave de firma no es válida. Tiene que ser el hex que imprime \
         `siger-api generar-claves-licencia` como INFORMES_LICENCIA_SK."
    )]
    ClaveDeFirma,
}

fn del_anexo(campo: &'static str, valor: &str, validos: &[&str]) -> Result<(), ErrorEmitir> {
    if validos.contains(&valor) {
        Ok(())
    } else {
        Err(ErrorEmitir::FueraDelAnexo(campo, valor.to_string(), validos.join(", ")))
    }
}

fn fecha(campo: &'static str, s: &str) -> Result<NaiveDate, ErrorEmitir> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| ErrorEmitir::Fecha(campo, s.to_string()))
}

/// Emite una licencia firmada. **Lo usa EL LICENCIANTE, no el cliente.**
///
/// Aquí sí se valida contra las listas del Anexo A: la emisión es el único
/// momento en que un valor fuera de lista es un error de quien escribe, y
/// atraparlo aquí evita mandarle al cliente una licencia con «profesionall».
pub fn emitir(l: &Licencia, sk_hex: &str) -> Result<String, ErrorEmitir> {
    let faltan = l.campos_vacios();
    if !faltan.is_empty() {
        return Err(ErrorEmitir::Incompleta(faltan.join(", ")));
    }
    del_anexo("edicion", &l.edicion, EDICIONES)?;
    del_anexo("perfil", &l.perfil, PERFILES)?;
    del_anexo("modalidad", &l.modalidad, MODALIDADES)?;
    fecha("emitida", &l.emitida)?;
    fecha("mantenimiento_hasta", &l.mantenimiento_hasta)?;

    let sk = de_hex(sk_hex)
        .and_then(|b| SigningKey::from_bytes(&b))
        .ok_or(ErrorEmitir::ClaveDeFirma)?;
    Ok(encode_signed(&l.canonico(), &sk, &dict()))
}

/// Verifica el contenido firmado de una licencia y dictamina.
///
/// `firmado` es lo que hay en el archivo; `None` significa que no hay archivo.
/// `vk_hex` es la clave pública del licenciante; `None` o ilegible da
/// [`Situacion::SinVerificador`] y **no** «inválida», porque no lo sabemos.
///
/// No se cachea a propósito: una licencia que se sustituye en caliente tiene que
/// reflejarse sin reiniciar, y cachear el veredicto es cómo se consigue que la
/// interfaz mienta durante horas.
pub fn veredicto(firmado: Option<&str>, vk_hex: Option<&str>, hoy: NaiveDate) -> Estado {
    let Some(vk) = vk_hex.and_then(de_hex).and_then(|b| VerifyingKey::from_bytes(&b)) else {
        return Estado::de(
            Situacion::SinVerificador,
            "No se puede verificar la licencia: falta o es ilegible \
             INFORMES_LICENCIA_PUBKEY. El sistema funciona con todas sus funciones.",
            None,
        );
    };

    let firmado = firmado.map(str::trim).filter(|s| !s.is_empty());
    let Some(firmado) = firmado else {
        return Estado::de(
            Situacion::Ausente,
            "Sin licencia instalada. El sistema funciona con todas sus funciones; \
             el uso se rige por el contrato.",
            None,
        );
    };

    // Firma que no valida, clave equivocada o archivo corrupto: todo da el MISMO
    // resultado. Distinguirlos no ayudaría a nadie honesto y sí orientaría a
    // quien intente fabricar una.
    let invalida = || {
        Estado::de(
            Situacion::Invalida,
            "La licencia instalada no es válida o fue alterada. Avise a EL LICENCIANTE. \
             El sistema sigue funcionando con todas sus funciones.",
            None,
        )
    };
    let Ok(crudo) = decode_verified(firmado, &vk, &dict()) else {
        return invalida();
    };
    let Ok(l) = serde_json::from_slice::<Licencia>(&crudo) else {
        return invalida();
    };
    if !l.campos_vacios().is_empty() {
        return invalida();
    }

    // A partir de aquí la firma es BUENA, así que los datos se devuelven pase lo
    // que pase con las fechas: son el alcance contratado.
    let Ok(hasta) = NaiveDate::parse_from_str(l.mantenimiento_hasta.trim(), "%Y-%m-%d") else {
        return Estado::de(
            Situacion::Invalida,
            "La licencia tiene una fecha de mantenimiento ilegible. Avise a EL LICENCIANTE.",
            Some(l),
        );
    };

    // El día de gracia va AQUÍ y no en la fecha emitida: lo que se guarda
    // firmado sigue siendo la fecha del contrato, exacta. La tolerancia es de la
    // comparación, no del dato — ver `DIAS_DE_GRACIA`.
    if hoy > hasta + chrono::Duration::days(DIAS_DE_GRACIA) {
        // NO se dice «la licencia venció»: LA LICENCIA NO VENCE.
        return Estado::de(
            Situacion::MantenimientoVencido,
            format!(
                "Mantenimiento vencido el {hasta}. El derecho de uso es perpetuo y no se \
                 ve afectado; lo que cesa es el acceso a actualizaciones y soporte."
            ),
            Some(l),
        );
    }

    Estado::de(Situacion::Valida, format!("Mantenimiento vigente hasta {hasta}."), Some(l))
}

/// ¿Se superó el número de Usuarios Autorizados del Anexo A?
///
/// Devuelve el aviso, o `None` si no hay exceso o no hay con qué compararlo.
/// **No bloquea**: informa. El remedio es contractual (Cláusula Tercera) y el
/// software solo aporta la prueba.
pub fn excede_usuarios(e: &Estado, activos: u32) -> Option<String> {
    let tope = e.datos.as_ref()?.usuarios;
    (activos > tope).then(|| {
        format!("Usuarios activos ({activos}) por encima de los autorizados en el Anexo A ({tope}).")
    })
}

/// Genera el par del licenciante: `(pubkey_hex, signing_key_hex)`. La segunda es
/// SECRETA y **es el secreto que sostiene todo el licenciamiento**: quien la tenga
/// puede emitir licencias válidas. No vive en el repositorio ni viaja al cliente.
pub fn generar_claves() -> (String, String) {
    let (vk, sk) = generate_keypair();
    (a_hex(&vk.to_bytes()), a_hex(&sk.to_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;


    fn ejemplo() -> Licencia {
        Licencia {
            licenciatario: "ACME S.A.S.".into(),
            nit: "900123456-1".into(),
            edicion: "profesional".into(),
            perfil: "onprem".into(),
            version: "4.0".into(),
            usuarios: 25,
            sedes: 3,
            modalidad: "asistida".into(),
            emitida: "2026-07-31".into(),
            mantenimiento_hasta: "2027-07-31".into(),
        }
    }

    fn dia(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// El canónico es el JSON ORDENADO y compacto — no el orden de declaración
    /// de la estructura, que es el del Anexo A. Si esto cambia, las licencias
    /// emitidas por el Python de antes del cutover dejan de verificarse.
    #[test]
    fn el_canonico_va_ordenado_y_sin_espacios() {
        let c = String::from_utf8(ejemplo().canonico()).unwrap();
        assert!(c.starts_with(r#"{"edicion":"profesional","emitida":"2026-07-31","licenciatario""#), "{c}");
        assert!(!c.contains(", "), "el canónico no lleva espacios: {c}");
        // Los números van como números, no como cadenas: es lo que emitía el
        // Python y lo que el Anexo A significa.
        assert!(c.contains(r#""usuarios":25"#), "{c}");
    }

    #[test]
    fn una_licencia_emitida_se_verifica() {
        let (vk, sk) = generar_claves();
        let firmado = emitir(&ejemplo(), &sk).unwrap();
        let e = veredicto(Some(&firmado), Some(&vk), dia("2026-08-01"));
        assert_eq!(e.situacion, Situacion::Valida, "{}", e.mensaje);
        assert_eq!(e.datos.unwrap(), ejemplo());
    }

    /// El invariante que manda sobre todos los demás: NADA de esto bloquea. La
    /// prueba es que las cinco situaciones son alcanzables y ninguna es un error
    /// que el llamante tenga que propagar — `veredicto` devuelve `Estado`, no
    /// `Result`, y `es_grave` solo es cierto en UNA.
    #[test]
    fn ninguna_situacion_bloquea_y_solo_una_alarma() {
        let (vk, sk) = generar_claves();
        let firmado = emitir(&ejemplo(), &sk).unwrap();
        let (otra_vk, _) = generar_claves();

        let casos = [
            (veredicto(Some(&firmado), Some(&vk), dia("2026-08-01")), Situacion::Valida, false),
            (veredicto(None, Some(&vk), dia("2026-08-01")), Situacion::Ausente, false),
            (veredicto(Some(&firmado), Some(&otra_vk), dia("2026-08-01")), Situacion::Invalida, true),
            (veredicto(Some(&firmado), Some(&vk), dia("2028-01-01")), Situacion::MantenimientoVencido, false),
            (veredicto(Some(&firmado), None, dia("2026-08-01")), Situacion::SinVerificador, false),
        ];
        for (e, esperada, grave) in casos {
            assert_eq!(e.situacion, esperada, "{}", e.mensaje);
            assert_eq!(e.es_grave(), grave, "{esperada:?}: {}", e.mensaje);
            assert!(!e.mensaje.is_empty(), "{esperada:?} sin mensaje para la interfaz");
        }
    }

    /// El mantenimiento vencido NO se llama «licencia vencida», y no es por
    /// estilo: el derecho de uso es perpetuo (3.1, 3.2, 7.6) y esa frase
    /// empujaría al cliente a pagar por algo que ya tiene.
    #[test]
    fn el_mantenimiento_vencido_no_dice_que_la_licencia_vencio() {
        let (vk, sk) = generar_claves();
        let firmado = emitir(&ejemplo(), &sk).unwrap();
        let e = veredicto(Some(&firmado), Some(&vk), dia("2028-01-01"));
        assert_eq!(e.situacion, Situacion::MantenimientoVencido);
        let m = e.mensaje.to_lowercase();
        assert!(!m.contains("licencia vencida") && !m.contains("licencia venció"), "{}", e.mensaje);
        assert!(m.contains("perpetuo"), "el aviso tiene que decir qué NO se pierde: {}", e.mensaje);
        // Y sigue trayendo el alcance contratado: una licencia vencida delimita
        // igual, y sin los datos la interfaz no podría seguir mostrándolo.
        assert_eq!(e.datos.unwrap().usuarios, 25);
    }

    /// El día EXACTO del vencimiento está vigente, y el siguiente TAMBIÉN.
    ///
    /// Lo primero porque el Período de Mantenimiento va «hasta» esa fecha,
    /// inclusive: un `>=` le quitaría al cliente un día que pagó. Lo segundo es
    /// [`DIAS_DE_GRACIA`], y no es generosidad: el reloj del servidor puede ir
    /// hasta 14 h por delante del calendario del cliente, así que sin gracia
    /// habría zonas horarias en las que el aviso sale antes de ser verdad.
    ///
    /// Este par de asertos es lo que impide que alguien «limpie» la comparación
    /// quitando la gracia: el segundo se cae.
    #[test]
    fn el_ultimo_dia_del_mantenimiento_esta_vigente_y_el_siguiente_tambien() {
        let (vk, sk) = generar_claves();
        let firmado = emitir(&ejemplo(), &sk).unwrap();
        let situacion = |d: &str| veredicto(Some(&firmado), Some(&vk), dia(d)).situacion;

        assert_eq!(situacion("2027-07-31"), Situacion::Valida, "el último día pagado");
        assert_eq!(situacion("2027-08-01"), Situacion::Valida, "el día de gracia");
        assert_eq!(situacion("2027-08-02"), Situacion::MantenimientoVencido, "ya sin excusa");
    }

    /// Lo que la comprobación de compilación NO puede decir: que la gracia
    /// tenga el efecto que se le pide sobre un caso concreto.
    ///
    /// El `const _: ()` de arriba sujeta la ARITMÉTICA (gracia ≥ desfase
    /// máximo). Esto sujeta la CONDUCTA: simulando el peor servidor posible —uno
    /// que va un día por delante del calendario del cliente—, el aviso todavía
    /// no ha salido el último día que el cliente pagó.
    #[test]
    fn ni_el_servidor_mas_adelantado_avisa_antes_de_tiempo() {
        let (vk, sk) = generar_claves();
        let firmado = emitir(&ejemplo(), &sk).unwrap();

        // El cliente vive su último día pagado: 2027-07-31. Un servidor en
        // UTC+14 puede creer que ya es el 1 de agosto.
        let ultimo_dia_del_cliente = dia("2027-07-31");
        let lo_que_cree_el_servidor = ultimo_dia_del_cliente + chrono::Duration::days(1);

        assert_eq!(
            veredicto(Some(&firmado), Some(&vk), lo_que_cree_el_servidor).situacion,
            Situacion::Valida,
            "el cliente está en su último día pagado y el servidor va adelantado: \
             no se le puede decir que venció"
        );
    }

    /// Una licencia alterada byte a byte no pasa. Es la prueba de que la firma
    /// hace algo: sin ella, cualquiera se sube los usuarios autorizados con un
    /// editor de texto.
    #[test]
    fn una_licencia_manipulada_no_pasa() {
        let (vk, sk) = generar_claves();
        let firmado = emitir(&ejemplo(), &sk).unwrap();

        // Cambiar un carácter cualquiera del artefacto.
        let mut bytes = firmado.clone().into_bytes();
        let i = bytes.len() / 2;
        bytes[i] = if bytes[i] == b'A' { b'B' } else { b'A' };
        let tocado = String::from_utf8(bytes).unwrap();
        assert_ne!(tocado, firmado, "la mutación tiene que cambiar algo");

        let e = veredicto(Some(&tocado), Some(&vk), dia("2026-08-01"));
        assert_eq!(e.situacion, Situacion::Invalida, "{}", e.mensaje);
        assert!(e.es_grave());
        // Y no se filtra NADA de lo que decía: ni siquiera si llegó a parsear.
        assert!(e.datos.is_none());
    }

    /// Firmada de verdad, pero por OTRO. Es el caso que una comprobación de
    /// «¿parsea el JSON?» dejaría pasar entero.
    #[test]
    fn una_licencia_firmada_por_otra_clave_no_pasa() {
        let (_, sk_ajena) = generar_claves();
        let (vk, _) = generar_claves();
        let firmado = emitir(&ejemplo(), &sk_ajena).unwrap();
        assert_eq!(
            veredicto(Some(&firmado), Some(&vk), dia("2026-08-01")).situacion,
            Situacion::Invalida
        );
    }

    /// Sin clave pública NO se dice «inválida»: no lo sabemos. Decirlo pintaría
    /// de rojo la instalación de un cliente cuyo único pecado es no haber puesto
    /// una variable de entorno.
    ///
    /// La lista incluye claves con caracteres MULTIBYTE, y no por exotismo: la
    /// forma obvia de decodificar hex rebana el `&str` por índices de byte y
    /// entra en pánico si el corte cae dentro de un carácter. Esto corre en cada
    /// consulta, así que una tilde pegada por error en el `.env` mataría la
    /// petición en vez de dar «sin verificador». Lo cazó una revisión de
    /// seguridad; `firma.rs` tenía la misma copia y va arreglada igual.
    #[test]
    fn sin_clave_publica_no_se_acusa_a_la_licencia() {
        let (_, sk) = generar_claves();
        let firmado = emitir(&ejemplo(), &sk).unwrap();
        for vk in [
            None,
            Some(""),
            Some("no-es-hex"),
            Some("abc"),
            Some("€a"),          // 3 bytes + 1: el corte caería dentro del €
            Some("añb1"),        // multibyte en medio
            Some("—"),           // guion tipográfico, lo que pega un gestor de correo
        ] {
            let e = veredicto(Some(&firmado), vk, dia("2026-08-01"));
            assert_eq!(e.situacion, Situacion::SinVerificador, "vk={vk:?}: {}", e.mensaje);
            assert!(!e.es_grave(), "vk={vk:?}");
        }
    }

    /// Una licencia incompleta no pasa por buena aunque la firma sea impecable:
    /// su función es DELIMITAR, y un alcance incompleto no delimita nada.
    #[test]
    fn una_licencia_incompleta_no_se_emite_ni_se_acepta() {
        let (vk, sk) = generar_claves();

        for romper in [
            (|l: &mut Licencia| l.licenciatario.clear()) as fn(&mut Licencia),
            |l: &mut Licencia| l.nit.clear(),
            |l: &mut Licencia| l.usuarios = 0,
            |l: &mut Licencia| l.sedes = 0,
            |l: &mut Licencia| l.mantenimiento_hasta.clear(),
        ] {
            let mut l = ejemplo();
            romper(&mut l);
            // Ni se emite…
            assert!(matches!(emitir(&l, &sk), Err(ErrorEmitir::Incompleta(_))), "{l:?}");
            // …ni se acepta si alguien la firmó rodeando `emitir`.
            let sk_bytes = de_hex(&sk).unwrap();
            let clave = SigningKey::from_bytes(&sk_bytes).unwrap();
            let a_mano = encode_signed(&l.canonico(), &clave, &dict());
            assert_eq!(
                veredicto(Some(&a_mano), Some(&vk), dia("2026-08-01")).situacion,
                Situacion::Invalida,
                "{l:?}"
            );
        }
    }

    /// La emisión rechaza lo que el Anexo A no contempla. Es el único sitio
    /// donde se valida contra las listas, y es a propósito: ver la prueba
    /// siguiente.
    #[test]
    fn la_emision_rechaza_lo_que_no_esta_en_el_anexo() {
        let (_, sk) = generar_claves();
        for romper in [
            (|l: &mut Licencia| l.edicion = "profesionall".into()) as fn(&mut Licencia),
            |l: &mut Licencia| l.perfil = "nube".into(),
            |l: &mut Licencia| l.modalidad = "presencial".into(),
        ] {
            let mut l = ejemplo();
            romper(&mut l);
            assert!(matches!(emitir(&l, &sk), Err(ErrorEmitir::FueraDelAnexo(..))), "{l:?}");
        }
        // Y una fecha que no es fecha tampoco sale firmada.
        let mut l = ejemplo();
        l.mantenimiento_hasta = "el año que viene".into();
        assert!(matches!(emitir(&l, &sk), Err(ErrorEmitir::Fecha(..))), "{l:?}");
    }

    /// Y el VEREDICTO deliberadamente NO valida contra esas listas.
    ///
    /// Si mañana el Anexo A añade una edición, el licenciante la emite con su
    /// binario nuevo y el cliente la instala en el suyo, que es viejo. Un binario
    /// que rechazara la edición desconocida pintaría de ROJO —«no es válida o fue
    /// alterada»— una licencia legítima y correctamente firmada. La firma es la
    /// que dice si el documento es auténtico; la lista solo dice si quien lo
    /// escribió se equivocó al teclear, y de eso responde la emisión.
    #[test]
    fn el_veredicto_no_rechaza_una_edicion_que_todavia_no_conoce() {
        let (vk, sk) = generar_claves();
        let mut l = ejemplo();
        l.edicion = "corporativa-plus".into(); // no existe hoy
        let sk_bytes = de_hex(&sk).unwrap();
        let clave = SigningKey::from_bytes(&sk_bytes).unwrap();
        let firmado = encode_signed(&l.canonico(), &clave, &dict());

        let e = veredicto(Some(&firmado), Some(&vk), dia("2026-08-01"));
        assert_eq!(e.situacion, Situacion::Valida, "{}", e.mensaje);
        assert_eq!(e.datos.unwrap().edicion, "corporativa-plus");
    }

    /// El exceso de usuarios se INFORMA. Que no bloquee no se prueba aquí: se
    /// prueba en que esta función devuelve un texto y nadie puede devolver otra
    /// cosa desde ella.
    #[test]
    fn el_exceso_de_usuarios_se_informa_y_no_impide_nada() {
        let (vk, sk) = generar_claves();
        let firmado = emitir(&ejemplo(), &sk).unwrap();
        let e = veredicto(Some(&firmado), Some(&vk), dia("2026-08-01"));

        assert!(excede_usuarios(&e, 25).is_none(), "el tope EXACTO no excede");
        assert!(excede_usuarios(&e, 24).is_none());
        let aviso = excede_usuarios(&e, 26).expect("26 > 25 excede");
        assert!(aviso.contains("26") && aviso.contains("25"), "{aviso}");

        // Sin licencia no hay con qué comparar, y NO se inventa un tope: sin
        // datos no hay aviso (directiva 20 — no se rellena con un defecto).
        let sin = veredicto(None, Some(&vk), dia("2026-08-01"));
        assert!(excede_usuarios(&sin, 10_000).is_none());
    }
}
