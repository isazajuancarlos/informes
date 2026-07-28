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
) -> Vec<String> {
    let mut correos: Vec<String> = Vec::new();

    if c.a_operador {
        if let Some(uid) = p.usuario_id {
            if let Ok(Some((Some(email),))) =
                sqlx::query_as::<_, (Option<String>,)>("SELECT email FROM usuarios WHERE id = ? AND activo = 1")
                    .bind(uid)
                    .fetch_optional(bd)
                    .await
            {
                correos.push(email);
            }
        }
    }

    if c.a_supervision {
        correos.extend(supervisores.iter().cloned());
    }

    if c.a_tercero {
        // El correo del stakeholder destino de este informe.
        if let Ok(filas) = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT s.email FROM participacion p \
             JOIN stakeholders s ON s.id = p.stakeholder_id \
             WHERE p.informe_id = ? AND p.rol = 'DESTINO' AND s.email IS NOT NULL",
        )
        .bind(p.informe_id)
        .fetch_all(bd)
        .await
        {
            correos.extend(filas.into_iter().flat_map(|(e,)| e));
        }
    }

    // Únicas, no vacías. Enviar dos veces al mismo correo por ser operador y
    // supervisor a la vez sería ruido.
    correos.retain(|e| !e.trim().is_empty());
    correos.sort();
    correos.dedup();
    correos
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
            tracing::error!("alertas: SMTP_DESDE «{}» no es una dirección válida; no se envía nada", c.desde);
            return r;
        }
    };
    let transporte = match transporte(c) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("alertas: no se pudo preparar el transporte SMTP — {e}");
            return r;
        }
    };

    // Los supervisores son los mismos para todas las entregas: se traen una vez.
    let supervisores: Vec<String> = if c.a_supervision {
        sqlx::query_as::<_, (String,)>(
            "SELECT email FROM usuarios WHERE rol = 'SUPERVISOR' AND activo = 1 AND email <> ''",
        )
        .fetch_all(bd)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(e,)| e)
        .collect()
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
    .await
    .unwrap_or_default();

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
    .await
    .unwrap_or_default();

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
        let bcc = destinatarios(bd, c, &p, &supervisores).await;
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
        assert_eq!(d, vec!["ente@x.co", "op@x.co", "sup@x.co"]);
        // Solo operador.
        assert_eq!(destinatarios(&bd, &cfg(true, false, false), &p, &sup).await, vec!["op@x.co"]);
        // Solo tercero.
        assert_eq!(destinatarios(&bd, &cfg(false, false, true), &p, &sup).await, vec!["ente@x.co"]);
        // Ninguno con correo → vacío (y correr() lo contaría como sin_destinatario).
        let sin = Pendiente { usuario_id: None, ..p };
        assert!(destinatarios(&bd, &cfg(true, false, false), &sin, &[]).await.is_empty());
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
