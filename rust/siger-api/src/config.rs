//! La configuración se lee una vez, y si falta algo el binario **no arranca**.
//!
//! # Por qué esto es un módulo con errores y no un puñado de `unwrap_or`
//!
//! El sistema anterior traía `STORAGE_SECRET = "SECRET_KEY_FINAL_V6"` como valor
//! por defecto, escrito además en `.env.example`. Es la clave con la que se
//! firma la sesión: cualquiera que leyera el repositorio podía firmarse una
//! sesión de administrador en la instalación de un cliente que no la hubiera
//! cambiado — y el defecto invita a no cambiarla. La única señal era un aviso en
//! el arranque, y **un aviso en el log no es fallar**: nadie lo lee.
//!
//! Aquí no hay valor por defecto para nada que sea un secreto. Sin él no hay
//! binario en marcha, así que no existe la instalación insegura silenciosa. El
//! instalador genera el secreto y lo escribe; el cliente nunca tiene que decidir
//! nada al respecto.

use std::env::{self, VarError};

#[derive(Debug, thiserror::Error)]
pub enum ErrorConfig {
    #[error(
        "falta {0}. No hay valor por defecto a propósito: es un secreto, y un secreto \
         con valor conocido es el mismo secreto para todas las instalaciones. \
         Genérelo con `siger-api generar-secreto` y póngalo en el entorno."
    )]
    SecretoAusente(&'static str),
    #[error("{0} tiene {1} bytes; hacen falta al menos {2}")]
    SecretoCorto(&'static str, usize, usize),
    #[error("{0} no es un número válido: {1}")]
    NoEsNumero(&'static str, String),
    #[error("el calendario no se pudo construir: {0}")]
    Calendario(#[from] siger_core::ErrorCalendario),
    #[error(
        "las alertas están activas (ALERTAS_ACTIVAS=true) pero falta {0}. Una alerta \
         sin servidor de correo es una alerta que no llega: o se configura entera, o se \
         apaga. Ponga {0} o quite ALERTAS_ACTIVAS."
    )]
    CorreoIncompleto(&'static str),
}

/// Cómo cifra el canal SMTP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsCorreo {
    /// STARTTLS sobre el puerto de envío (587). Lo normal en producción.
    StartTls,
    /// TLS implícito (465).
    Directo,
    /// Sin cifrar. SOLO para un buzón local de pruebas: jamás contra un servidor
    /// real, donde las credenciales viajarían en claro.
    Ninguno,
}

/// Configuración del correo de alertas. Solo existe si las alertas están activas;
/// si están, todos sus campos esenciales son obligatorios (se falla al arrancar,
/// no al primer envío, cuando ya nadie mira).
#[derive(Debug, Clone)]
pub struct ConfigCorreo {
    pub host: String,
    pub puerto: u16,
    pub usuario: Option<String>,
    pub clave: Option<String>,
    /// El remitente de las alertas (`From`).
    pub desde: String,
    pub tls: TlsCorreo,
    /// Cuántos días antes del límite se avisa de la proximidad.
    pub dias_proximidad: i64,
    /// A quién se avisa. Configurable; por defecto, a los tres.
    pub a_operador: bool,
    pub a_supervision: bool,
    pub a_tercero: bool,
}

/// Bytes mínimos del secreto de sesión. 32 es el tamaño de la salida de
/// SHA-256: por debajo, la firma es más débil que el algoritmo que la produce.
const MINIMO_SECRETO: usize = 32;

#[derive(Debug, Clone)]
pub struct Config {
    pub url_bd: String,
    pub puerto: u16,
    pub secreto_sesion: Vec<u8>,
    pub pais_iso: String,
    pub minutos_sesion: i64,
    /// Dónde se guardan los archivos de las entregas. Ninguna ruta que llegue
    /// del navegador se usa jamás: el servidor construye la ruta desde el id.
    pub dir_entregas: String,

    /// Tamaño máximo de un archivo subido, en MB.
    ///
    /// En la versión anterior este ajuste existía en la configuración **y en la
    /// pantalla de administración**, y no se leía en ninguna ruta de subida: el
    /// administrador lo ajustaba creyendo que aplicaba. Aquí se aplica en el
    /// límite del cuerpo de la petición, así que el archivo grande se corta
    /// antes de llegar a memoria, no después.
    pub max_mb: usize,

    /// Extensiones admitidas, en minúsculas y con punto.
    ///
    /// Es un tope general del sistema; cada informe puede exigir menos, nunca
    /// más. Una lista vacía significa «cualquiera», y hay que escribirla
    /// explícitamente: el permiso amplio se pide, no se hereda de un descuido.
    pub extensiones: Vec<String>,

    /// Clave de firma de entregas (hex), perfil de custodia `entorno`. `None`
    /// desactiva la firma CON GRACIA: la entrega se registra igual, sin firma —
    /// la ausencia de firma nunca impide cumplir. Es un secreto, pero opcional:
    /// no firmar es una postura válida, a diferencia del secreto de sesión.
    pub firma_sk_hex: Option<String>,
    /// Clave de verificación (hex), pública. Sin ella no se puede verificar lo
    /// firmado, pero sí seguir firmando.
    pub firma_vk_hex: Option<String>,

    /// Frase de cifrado EN REPOSO de los archivos de entrega (A1). `None` =
    /// desactivado CON GRACIA: los archivos se guardan en claro, como antes —
    /// cifrar es una postura que se elige, y no cifrar nunca impide entregar.
    /// Cada fila recuerda si su archivo está cifrado (`archivo_cifrado`), así que
    /// encender la frase no invalida los archivos ya guardados en claro; y la
    /// firma sigue cubriendo el SHA del DOCUMENTO (texto plano), no del blob.
    pub clave_reposo: Option<String>,

    /// Correo de alertas. `None` = dormido: no se envía nada. Es la postura por
    /// defecto, para que una instalación nueva no intente mandar correos antes de
    /// que alguien decida por dónde. Se enciende con `ALERTAS_ACTIVAS=true`.
    pub correo: Option<ConfigCorreo>,

    /// Si el binario está detrás de un proxy inverso de confianza
    /// (`SIGER_TRAS_PROXY=1`), de quien se acepta la cabecera `X-Real-IP` como
    /// origen del cliente.
    ///
    /// **El defecto es `false`, y no es timidez.** El freno al ensayo de
    /// contraseñas cuenta por pareja (correo, origen); si el origen saliera de una
    /// cabecera que el cliente puede escribir, bastaría con cambiarla en cada
    /// petición para no llegar nunca al umbral — el freno existiría y no frenaría
    /// nada. Sin proxy, el origen es la dirección del socket, que el cliente no
    /// elige.
    ///
    /// Encenderlo sin un proxy que REESCRIBA la cabecera (el `informes.xiliux.com`
    /// del repositorio pone `proxy_set_header X-Real-IP $remote_addr`, que pisa lo
    /// que venga) devuelve justo ese agujero. Por eso se pide explícitamente.
    pub tras_proxy: bool,
}

fn booleano(clave: &str, defecto: bool) -> bool {
    match opcional(clave) {
        None => defecto,
        Some(v) => matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "si" | "sí" | "yes"),
    }
}

/// Construye la configuración de correo si las alertas están activas. Falla
/// ruidosamente si están activas pero incompletas: lo esencial es el host y el
/// remitente. Usuario y clave son opcionales —un relé interno puede no pedir
/// autenticación—, pero si no hay a quién avisar (los tres destinatarios en
/// falso) tampoco tiene sentido, y se dice.
fn correo_desde_entorno() -> Result<Option<ConfigCorreo>, ErrorConfig> {
    if !booleano("ALERTAS_ACTIVAS", false) {
        return Ok(None);
    }
    let host = opcional("SMTP_HOST").ok_or(ErrorConfig::CorreoIncompleto("SMTP_HOST"))?;
    let desde = opcional("SMTP_DESDE").ok_or(ErrorConfig::CorreoIncompleto("SMTP_DESDE"))?;
    let tls = match opcional("SMTP_TLS").unwrap_or_else(|| "starttls".into()).trim().to_lowercase().as_str() {
        "starttls" => TlsCorreo::StartTls,
        "directo" | "tls" => TlsCorreo::Directo,
        "ninguno" | "none" => TlsCorreo::Ninguno,
        otro => return Err(ErrorConfig::NoEsNumero("SMTP_TLS", otro.to_string())),
    };
    let puerto = numero("SMTP_PUERTO", if tls == TlsCorreo::Directo { 465u16 } else { 587 })?;

    let a_operador = booleano("ALERTAS_A_OPERADOR", true);
    let a_supervision = booleano("ALERTAS_A_SUPERVISION", true);
    let a_tercero = booleano("ALERTAS_A_TERCERO", true);
    if !(a_operador || a_supervision || a_tercero) {
        return Err(ErrorConfig::CorreoIncompleto("al menos un destinatario (ALERTAS_A_*)"));
    }

    Ok(Some(ConfigCorreo {
        host,
        puerto,
        usuario: opcional("SMTP_USUARIO"),
        clave: opcional("SMTP_CLAVE"),
        desde,
        tls,
        dias_proximidad: numero("ALERTAS_DIAS_PROXIMIDAD", 3i64)?,
        a_operador,
        a_supervision,
        a_tercero,
    }))
}

fn opcional(clave: &str) -> Option<String> {
    match env::var(clave) {
        Ok(v) if !v.trim().is_empty() => Some(v),
        Ok(_) | Err(VarError::NotPresent) => None,
        Err(VarError::NotUnicode(_)) => None,
    }
}

fn numero<T: std::str::FromStr>(clave: &'static str, defecto: T) -> Result<T, ErrorConfig> {
    match opcional(clave) {
        None => Ok(defecto),
        Some(v) => v
            .trim()
            .parse()
            .map_err(|_| ErrorConfig::NoEsNumero(clave, v)),
    }
}

impl Config {
    /// Una configuración válida para las PRUEBAS, sin tocar el entorno.
    ///
    /// El entorno es global al proceso y las pruebas corren en paralelo: montarlo
    /// con `set_var` haría que una prueba cambiara la configuración de otra, y el
    /// fallo aparecería de forma intermitente y en el sitio equivocado.
    #[cfg(test)]
    pub fn para_pruebas() -> Self {
        Self {
            url_bd: "sqlite::memory:".into(),
            puerto: 0,
            secreto_sesion: b"secreto-de-prueba-de-mas-de-treinta-y-dos-bytes".to_vec(),
            pais_iso: "CO".into(),
            minutos_sesion: 60,
            dir_entregas: "storage/entregas".into(),
            max_mb: 50,
            extensiones: vec![".pdf".into()],
            firma_sk_hex: None,
            firma_vk_hex: None,
            clave_reposo: None,
            correo: None,
            tras_proxy: false,
        }
    }

    pub fn desde_entorno() -> Result<Self, ErrorConfig> {
        let secreto = opcional("SIGER_SECRETO_SESION")
            .ok_or(ErrorConfig::SecretoAusente("SIGER_SECRETO_SESION"))?;
        let secreto = secreto.into_bytes();
        if secreto.len() < MINIMO_SECRETO {
            return Err(ErrorConfig::SecretoCorto(
                "SIGER_SECRETO_SESION",
                secreto.len(),
                MINIMO_SECRETO,
            ));
        }

        // La frase de reposo, si se pone, debe ser tan robusta como el secreto de
        // sesión: una frase corta debilita el Argon2 que protege los archivos, y
        // ningún despliegue legítimo necesita una corta (se deriva de un secreto).
        let clave_reposo = opcional("SIGER_CLAVE_REPOSO");
        if let Some(f) = &clave_reposo {
            if f.len() < MINIMO_SECRETO {
                return Err(ErrorConfig::SecretoCorto("SIGER_CLAVE_REPOSO", f.len(), MINIMO_SECRETO));
            }
        }

        Ok(Self {
            // La base SÍ tiene defecto: no es un secreto, y el perfil de una
            // sola máquina es el que se entrega instalado.
            url_bd: opcional("DATABASE_URL").unwrap_or_else(|| "sqlite://siger.db".into()),
            puerto: numero("PORT", 8003u16)?,
            secreto_sesion: secreto,
            pais_iso: opcional("PAIS_ISO").unwrap_or_else(|| "CO".into()),
            minutos_sesion: numero("SIGER_MINUTOS_SESION", 60i64)?,
            dir_entregas: opcional("SIGER_DIR_ENTREGAS")
                .unwrap_or_else(|| "storage/entregas".into()),
            max_mb: numero("SIGER_MAX_MB", 50usize)?,
            extensiones: opcional("SIGER_EXTENSIONES")
                .unwrap_or_else(|| ".pdf,.xlsx,.docx,.zip,.csv,.xml".into())
                .split(',')
                .map(|e| e.trim().to_lowercase())
                .filter(|e| !e.is_empty())
                .collect(),
            // Firma opcional (perfil `entorno`): sin clave, se registra sin firmar.
            firma_sk_hex: opcional("QUIPU_SIGNING_KEY"),
            firma_vk_hex: opcional("QUIPU_VERIFYING_KEY"),
            // Cifrado en reposo opcional (validado arriba): sin frase, en claro.
            clave_reposo,
            // Alertas dormidas por defecto; se encienden con ALERTAS_ACTIVAS.
            correo: correo_desde_entorno()?,
            tras_proxy: booleano("SIGER_TRAS_PROXY", false),
        })
    }

    /// Configuración mínima para pruebas: base en memoria, sin firma ni correo.
    #[cfg(test)]
    pub fn de_prueba() -> Self {
        Self {
            url_bd: "sqlite::memory:".into(),
            puerto: 0,
            secreto_sesion: vec![0u8; MINIMO_SECRETO],
            pais_iso: "CO".into(),
            minutos_sesion: 60,
            dir_entregas: "/tmp".into(),
            max_mb: 50,
            extensiones: vec![],
            firma_sk_hex: None,
            firma_vk_hex: None,
            clave_reposo: None,
            correo: None,
            tras_proxy: false,
        }
    }
}
