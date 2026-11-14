//! Alertas por correo de las entregas próximas y vencidas (#151.5).
//!
//! # Modo degradado, que es el punto de la directiva
//!
//! Un fallo del correo **no puede causar el incumplimiento que pretende evitar**.
//! Por eso: (1) el envío corre DESPUÉS de generar las entregas, nunca antes, así
//! que un servidor de correo caído no impide que las tareas existan; (2) una
//! entrega se marca como avisada SOLO si el envío tuvo éxito —si falla, se
//! reintenta en la siguiente pasada—; (3) un fallo de envío se registra y se
//! cuenta, pero no propaga: la pasada diaria termina igual.
//!
//! # Por qué se marca lo avisado
//!
//! Sin las columnas `alerta_proximidad_en` / `alerta_incumplimiento_en`, la misma
//! alerta saldría cada día, y una alerta diaria es una que nadie mira. Se marca
//! el instante del envío; la consulta excluye lo ya marcado.

use chrono::{NaiveDate, Utc};
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use sqlx::SqlitePool;

use crate::config::{ConfigCorreo, TlsCorreo};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Resumen {
    pub proximidad: usize,
    pub incumplimiento: usize,
    /// Envíos que fallaron: NO se marcaron, se reintentarán.
    pub fallidas: usize,
    /// Entregas que tocaba avisar pero sin nadie a quien: tampoco se marcan, para
    /// que se envíen cuando alguien configure el correo del destinatario.
    pub sin_destinatario: usize,
    /// **No se llegó a MIRAR ninguna entrega**: algo que la pasada necesitaba
    /// —la configuración del correo, o la propia cola en la base— no se pudo
    /// leer, y se abandonó antes de empezar. Es un caso aparte por
    /// la misma razón que `Documento::Ocupado`: sin él, un `SMTP_DESDE` mal
    /// escrito devuelve un resumen de ceros **idéntico** al de un día tranquilo
    /// en que no tocaba avisar a nadie, y las dos cosas no se pueden distinguir
    /// nunca más (directiva 30). Lleva el motivo porque quien lo arregla es una
    /// persona.
    pub abortada: Option<String>,
}

impl Resumen {
    /// **La regla, en una frase**: una pasada está incompleta si no llegó a
    /// intentarlo o si algún aviso CON destinatario no se pudo enviar.
    ///
    /// `sin_destinatario` **no** entra, y es deliberado: no es un fallo del
    /// sistema de alertas sino un dato que le falta al cliente —una persona sin
    /// correo en su ficha—, ya se cuenta por separado, y nada lo cierra. Meterlo
    /// aquí encendería una alarma que no se puede apagar, y a la tercera semana
    /// nadie la miraría (directiva 23).
    pub fn incompleta(&self) -> bool {
        self.abortada.is_some() || self.fallidas > 0
    }

    /// Lo que se escribe en la bitácora y en el log: los cuatro números, más el
    /// motivo si la pasada ni siquiera empezó.
    pub fn detalle(&self) -> String {
        match &self.abortada {
            Some(m) => format!("no se intentó: {m}"),
            None => format!(
                "proximidad={} incumplimiento={} fallidas={} sin_destinatario={}",
                self.proximidad, self.incumplimiento, self.fallidas, self.sin_destinatario
            ),
        }
    }
}

/// Qué clase de aviso, que decide el asunto y la columna que se marca.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Clase {
    Proximidad,
    Incumplimiento,
}

impl Clase {
    fn columna(self) -> &'static str {
        match self {
            Clase::Proximidad => "alerta_proximidad_en",
            Clase::Incumplimiento => "alerta_incumplimiento_en",
        }
    }
}

/// Una entrega que hay que avisar, con lo justo para redactar y para localizar a
/// quién avisar.
struct Pendiente {
    id: i64,
    informe_id: i64,
    usuario_id: Option<i64>,
    informe: String,
    fecha_limite: String,
    clase: Clase,
}

/// Construye el transporte SMTP según la configuración. `builder_dangerous` (sin
/// cifrar) SOLO se usa cuando se pidió `SMTP_TLS=ninguno`, para un buzón local de
/// pruebas; contra un servidor real se usa STARTTLS.
fn transporte(c: &ConfigCorreo) -> Result<AsyncSmtpTransport<Tokio1Executor>, String> {
    let base = match c.tls {
        TlsCorreo::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&c.host)
            .map_err(|e| e.to_string())?,
        TlsCorreo::Directo => {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&c.host).map_err(|e| e.to_string())?
        }
        TlsCorreo::Ninguno => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&c.host),
    };
    let mut base = base.port(c.puerto);
    if let (Some(u), Some(p)) = (&c.usuario, &c.clave) {
        base = base.credentials(Credentials::new(u.clone(), p.clone()));
    }
    Ok(base.build())
}

/// Envía un correo suelto a un destinatario. Lo usa el flujo del supervisor
/// (#163) para la «copia al correo» de una notificación interna. Devuelve si se
/// envió; nunca propaga —un correo caído no puede tumbar la operación—.
pub async fn enviar(c: &ConfigCorreo, destinatario: &str, asunto: &str, cuerpo: &str) -> bool {
    let (Ok(desde), Ok(para)) = (c.desde.parse::<Mailbox>(), destinatario.parse::<Mailbox>()) else {
        return false;
    };
    let t = match transporte(c) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let Ok(msg) = Message::builder().from(desde).to(para).subject(asunto).body(cuerpo.to_string())
    else {
        return false;
    };
    t.send(msg).await.is_ok()
}

/// Los correos a los que avisar de una entrega, según la política configurada.
/// Devuelve direcciones únicas y válidas; una dirección nula o vacía no cuenta.
async fn destinatarios(
    bd: &SqlitePool,
    c: &ConfigCorreo,
    p: &Pendiente,
    supervisores: &[String],
) -> Result<Vec<String>, String> {
    let mut correos: Vec<String> = Vec::new();

    if c.a_operador {
        if let Some(uid) = p.usuario_id {
            // **`if let Ok(…)` era la misma degradación silenciosa que se cerró
            // en `correr`, una función más abajo — y el grep que dio la lista por
            // completa buscaba `unwrap_or`, `.ok()` y `let _ =`, que no casan con
            // esto** (directiva 16: se midió lo fácil, no lo que se afirmaba).
            //
            // Y aquí el daño no era «una pasada de ceros» sino peor: con los tres
            // destinos activos —el defecto— un fallo de lectura aquí deja `bcc`
            // NO vacío gracias a supervisión, así que el aviso sale a menos gente
            // de la debida, se MARCA como enviado, y el marcado es la compuerta
            // anti-spam: al operador no se le avisa nunca.
            match sqlx::query_as::<_, (Option<String>,)>(
                "SELECT email FROM usuarios WHERE id = ? AND activo = 1",
            )
            .bind(uid)
            .fetch_optional(bd)
            .await
            {
                Ok(Some((Some(email),))) => correos.push(email),
                // La cuenta existe y no tiene correo, o no existe: eso SÍ es
                // «no hay a quién», y se distingue de no haber podido mirar.
                Ok(_) => {}
                Err(e) => return Err(format!("no se pudo leer el correo del operador — {e}")),
            }
        }
    }

    if c.a_supervision {
        correos.extend(supervisores.iter().cloned());
    }

    if c.a_tercero {
        // El correo del stakeholder destino de este informe.
        match sqlx::query_as::<_, (Option<String>,)>(
            "SELECT s.email FROM participacion p \
             JOIN stakeholders s ON s.id = p.stakeholder_id \
             WHERE p.informe_id = ? AND p.rol = 'DESTINO' AND s.email IS NOT NULL",
        )
        .bind(p.informe_id)
        .fetch_all(bd)
        .await
        {
            Ok(filas) => correos.extend(filas.into_iter().flat_map(|(e,)| e)),
            Err(e) => return Err(format!("no se pudo leer el correo del destino — {e}")),
        }
    }

    // Únicas, no vacías. Enviar dos veces al mismo correo por ser operador y
    // supervisor a la vez sería ruido.
    correos.retain(|e| !e.trim().is_empty());
    correos.sort();
    correos.dedup();
    Ok(correos)
}

fn redactar(desde: &Mailbox, bcc: &[String], p: &Pendiente) -> Result<Message, String> {
    let (asunto, encabezado) = match p.clase {
        Clase::Proximidad => (
            format!("Recordatorio: «{}» vence el {}", p.informe, p.fecha_limite),
            "Esta entrega está próxima a vencer.",
        ),
        Clase::Incumplimiento => (
            format!("Vencido: «{}» venció el {}", p.informe, p.fecha_limite),
            "Esta entrega venció y no consta registrada.",
        ),
    };
    let cuerpo = format!(
        "{encabezado}\n\n  Informe: {}\n  Fecha límite: {}\n\n\
         Este es un aviso automático de SIGER.",
        p.informe, p.fecha_limite
    );

    let mut msg = Message::builder()
        .from(desde.clone())
        // Los destinatarios reales van en copia oculta: un aviso no tiene por qué
        // revelarle a un tercero externo las demás direcciones. El `To` es el
        // propio remitente.
        .to(desde.clone())
        .subject(asunto);
    for correo in bcc {
        match correo.parse::<Mailbox>() {
            Ok(m) => msg = msg.bcc(m),
            Err(_) => return Err(format!("dirección inválida: {correo}")),
        }
    }
    msg.body(cuerpo).map_err(|e| e.to_string())
}

/// Ejecuta una pasada de alertas. Se llama tras generar las entregas del día.
pub async fn correr(bd: &SqlitePool, c: &ConfigCorreo, hoy: NaiveDate) -> Resumen {
    let mut r = Resumen::default();

    let desde: Mailbox = match c.desde.parse() {
        Ok(m) => m,
        Err(_) => {
            r.abortada = Some(format!("SMTP_DESDE «{}» no es una dirección válida", c.desde));
            return cerrar(bd, r).await;
        }
    };
    let transporte = match transporte(c) {
        Ok(t) => t,
        Err(e) => {
            r.abortada = Some(format!("no se pudo preparar el transporte SMTP — {e}"));
            return cerrar(bd, r).await;
        }
    };

    // Los supervisores son los mismos para todas las entregas: se traen una vez.
    let supervisores: Vec<String> = if c.a_supervision {
        match sqlx::query_as::<_, (String,)>(
            "SELECT email FROM usuarios WHERE rol = 'SUPERVISOR' AND activo = 1 AND email <> ''",
        )
        .fetch_all(bd)
        .await
        {
            Ok(v) => v.into_iter().map(|(e,)| e).collect(),
            // Degradaba a lista vacía, y ése era el peor de los tres: la política
            // dice avisar a supervisión, así que un fallo de lectura habría
            // mandado los avisos a MENOS gente de la debida **y los habría
            // marcado como enviados** — y el marcado es la compuerta anti-spam,
            // o sea que ese supervisor no se enteraría nunca. Un día de retraso
            // se recupera; un aviso marcado sin enviar, no.
            Err(e) => {
                r.abortada = Some(format!("no se pudo leer la lista de supervisión — {e}"));
                return cerrar(bd, r).await;
            }
        }
    } else {
        Vec::new()
    };

    let hoy_s = hoy.to_string();
    let tope = (hoy + chrono::Duration::days(c.dias_proximidad)).to_string();

    // Próximas: pendientes cuyo límite cae entre hoy y hoy+N, aún sin avisar.
    let proximas = sqlx::query_as::<_, (i64, i64, Option<i64>, String, String)>(
        "SELECT e.id, e.informe_id, e.usuario_id, i.nombre, e.fecha_limite \
         FROM entregas e JOIN informes i ON i.id = e.informe_id \
         WHERE e.estado = 'PENDIENTE' AND e.alerta_proximidad_en IS NULL \
           AND e.fecha_limite >= ? AND e.fecha_limite <= ? \
         ORDER BY e.fecha_limite",
    )
    .bind(&hoy_s)
    .bind(&tope)
    .fetch_all(bd)
    .await;

    // Vencidas: pendientes cuyo límite ya pasó, aún sin avisar del incumplimiento.
    let vencidas = sqlx::query_as::<_, (i64, i64, Option<i64>, String, String)>(
        "SELECT e.id, e.informe_id, e.usuario_id, i.nombre, e.fecha_limite \
         FROM entregas e JOIN informes i ON i.id = e.informe_id \
         WHERE e.estado = 'PENDIENTE' AND e.alerta_incumplimiento_en IS NULL \
           AND e.fecha_limite < ? \
         ORDER BY e.fecha_limite",
    )
    .bind(&hoy_s)
    .fetch_all(bd)
    .await;

    // **Las dos degradaban a lista vacía**, y con eso `correr` devolvía un
    // `Resumen` de ceros que la bitácora escribía como `alertas_pasada
    // proximidad=0 … fallidas=0`: el mismo renglón, carácter por carácter, que un
    // día tranquilo en que no tocaba avisar a nadie. Es exactamente el argumento
    // que `abortada` trae escrito, aplicado a la otra fuente — se cerró el camino
    // del SMTP y se dejó abierto el de la base. Lo encontró la revisión de
    // seguridad de este mismo cambio.
    let (proximas, vencidas) = match (proximas, vencidas) {
        (Ok(p), Ok(v)) => (p, v),
        (Err(e), _) | (_, Err(e)) => {
            r.abortada = Some(format!("no se pudo leer la cola de avisos — {e}"));
            return cerrar(bd, r).await;
        }
    };

    let pendientes = proximas
        .into_iter()
        .map(|(id, informe_id, usuario_id, informe, fecha_limite)| Pendiente {
            id, informe_id, usuario_id, informe, fecha_limite, clase: Clase::Proximidad,
        })
        .chain(vencidas.into_iter().map(
            |(id, informe_id, usuario_id, informe, fecha_limite)| Pendiente {
                id, informe_id, usuario_id, informe, fecha_limite, clase: Clase::Incumplimiento,
            },
        ));

    for p in pendientes {
        // No aborta la pasada: el fallo es de UNA entrega y suele ser transitorio
        // (`SQLITE_BUSY`), así que tumbar el resto castigaría a todos. Cuenta como
        // fallida —que es lo que es: no se pudo avisar— y NO se marca, así que el
        // siguiente pase lo reintenta. La regla sigue siendo una frase.
        let bcc = match destinatarios(bd, c, &p, &supervisores).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("alertas: entrega {} — {e}", p.id);
                r.fallidas += 1;
                continue;
            }
        };
        if bcc.is_empty() {
            r.sin_destinatario += 1;
            continue;
        }
        let mensaje = match redactar(&desde, &bcc, &p) {
            Ok(m) => m,
            Err(e) => {
                tracing::error!("alertas: no se pudo redactar el aviso de la entrega {} — {e}", p.id);
                r.fallidas += 1;
                continue;
            }
        };
        // El envío puede fallar; si lo hace, NO se marca —se reintentará—.
        match transporte.send(mensaje).await {
            Ok(_) => {
                let sql = format!("UPDATE entregas SET {} = ? WHERE id = ?", p.clase.columna());
                if let Err(e) = sqlx::query(&sql).bind(Utc::now().to_rfc3339()).bind(p.id).execute(bd).await {
                    // Se envió pero no se pudo marcar: mejor un duplicado mañana
                    // que perder el aviso. Se registra.
                    tracing::error!("alertas: enviada la entrega {} pero no se marcó — {e}", p.id);
                }
                match p.clase {
                    Clase::Proximidad => r.proximidad += 1,
                    Clase::Incumplimiento => r.incumplimiento += 1,
                }
            }
            Err(e) => {
                tracing::error!("alertas: falló el envío de la entrega {} — {e}", p.id);
                r.fallidas += 1;
            }
        }
    }

    cerrar(bd, r).await
}

/// **El único sitio por el que sale una pasada de alertas**, y por eso deja
/// rastro donde se puede consultar después.
///
/// Hasta hoy el resumen se emitía por `tracing` y nada más: un `info!` con
/// «120 fallidas» dentro del diario del proceso, que en el VPS nadie mira hasta
/// que un cliente llama para decir que no le avisaron. El log del proceso es
/// para depurar; la bitácora es la que el producto ya ofrece como evidencia, y
/// es donde tiene que verse que el aviso que SIGER vende no salió.
///
/// Se escribe **en cada pasada**, también cuando todo fue bien, y eso es a
/// propósito (directiva 30): sin la línea buena, una tabla sin eventos de
/// alertas significa a la vez «no hacía falta avisar a nadie» y «el reloj lleva
/// tres semanas muerto». Es una fila al día — no tiene nada que ver con lo que
/// se decidió NO auditar, que era lo que se escribe en cada pintado de pantalla.
async fn cerrar(bd: &SqlitePool, r: Resumen) -> Resumen {
    let detalle = r.detalle();
    if r.incompleta() {
        tracing::error!("alertas: pasada INCOMPLETA — {detalle}");
        crate::rutas::auditar(bd, "alertas_incompletas", None, Some(&detalle)).await;
    } else {
        tracing::info!("alertas: {detalle}");
        crate::rutas::auditar(bd, "alertas_pasada", None, Some(&detalle)).await;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(a_op: bool, a_sup: bool, a_ter: bool) -> ConfigCorreo {
        ConfigCorreo {
            host: "localhost".into(),
            puerto: 2525,
            usuario: None,
            clave: None,
            desde: "SIGER <siger@local>".into(),
            tls: TlsCorreo::Ninguno,
            dias_proximidad: 3,
            a_operador: a_op,
            a_supervision: a_sup,
            a_tercero: a_ter,
        }
    }

    async fn base() -> SqlitePool {
        let bd = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&bd).await.unwrap();
        bd
    }

    #[tokio::test]
    async fn los_destinatarios_salen_de_la_politica_y_van_sin_repetir() {
        let bd = base().await;
        // Un usuario que es a la vez operador de la entrega. Y un supervisor.
        sqlx::query("INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) VALUES (1,'Op','op@x.co','h','OPERADOR',1),(2,'Sup','sup@x.co','h','SUPERVISOR',1)")
            .execute(&bd).await.unwrap();
        sqlx::query("INSERT INTO stakeholders (id,nombre,externo,email) VALUES (9,'Ente',1,'ente@x.co')").execute(&bd).await.unwrap();
        sqlx::query("INSERT INTO informes (id,nombre,recurrencia) VALUES (5,'Informe','{\"modo\":\"INMEDIATO\"}')").execute(&bd).await.unwrap();
        sqlx::query("INSERT INTO participacion (informe_id,stakeholder_id,rol) VALUES (5,9,'DESTINO')").execute(&bd).await.unwrap();
        let p = Pendiente { id: 1, informe_id: 5, usuario_id: Some(1), informe: "Informe".into(), fecha_limite: "2026-01-05".into(), clase: Clase::Proximidad };

        let sup = vec!["sup@x.co".to_string()];
        // Los tres activos → los tres correos.
        let d = destinatarios(&bd, &cfg(true, true, true), &p, &sup).await;
        assert_eq!(d.unwrap(), vec!["ente@x.co", "op@x.co", "sup@x.co"]);
        // Solo operador.
        assert_eq!(destinatarios(&bd, &cfg(true, false, false), &p, &sup).await.unwrap(), vec!["op@x.co"]);
        // Solo tercero.
        assert_eq!(destinatarios(&bd, &cfg(false, false, true), &p, &sup).await.unwrap(), vec!["ente@x.co"]);
        // Ninguno con correo → vacío (y correr() lo contaría como sin_destinatario).
        let sin = Pendiente { usuario_id: None, ..p };
        assert!(destinatarios(&bd, &cfg(true, false, false), &sin, &[]).await.unwrap().is_empty());
    }

    #[test]
    fn el_asunto_distingue_proximo_de_vencido() {
        let desde: Mailbox = "s@local".parse().unwrap();
        let p = Pendiente { id: 1, informe_id: 1, usuario_id: None, informe: "Anual".into(), fecha_limite: "2026-01-05".into(), clase: Clase::Proximidad };
        assert!(redactar(&desde, &["a@b.co".into()], &p).is_ok());
        // Una dirección inválida en el bcc es un error, no un envío a medias.
        assert!(redactar(&desde, &["no-es-correo".into()], &p).is_err());
    }
}

/// Verificación del MODO DEGRADADO (#71): un fallo del correo **no puede causar
/// el incumplimiento que pretende evitar**, y el mecanismo DISCRIMINA — cuando el
/// envío funciona marca (y no reenvía); cuando falla no marca (y reintenta).
///
/// El SMTP de prueba es un hilo `std::net` que habla el subconjunto mínimo del
/// protocolo; el «caído» es un puerto sin nadie escuchando. Cada pasada va
/// envuelta en un timeout: un envío colgado debe fallar RUIDOSAMENTE, no parecer
/// una compilación lenta (misma lección que el deadlock de 13 minutos del
/// autotest de Quipu).
#[cfg(test)]
mod degradado {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Un mensaje tal y como el sink lo recibió: a quién iba por SOBRE (`RCPT
    /// TO`) y el texto completo del `DATA`.
    ///
    /// **El sink guardaba solo un contador**, y con eso el banco no distinguía
    /// «avisó a las 120 personas correctas» de «mandó 120 correos vacíos a la
    /// dirección equivocada»: los dos casos son 120 bloques DATA. Un contador
    /// mide que el protocolo se habló, no que el aviso sirva.
    ///
    /// Se mira el SOBRE y no la cabecera `To:` porque `redactar` pone a los
    /// destinatarios en **BCC** —un aviso a tres personas no revela quiénes son
    /// las otras dos—, así que en el texto no aparecen: la única constancia de a
    /// quién se envió está en el `RCPT TO`.
    #[derive(Clone)]
    struct Recibido {
        para: Vec<String>,
        texto: String,
    }

    impl Recibido {
        /// El asunto, ya decodificado si venía en MIME encoded-word (`=?utf-8?..`),
        /// que es como viaja en cuanto lleva una tilde.
        fn asunto(&self) -> String {
            let crudo = self
                .texto
                .lines()
                .find_map(|l| l.strip_prefix("Subject: "))
                .unwrap_or_default()
                .trim()
                .to_string();
            descodificar(&crudo)
        }

        fn cuerpo(&self) -> String {
            // El cuerpo empieza tras la línea en blanco que cierra las cabeceras.
            let bruto = match self.texto.split_once("\r\n\r\n") {
                Some((_, c)) => c.to_string(),
                None => self.texto.split_once("\n\n").map(|(_, c)| c.to_string()).unwrap_or_default(),
            };
            // lettre manda el cuerpo en quoted-printable si lleva acentos.
            descomillar(&bruto)
        }
    }

    /// `=?utf-8?B?..?=` y `=?utf-8?Q?..?=` → texto. Lo justo para leer un asunto.
    fn descodificar(s: &str) -> String {
        let mut fuera = String::new();
        let mut resto = s;
        while let Some(i) = resto.find("=?") {
            fuera.push_str(&resto[..i]);
            let Some(fin) = resto[i..].find("?=") else { break };
            let trozo = &resto[i + 2..i + fin];
            let partes: Vec<&str> = trozo.splitn(3, '?').collect();
            if partes.len() == 3 {
                let texto = match partes[1].to_uppercase().as_str() {
                    "B" => String::from_utf8_lossy(
                        &base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            partes[2],
                        )
                        .unwrap_or_default(),
                    )
                    .into_owned(),
                    _ => descomillar(&partes[2].replace('_', " ")),
                };
                fuera.push_str(&texto);
            }
            resto = &resto[i + fin + 2..];
        }
        fuera.push_str(resto);
        if fuera.trim().is_empty() { s.to_string() } else { fuera }
    }

    /// quoted-printable → texto, con lo justo: `=XX` y el corte de línea blando.
    fn descomillar(s: &str) -> String {
        let s = s.replace("=\r\n", "").replace("=\n", "");
        let b = s.as_bytes();
        let mut fuera: Vec<u8> = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'=' && i + 2 < b.len() {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    fuera.push(v);
                    i += 3;
                    continue;
                }
            }
            fuera.push(b[i]);
            i += 1;
        }
        String::from_utf8_lossy(&fuera).into_owned()
    }

    /// Sink SMTP que acepta todo y **guarda** los mensajes con DATA completa.
    fn sink_ok() -> (u16, Arc<Mutex<Vec<Recibido>>>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let puerto = l.local_addr().unwrap().port();
        let buzon = Arc::new(Mutex::new(Vec::new()));
        let b2 = buzon.clone();
        std::thread::spawn(move || {
            for mut s in l.incoming().flatten() {
                let _ = atiende(&mut s, &b2);
            }
        });
        (puerto, buzon)
    }

    fn atiende(s: &mut std::net::TcpStream, buzon: &Mutex<Vec<Recibido>>) -> std::io::Result<()> {
        let mut w = s.try_clone()?;
        let mut r = BufReader::new(s);
        w.write_all(b"220 sink\r\n")?;
        w.flush()?;
        let mut linea = String::new();
        let mut para: Vec<String> = Vec::new();
        loop {
            linea.clear();
            if r.read_line(&mut linea)? == 0 {
                break;
            }
            let up = linea.trim_end().to_uppercase();
            if up.starts_with("EHLO") || up.starts_with("HELO") {
                w.write_all(b"250-sink\r\n250 OK\r\n")?;
            } else if up.starts_with("RCPT TO:") {
                // `RCPT TO:<alguien@x.co>` → `alguien@x.co`
                let d = linea.trim_end();
                let d = d[8..].trim().trim_start_matches('<').trim_end_matches('>');
                para.push(d.to_string());
                w.write_all(b"250 OK\r\n")?;
            } else if up.starts_with("DATA") {
                w.write_all(b"354 fin con .\r\n")?;
                w.flush()?;
                let mut texto = String::new();
                let mut cuerpo = String::new();
                loop {
                    cuerpo.clear();
                    if r.read_line(&mut cuerpo)? == 0 || cuerpo.trim_end() == "." {
                        break;
                    }
                    texto.push_str(&cuerpo);
                }
                buzon.lock().unwrap().push(Recibido { para: std::mem::take(&mut para), texto });
                w.write_all(b"250 OK\r\n")?;
            } else if up.starts_with("QUIT") {
                w.write_all(b"221 adios\r\n")?;
                w.flush()?;
                break;
            } else {
                // MAIL, RSET, NOOP y cualquier otra: se aceptan.
                w.write_all(b"250 OK\r\n")?;
            }
            w.flush()?;
        }
        Ok(())
    }

    fn cuantos(b: &Arc<Mutex<Vec<Recibido>>>) -> usize {
        b.lock().unwrap().len()
    }

    /// Un puerto con nadie escuchando: la conexión será rechazada.
    fn puerto_muerto() -> u16 {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let p = l.local_addr().unwrap().port();
        drop(l);
        p
    }

    fn cfg_en(host: &str, puerto: u16) -> ConfigCorreo {
        ConfigCorreo {
            host: host.into(),
            puerto,
            usuario: None,
            clave: None,
            desde: "SIGER <siger@local>".into(),
            tls: TlsCorreo::Ninguno,
            dias_proximidad: 3,
            a_operador: true,
            a_supervision: false,
            a_tercero: false,
        }
    }

    async fn base() -> SqlitePool {
        let bd = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&bd).await.unwrap();
        bd
    }

    /// Siembra `n` entregas PENDIENTE próximas a vencer, con un operador con correo.
    async fn siembra(bd: &SqlitePool, n: i64) {
        sqlx::query("INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) VALUES (1,'Op','op@x.co','h','OPERADOR',1)").execute(bd).await.unwrap();
        sqlx::query("INSERT INTO informes (id,nombre,recurrencia) VALUES (1,'Inf','{\"modo\":\"INMEDIATO\"}')").execute(bd).await.unwrap();
        for k in 0..n {
            sqlx::query("INSERT INTO entregas (informe_id,usuario_id,fecha_nominal,fecha_limite,estado) VALUES (1,1,?,'2026-06-01','PENDIENTE')")
                .bind(format!("n{k}"))
                .execute(bd)
                .await
                .unwrap();
        }
    }

    async fn marcadas(bd: &SqlitePool) -> i64 {
        sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM entregas WHERE alerta_proximidad_en IS NOT NULL")
            .fetch_one(bd)
            .await
            .unwrap()
            .0
    }

    async fn pendientes(bd: &SqlitePool) -> i64 {
        sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM entregas WHERE estado = 'PENDIENTE'")
            .fetch_one(bd)
            .await
            .unwrap()
            .0
    }

    async fn con_timeout(bd: &SqlitePool, c: &ConfigCorreo, hoy: NaiveDate) -> Resumen {
        tokio::time::timeout(Duration::from_secs(30), correr(bd, c, hoy))
            .await
            .expect("una pasada de alertas no debe colgarse")
    }

    fn hoy() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()
    }

    /// Camino feliz + 100+ operaciones: 120 entregas → 120 avisos, todas
    /// marcadas; una segunda pasada NO reenvía (el marcado es la compuerta
    /// anti-spam).
    #[tokio::test]
    async fn envio_ok_marca_y_no_reenvia() {
        let bd = base().await;
        siembra(&bd, 120).await;
        let (puerto, buzon) = sink_ok();
        let c = cfg_en("127.0.0.1", puerto);

        let r = con_timeout(&bd, &c, hoy()).await;
        assert_eq!(r.proximidad, 120, "deben avisarse las 120");
        assert_eq!(r.fallidas, 0);
        assert!(!r.incompleta(), "una pasada entera no está incompleta");
        assert_eq!(marcadas(&bd).await, 120, "las 120 quedan marcadas");
        assert_eq!(cuantos(&buzon), 120, "el sink recibió 120");

        // Y lo que el sink recibió SIRVE: va a quien toca, con asunto y con el
        // informe dentro. Sin esto, 120 correos vacíos a la dirección
        // equivocada contarían igual.
        let msgs = buzon.lock().unwrap().clone();
        for m in &msgs {
            // El sobre es EXACTO, no «contiene»: así caza las dos direcciones del
            // fallo —que el aviso vaya a quien no era, y que se le filtre a un
            // tercero de más—. `siger@local` está ahí legítimamente porque
            // `redactar` pone el `To` al propio remitente y a los destinatarios
            // en copia oculta, para no revelarle a un ente externo las demás
            // direcciones; el operador viaja en ese BCC y por eso solo consta en
            // el sobre.
            assert_eq!(m.para, vec!["siger@local", "op@x.co"], "el sobre exacto del aviso");
            assert!(m.asunto().contains("Inf"), "el asunto nombra el informe: {:?}", m.asunto());
            assert!(m.asunto().contains("2026-06-01"), "y su fecha: {:?}", m.asunto());
            assert!(m.cuerpo().contains("Informe: Inf"), "el cuerpo: {:?}", m.cuerpo());
            assert!(m.cuerpo().contains("2026-06-01"), "y su fecha límite: {:?}", m.cuerpo());
        }

        let r2 = con_timeout(&bd, &c, hoy()).await;
        assert_eq!(r2.proximidad, 0, "no se reenvía lo ya avisado");
        assert_eq!(cuantos(&buzon), 120, "el sink no recibió más");
    }

    /// La INYECCIÓN del fallo: SMTP caído. Ninguna entrega se marca, ninguna se
    /// pierde, la pasada termina; y al volver el correo, el aviso se reintenta y
    /// se entrega. La caída NO causó el incumplimiento.
    #[tokio::test]
    async fn smtp_caido_no_marca_ni_pierde_y_reintenta() {
        let bd = base().await;
        siembra(&bd, 120).await;

        let r = con_timeout(&bd, &cfg_en("127.0.0.1", puerto_muerto()), hoy()).await;
        assert_eq!(r.proximidad, 0, "no se avisa nada con el correo caído");
        assert_eq!(r.fallidas, 120, "las 120 se cuentan como fallidas");
        assert!(r.incompleta(), "120 envíos fallidos son una pasada incompleta");
        assert_eq!(marcadas(&bd).await, 0, "NINGUNA se marca: se reintentarán");
        assert_eq!(pendientes(&bd).await, 120, "las entregas siguen ahí: la caída no las borró");

        let (puerto, buzon) = sink_ok();
        let r2 = con_timeout(&bd, &cfg_en("127.0.0.1", puerto), hoy()).await;
        assert_eq!(r2.proximidad, 120, "al volver el correo se avisan las 120 que esperaban");
        assert!(!r2.incompleta(), "y al volver, la pasada ya está entera");
        assert_eq!(marcadas(&bd).await, 120);
        assert_eq!(cuantos(&buzon), 120);
    }

    async fn eventos(bd: &SqlitePool) -> Vec<(String, String)> {
        sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT evento, detalle FROM auditoria WHERE evento LIKE 'alertas_%' ORDER BY id",
        )
        .fetch_all(bd)
        .await
        .unwrap()
        .into_iter()
        .map(|(e, d)| (e, d.unwrap_or_default()))
        .collect()
    }

    /// **El caso que no tenía forma de verse, y era el peor de los tres.**
    ///
    /// Con `SMTP_DESDE` ilegible, `correr` abandonaba antes de mirar una sola
    /// entrega y devolvía un resumen de CEROS —cero avisos, cero fallidas—,
    /// exactamente igual que un día tranquilo en que no tocaba avisar a nadie.
    /// Un negativo que se produce sin recorrer el camino no vale (directiva 30):
    /// aquí ni siquiera había camino.
    ///
    /// La pareja está dentro: la misma base y las mismas 5 entregas con el
    /// remitente BUENO recorren y avisan. Si el control no avisara, el rojo
    /// podría venir de que no hubiera nada que enviar.
    #[tokio::test]
    async fn el_remitente_ilegible_no_se_ve_igual_que_un_dia_sin_avisos() {
        let bd = base().await;
        siembra(&bd, 5).await;
        let (puerto, buzon) = sink_ok();

        let mut roto = cfg_en("127.0.0.1", puerto);
        roto.desde = "esto no es una dirección".into();
        let r = con_timeout(&bd, &roto, hoy()).await;

        assert_eq!(r.proximidad, 0);
        assert_eq!(r.fallidas, 0, "no falló ningún envío: no se intentó ninguno");
        assert!(r.abortada.is_some(), "la pasada tiene que DECIR que no se intentó");
        assert!(r.incompleta(), "y por tanto está incompleta");
        assert_eq!(cuantos(&buzon), 0, "nada salió");
        assert_eq!(marcadas(&bd).await, 0, "y nada se marcó: se reintentará");

        let ev = eventos(&bd).await;
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].0, "alertas_incompletas");
        assert!(ev[0].1.contains("no se intentó"), "la bitácora dice qué pasó: {:?}", ev[0].1);

        // EL CONTROL: mismo día, misma base, remitente bueno → sí avisa.
        let r2 = con_timeout(&bd, &cfg_en("127.0.0.1", puerto), hoy()).await;
        assert_eq!(r2.proximidad, 5, "con el remitente bueno sí había cinco que avisar");
        assert!(!r2.incompleta());
        assert_eq!(cuantos(&buzon), 5);
    }

    /// **La misma trampa, en la otra fuente** — la encontró la revisión de
    /// seguridad de este mismo cambio, y es la lección que más se repite: se
    /// cierra un agujero por un lado y se deja abierto el gemelo.
    ///
    /// Las tres lecturas de base degradaban a lista vacía con `unwrap_or_default`.
    /// Con la cola ilegible, `correr` devolvía ceros y la bitácora escribía
    /// `alertas_pasada … fallidas=0`: el mismo renglón, carácter por carácter,
    /// que un día en que no había a quién avisar. La pareja está dentro y es lo
    /// que le da valor: la MISMA base sembrada, con la tabla en pie, sí avisa.
    #[tokio::test]
    async fn la_cola_ilegible_no_se_ve_igual_que_un_dia_sin_avisos() {
        let bd = base().await;
        siembra(&bd, 4).await;
        let (puerto, buzon) = sink_ok();

        // EL CONTROL PRIMERO, para que el rojo de después no pueda venir de que
        // no hubiera nada que enviar.
        let ok = con_timeout(&bd, &cfg_en("127.0.0.1", puerto), hoy()).await;
        assert_eq!(ok.proximidad, 4, "con la tabla en pie hay cuatro que avisar");
        assert_eq!(cuantos(&buzon), 4);

        // Y ahora se rompe la lectura por la vía real: la consulta de la cola
        // falla porque la tabla no está.
        sqlx::query("DROP TABLE entregas").execute(&bd).await.unwrap();
        let r = con_timeout(&bd, &cfg_en("127.0.0.1", puerto), hoy()).await;

        assert_eq!(r.proximidad, 0);
        assert_eq!(r.fallidas, 0, "no falló ningún envío: no se intentó ninguno");
        assert!(r.abortada.is_some(), "la pasada tiene que DECIR que no pudo leer");
        assert!(r.incompleta(), "y por tanto está incompleta");
        assert_eq!(cuantos(&buzon), 4, "no salió nada nuevo");

        let ev = eventos(&bd).await;
        assert_eq!(ev.len(), 2, "una línea por pasada");
        assert_eq!(ev[0].0, "alertas_pasada", "la buena");
        assert_eq!(ev[1].0, "alertas_incompletas", "y la que no pudo leer");
        assert!(ev[1].1.contains("no se pudo leer la cola"), "y dice qué: {:?}", ev[1].1);
    }

    /// La lista de supervisión ilegible aborta igual, y **este era el peor de los
    /// tres**: degradaba a lista vacía, así que los avisos habrían salido a menos
    /// gente de la que manda la política **y se habrían marcado como enviados**.
    /// El marcado es la compuerta anti-spam: ese supervisor no se enteraría nunca.
    #[tokio::test]
    async fn la_supervision_ilegible_no_manda_el_aviso_a_menos_gente_y_lo_da_por_hecho() {
        let bd = base().await;
        siembra(&bd, 3).await;
        sqlx::query("INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) VALUES (2,'Sup','sup@x.co','h','SUPERVISOR',1)")
            .execute(&bd).await.unwrap();
        let (puerto, buzon) = sink_ok();
        let mut c = cfg_en("127.0.0.1", puerto);
        c.a_supervision = true;

        // Se rompe la lectura por la vía real: la consulta pide `email` y la
        // columna deja de llamarse así. (`DROP TABLE` lo impide la clave
        // foránea de `entregas` —que es lo que esta pasada necesita en pie—, y
        // `DROP COLUMN` lo impide el UNIQUE del correo.)
        sqlx::query("ALTER TABLE usuarios RENAME COLUMN email TO correo_x").execute(&bd).await.unwrap();
        let r = con_timeout(&bd, &c, hoy()).await;

        assert!(r.abortada.is_some(), "sin poder leer supervisión no se avisa a ciegas");
        assert!(r.incompleta());
        assert_eq!(cuantos(&buzon), 0, "no salió ni uno");
        assert_eq!(marcadas(&bd).await, 0, "y NADA quedó marcado: se reintenta entero");
        let ev = eventos(&bd).await;
        assert!(ev[0].1.contains("supervisión"), "la bitácora dice qué falló: {:?}", ev[0].1);
    }

    /// **La mitad que faltaba, y la señaló un mutante que sobrevivía a las 251
    /// pruebas**: el brazo `Ok(_)` de la lectura del operador —la cuenta no
    /// existe, o está desactivada— tiene que seguir contando como
    /// `sin_destinatario` y NO como fallo. Es la distinción entera de este
    /// arreglo: «no hay a quién» frente a «no pude mirar».
    ///
    /// Ninguna prueba la ejercía. Las que parecían hacerlo usan `usuario_id:
    /// None`, que ni siquiera llega al `SELECT`, así que la distinción estaba
    /// afirmada en prosa y no medida.
    #[tokio::test]
    async fn una_cuenta_sin_correo_es_sin_destinatario_y_no_un_fallo() {
        let bd = base().await;
        // Un operador REAL, con su fila, DESACTIVADO. Es el caso alcanzable de
        // verdad —alguien que se va de la organización y deja entregas
        // pendientes a su nombre—, y llega al SELECT devolviendo `Ok(None)`,
        // porque la consulta filtra por `activo = 1`.
        //
        // (El primer intento ponía el correo a NULL, y no compila contra la
        // realidad: `usuarios.email` es NOT NULL, así que `Ok(Some((None,)))` es
        // imposible por esquema y el `Option` de la consulta es defensivo. Lo
        // dijo la base, no el código.)
        sqlx::query("INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) VALUES (1,'Op','op@x.co','h','OPERADOR',0)")
            .execute(&bd).await.unwrap();
        sqlx::query("INSERT INTO informes (id,nombre,recurrencia) VALUES (1,'Inf','{\"modo\":\"INMEDIATO\"}')")
            .execute(&bd).await.unwrap();
        sqlx::query("INSERT INTO entregas (informe_id,usuario_id,fecha_nominal,fecha_limite,estado) VALUES (1,1,'n1','2026-06-01','PENDIENTE')")
            .execute(&bd).await.unwrap();

        let (puerto, buzon) = sink_ok();
        let r = con_timeout(&bd, &cfg_en("127.0.0.1", puerto), hoy()).await;

        assert_eq!(r.sin_destinatario, 1, "una cuenta desactivada es «no hay a quién»");
        assert_eq!(r.fallidas, 0, "y NO un fallo: la lectura funcionó perfectamente");
        assert!(!r.incompleta(), "así que la pasada no está incompleta");
        assert_eq!(cuantos(&buzon), 0);
        assert_eq!(marcadas(&bd).await, 0, "no se marca: saldrá cuando alguien la reactive");

        // Y la pareja: la MISMA cuenta, reactivada, sí recibe.
        sqlx::query("UPDATE usuarios SET activo = 1 WHERE id = 1").execute(&bd).await.unwrap();
        let r2 = con_timeout(&bd, &cfg_en("127.0.0.1", puerto), hoy()).await;
        assert_eq!(r2.proximidad, 1, "reactivada, sí se avisa");
        assert_eq!(r2.sin_destinatario, 0);
        assert_eq!(cuantos(&buzon), 1);
    }

    /// **El caso que la revisión de seguridad describió, y el peor de la
    /// familia**: no es «una pasada de ceros» sino un aviso que SALE a menos
    /// gente de la debida y se MARCA como enviado.
    ///
    /// Se rompe la lectura del TERCERO y solo ésa. `usuarios` queda intacta, así
    /// que el operador y el supervisor se leen bien y `bcc` NO queda vacío: con
    /// la regla vieja el aviso salía a esos dos, se ejecutaba el `UPDATE` que es
    /// la compuerta anti-spam, y al ente destinatario no se le avisaba jamás —con
    /// `fallidas` en 0 y la bitácora escribiendo el renglón de una pasada sana.
    ///
    /// **Y la vía importa**: el primer intento de esta prueba renombraba la
    /// columna de `usuarios`, que rompe TAMBIÉN la consulta de supervisión —la
    /// que corre antes y aborta—, así que el rojo venía por el camino de la
    /// prueba de al lado y el mutante pasaba en verde. Se vio mutando, no
    /// leyendo.
    #[tokio::test]
    async fn si_no_se_puede_leer_a_quien_avisar_no_se_avisa_a_medias_ni_se_da_por_hecho() {
        async fn escenario(bd: &SqlitePool) {
            siembra(bd, 1).await;
            sqlx::query("INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) VALUES (2,'Sup','sup@x.co','h','SUPERVISOR',1)")
                .execute(bd).await.unwrap();
            sqlx::query("INSERT INTO stakeholders (id,nombre,externo,email) VALUES (9,'Ente',1,'ente@x.co')")
                .execute(bd).await.unwrap();
            sqlx::query("INSERT INTO participacion (informe_id,stakeholder_id,rol) VALUES (1,9,'DESTINO')")
                .execute(bd).await.unwrap();
        }

        let (puerto, buzon) = sink_ok();
        let mut c = cfg_en("127.0.0.1", puerto);
        c.a_supervision = true;
        c.a_tercero = true;

        // EL CONTROL: con todo en pie, el aviso va a los TRES.
        let bd = base().await;
        escenario(&bd).await;
        let ok = con_timeout(&bd, &c, hoy()).await;
        assert_eq!(ok.proximidad, 1);
        assert_eq!(
            buzon.lock().unwrap()[0].para,
            vec!["siger@local", "ente@x.co", "op@x.co", "sup@x.co"],
            "con todo en pie el aviso va al operador, al supervisor y al ente"
        );

        // Y AHORA falla SOLO la del tercero. `usuarios` intacta: operador y
        // supervisor se leen bien, así que `bcc` no queda vacío — que es
        // exactamente lo que hacía que el aviso saliera a medias y se marcara.
        let bd2 = base().await;
        escenario(&bd2).await;
        sqlx::query("ALTER TABLE stakeholders RENAME COLUMN email TO correo_x")
            .execute(&bd2).await.unwrap();
        let antes = cuantos(&buzon);
        let r = con_timeout(&bd2, &c, hoy()).await;

        assert_eq!(cuantos(&buzon), antes, "NO sale un aviso a medias");
        assert_eq!(r.proximidad, 0, "y no se cuenta como avisada");
        assert_eq!(r.fallidas, 1, "se cuenta como fallida, que es lo que es");
        assert!(r.incompleta(), "y la pasada lo DICE");
        assert_eq!(marcadas(&bd2).await, 0, "y NO se marca: mañana se reintenta entero");

        // LA OTRA MITAD, que el mutante exigió: romper la lectura del OPERADOR y
        // solo ésa. No se puede con supervisión encendida —comparte tabla y
        // columna, y su consulta corre antes y aborta la pasada—, así que se
        // apaga y se deja al TERCERO como el destinatario que sobrevive. El
        // resultado es el mismo escenario peligroso visto desde el otro lado.
        let bd3 = base().await;
        escenario(&bd3).await;
        let mut solo_tercero = cfg_en("127.0.0.1", puerto);
        solo_tercero.a_supervision = false;
        solo_tercero.a_tercero = true;
        sqlx::query("ALTER TABLE usuarios RENAME COLUMN email TO correo_x")
            .execute(&bd3).await.unwrap();
        let antes = cuantos(&buzon);
        let r = con_timeout(&bd3, &solo_tercero, hoy()).await;

        assert_eq!(cuantos(&buzon), antes, "tampoco sale a medias por este lado");
        assert_eq!(r.fallidas, 1);
        assert!(r.incompleta());
        assert_eq!(marcadas(&bd3).await, 0);
    }

    /// La pasada deja rastro **también cuando va bien**, y ésa es la mitad que
    /// distingue «hoy no había a quién avisar» de «el reloj lleva tres semanas
    /// muerto». Sin ella, la bitácora vacía significa las dos cosas a la vez.
    #[tokio::test]
    async fn la_pasada_buena_tambien_deja_su_linea() {
        let bd = base().await;
        let (puerto, _buzon) = sink_ok();

        // Base vacía: nada que avisar, y aun así la pasada consta.
        let r = con_timeout(&bd, &cfg_en("127.0.0.1", puerto), hoy()).await;
        assert_eq!(r, Resumen::default());
        let ev = eventos(&bd).await;
        assert_eq!(ev.len(), 1, "una pasada, una línea");
        assert_eq!(ev[0].0, "alertas_pasada");
        assert!(ev[0].1.contains("fallidas=0"), "con sus números: {:?}", ev[0].1);

        // Y una segunda pasada con envíos reales no se confunde con la primera.
        siembra(&bd, 2).await;
        con_timeout(&bd, &cfg_en("127.0.0.1", puerto), hoy()).await;
        let ev = eventos(&bd).await;
        assert_eq!(ev.len(), 2);
        assert!(ev[1].1.contains("proximidad=2"), "{:?}", ev[1].1);
    }

    /// Discriminación fina: en una misma pasada, unas con destinatario y otras
    /// sin él. Las que tienen correo se avisan y marcan; las que no, se cuentan
    /// aparte y NO se marcan (para salir cuando alguien configure el correo).
    #[tokio::test]
    async fn distingue_enviado_de_sin_destinatario() {
        let bd = base().await;
        sqlx::query("INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) VALUES (1,'Op','op@x.co','h','OPERADOR',1)").execute(&bd).await.unwrap();
        sqlx::query("INSERT INTO informes (id,nombre,recurrencia) VALUES (1,'Inf','{\"modo\":\"INMEDIATO\"}')").execute(&bd).await.unwrap();
        for k in 0..3 {
            sqlx::query("INSERT INTO entregas (informe_id,usuario_id,fecha_nominal,fecha_limite,estado) VALUES (1,1,?,'2026-06-01','PENDIENTE')")
                .bind(format!("con{k}")).execute(&bd).await.unwrap();
        }
        for k in 0..2 {
            sqlx::query("INSERT INTO entregas (informe_id,usuario_id,fecha_nominal,fecha_limite,estado) VALUES (1,NULL,?,'2026-06-01','PENDIENTE')")
                .bind(format!("sin{k}")).execute(&bd).await.unwrap();
        }
        let (puerto, buzon) = sink_ok();
        let r = con_timeout(&bd, &cfg_en("127.0.0.1", puerto), hoy()).await;
        assert_eq!(r.proximidad, 3, "las 3 con correo se avisan");
        assert_eq!(r.sin_destinatario, 2, "las 2 sin correo se cuentan aparte");
        assert_eq!(r.fallidas, 0);
        assert_eq!(marcadas(&bd).await, 3, "solo las enviadas se marcan");
        assert_eq!(cuantos(&buzon), 3);
        // **La mitad que decide la forma de la regla**: faltarle el correo a
        // alguien NO es una pasada incompleta. Si lo fuera, cualquier cliente con
        // una ficha sin correo tendría la alarma encendida para siempre, y a la
        // tercera semana nadie la miraría (directiva 23).
        assert!(!r.incompleta(), "sin_destinatario no enciende la alarma");
    }
}
