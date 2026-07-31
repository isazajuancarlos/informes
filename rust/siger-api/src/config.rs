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
    #[error(
        "SIGER_ESCUCHA no es una dirección IP válida: «{0}». Ponga una dirección \
         (por ejemplo 127.0.0.1 o 0.0.0.0), no un nombre de máquina ni un puerto."
    )]
    NoEsDireccion(String),
    #[error("{0} tiene que ser un entero de 1 en adelante, y es «{1}»")]
    NoEsPositivo(&'static str, String),
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

    /// En qué dirección escucha el servidor. Ver [`direccion_de_escucha`].
    pub escucha: std::net::IpAddr,

    /// Cuántos descifrados de archivo pueden correr A LA VEZ. Ver
    /// [`permisos_de_cripto`]. Compartido por todos los clones de `Config`,
    /// que es justo lo que hace de él un tope del PROCESO y no de la petición.
    pub cripto: std::sync::Arc<tokio::sync::Semaphore>,
}

/// Cuántos descifrados simultáneos admite el proceso.
///
/// # Por qué hay un tope, medido y no supuesto
///
/// Descifrar un archivo de entrega cuesta un Argon2id completo: **162 ms y
/// 64 MiB**, y —esto es lo que sorprende— **da igual el tamaño del archivo**: 8 KiB
/// y 4 MiB cuestan lo mismo, porque el gasto es la derivación de la clave, no el
/// AEAD. Cada blob lleva su propia sal aleatoria, así que la clave derivada no se
/// puede reutilizar entre archivos: el coste es por archivo y es inherente al
/// formato, no un descuido.
///
/// Con 64 MiB por descifrado, «cuántos a la vez» ES el consumo de memoria del
/// servidor: 8 en paralelo son 512 MiB y 32 son casi 2 GiB, medidos. En un VPS de
/// 2 GB eso no es lentitud, es el matarife de memoria del núcleo llevándose el
/// proceso — y con él las sesiones de todo el mundo.
///
/// # Y por qué el tope tiene que ser EXPLÍCITO
///
/// Antes había uno accidental: el descifrado se hacía en el hilo del runtime, así
/// que no podían correr más que hilos de trabajo tiene tokio. Salía barato en
/// memoria y carísimo en otra cosa —bloqueaba el servidor entero, ver
/// `leer_en_claro`—. Al mover el descifrado a `spawn_blocking`, ese tope
/// accidental DESAPARECE: el vivero de tokio admite 512 hilos por defecto, o sea
/// **32 GiB** de Argon2id simultáneo. Arreglar el bloqueo sin poner este semáforo
/// habría cambiado un problema de latencia por uno de memoria mucho peor.
///
/// Por eso las dos cosas son un solo diseño y no dos parches (directiva 1).
///
/// # La cascada
///
/// | `SIGER_CRIPTO_PARALELA` | resultado |
/// |---|---|
/// | puesta | lo que diga el operador (mínimo 1) |
/// | ausente | tantos como núcleos, mínimo 1 |
///
/// El defecto conserva el consumo de memoria que el sistema ya tenía —núcleos ×
/// 64 MiB— y solo le quita el bloqueo. Quien tenga memoria de sobra y quiera más
/// descargas en paralelo lo sube; quien vaya justo lo baja a 1. Un valor que no
/// sea un entero positivo **no arranca**: caer a un defecto dejaría al operador
/// creyendo que acotó la memoria mientras el servidor hace otra cosa.
pub fn permisos_de_cripto(valor: Option<&str>) -> Result<usize, ErrorConfig> {
    match valor.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => match v.parse::<usize>() {
            Ok(n) if n >= 1 => Ok(n),
            _ => Err(ErrorConfig::NoEsPositivo("SIGER_CRIPTO_PARALELA", v.to_string())),
        },
        None => Ok(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)),
    }
}

/// La dirección en la que liga el servidor, en cascada.
///
/// | `SIGER_ESCUCHA` | `tras_proxy` | resultado |
/// |---|---|---|
/// | puesta | cualquiera | lo que diga el operador |
/// | ausente | sí | `127.0.0.1` |
/// | ausente | no | `0.0.0.0` |
///
/// # Por qué el defecto depende de la otra bandera
///
/// Declarar `SIGER_TRAS_PROXY=1` es decir «a mí me habla el proxy». Si además el
/// binario sigue escuchando en `0.0.0.0`, cualquiera que alcance el puerto se
/// salta el proxy — y entonces **elige su propio `X-Real-IP`**, con lo que el
/// freno del login no engancha nunca: basta rotar la cabecera en cada intento.
/// El control existiría y no frenaría, que es el mismo fallo que ya mordió en
/// medico por el otro lado (allí la cabecera era `X-Forwarded-For`).
///
/// Así que la declaración arrastra su consecuencia: quien dice estar detrás de un
/// proxy, por defecto no se expone.
///
/// # Y por qué es un defecto y no una regla
///
/// Cerrarlo a `127.0.0.1` sin salida rompería una topología legítima: nginx en
/// otra máquina, o un contenedor cuyo proxy vive fuera de su red de bucle. Ahí el
/// operador pone `SIGER_ESCUCHA=0.0.0.0` —o la dirección de la interfaz que
/// toque— y lo dice a propósito. Se cierra el descuido, no el caso.
pub fn direccion_de_escucha(
    valor: Option<&str>,
    tras_proxy: bool,
) -> Result<std::net::IpAddr, ErrorConfig> {
    match valor.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => v.parse().map_err(|_| ErrorConfig::NoEsDireccion(v.to_string())),
        None if tras_proxy => Ok(std::net::IpAddr::from([127, 0, 0, 1])),
        None => Ok(std::net::IpAddr::from([0, 0, 0, 0])),
    }
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
            escucha: std::net::IpAddr::from([127, 0, 0, 1]),
            cripto: std::sync::Arc::new(tokio::sync::Semaphore::new(
                permisos_de_cripto(None).expect("el defecto siempre es válido"),
            )),
        }
    }

    pub fn desde_entorno() -> Result<Self, ErrorConfig> {
        // Se lee antes que nada porque el defecto de `escucha` depende de él.
        let tras_proxy = booleano("SIGER_TRAS_PROXY", false);
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
            tras_proxy,
            escucha: direccion_de_escucha(opcional("SIGER_ESCUCHA").as_deref(), tras_proxy)?,
            cripto: std::sync::Arc::new(tokio::sync::Semaphore::new(
                permisos_de_cripto(opcional("SIGER_CRIPTO_PARALELA").as_deref())?,
            )),
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
            escucha: std::net::IpAddr::from([127, 0, 0, 1]),
            cripto: std::sync::Arc::new(tokio::sync::Semaphore::new(
                permisos_de_cripto(None).expect("el defecto siempre es válido"),
            )),
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use std::net::IpAddr;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn sin_proxy_declarado_se_escucha_en_todas_las_interfaces() {
        // El perfil de una sola máquina y el de un servidor sin proxy: si esto
        // ligara a loopback por defecto, nadie llegaría al sistema.
        assert_eq!(direccion_de_escucha(None, false).unwrap(), ip("0.0.0.0"));
    }

    #[test]
    fn declarar_el_proxy_deja_de_exponer_el_puerto() {
        // LA prueba de este cambio. Con el proxy declarado y el puerto abierto a
        // todas las interfaces, quien alcance el puerto se salta el proxy y elige
        // su propio `X-Real-IP`: el freno del login no engancharía nunca.
        assert_eq!(direccion_de_escucha(None, true).unwrap(), ip("127.0.0.1"));
    }

    #[test]
    fn el_operador_manda_sobre_el_defecto_en_las_dos_direcciones() {
        // nginx en otra máquina es una topología legítima, y cerrarla sin salida
        // sería rechazar lo inusual en vez de lo imposible.
        assert_eq!(direccion_de_escucha(Some("0.0.0.0"), true).unwrap(), ip("0.0.0.0"));
        assert_eq!(direccion_de_escucha(Some("10.0.0.7"), true).unwrap(), ip("10.0.0.7"));
        // Y al revés: se puede cerrar sin declarar proxy.
        assert_eq!(direccion_de_escucha(Some("127.0.0.1"), false).unwrap(), ip("127.0.0.1"));
        // IPv6 también, que es una dirección como cualquier otra.
        assert_eq!(direccion_de_escucha(Some("::1"), false).unwrap(), ip("::1"));
    }

    #[test]
    fn sin_valor_los_permisos_de_cripto_son_los_nucleos() {
        // El defecto conserva el consumo de memoria que el sistema ya tenía por
        // accidente (núcleos × 64 MiB) y solo le quita el bloqueo del runtime.
        let esperado = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        assert_eq!(permisos_de_cripto(None).unwrap(), esperado);
        assert!(esperado >= 1, "nunca cero: cero permisos es un servidor que no sirve");
    }

    #[test]
    fn el_operador_puede_subirlos_o_bajarlos_a_uno() {
        // Quien va justo de memoria lo baja a 1; quien tiene de sobra lo sube. Se
        // cierra el descuido, no el caso.
        assert_eq!(permisos_de_cripto(Some("1")).unwrap(), 1);
        assert_eq!(permisos_de_cripto(Some("16")).unwrap(), 16);
        assert_eq!(permisos_de_cripto(Some(" 4 ")).unwrap(), 4);
        // Vacío es «no lo puso»: cae al defecto.
        assert_eq!(
            permisos_de_cripto(Some("  ")).unwrap(),
            permisos_de_cripto(None).unwrap()
        );
    }

    #[test]
    fn un_tope_imposible_no_arranca_en_vez_de_caer_al_defecto() {
        // CERO es el caso que importa: un servidor con cero permisos no descifra
        // nada nunca, y caer al defecto dejaría al operador creyendo que acotó la
        // memoria mientras el servidor usa núcleos × 64 MiB.
        for malo in ["0", "-1", "muchos", "4.5", "1e3"] {
            assert!(
                matches!(
                    permisos_de_cripto(Some(malo)),
                    Err(ErrorConfig::NoEsPositivo(_, _))
                ),
                "«{malo}» pasó por tope válido"
            );
        }
    }

    #[test]
    fn una_direccion_ilegible_no_se_sustituye_por_un_defecto() {
        // Directiva: ante un dato roto, fallar. Caer al defecto dejaría al
        // operador creyendo que puso una cosa mientras el servidor hace otra —y
        // en la dirección peligrosa, porque el defecto sin proxy es 0.0.0.0.
        for malo in ["localhost", "127.0.0.1:8003", "no", " "] {
            let r = direccion_de_escucha(Some(malo), false);
            if malo.trim().is_empty() {
                // Vacío es «no la puso»: cae al defecto, que es correcto.
                assert!(r.is_ok(), "«{malo}» debería contar como ausente");
            } else {
                assert!(
                    matches!(r, Err(ErrorConfig::NoEsDireccion(_))),
                    "«{malo}» pasó por dirección válida"
                );
            }
        }
    }
}
