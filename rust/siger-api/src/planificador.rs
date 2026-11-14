//! De la regla a las tareas. Es lo que hace que el sistema funcione **solo**.
//!
//! # Las tres decisiones que gobiernan este módulo
//!
//! **1. La identidad de una entrega es su día NOMINAL.** La clave por la que se
//! decide «esto ya existe» es `(informe, fecha_nominal)`, nunca el día límite:
//! dos días marcados en la misma semana pueden compartir límite tras el ajuste,
//! y con la clave puesta en el límite el segundo se perdía por «ya está».
//!
//! **2. Se barren también días nominales FUTUROS.** Con ajuste anticipado el día
//! límite llega antes que el nominal: un informe que vence el 1 de enero y se
//! entrega el hábil anterior tiene que aparecer en diciembre. Barrer solo hasta
//! hoy lo haría nacer ya vencido.
//!
//! **3. La tarea aparece cuando llega la primera de sus dos fechas**, no cuando
//! llega la nominal. Con gracia o sin ajuste esa primera es la nominal y no
//! cambia nada; con anticipación es la límite, y ahí está la diferencia.
//!
//! # Idempotencia: la garantía la da la BASE, no este código
//!
//! El esquema tiene `UNIQUE (informe_id, fecha_nominal)`. Aunque dos corridas se
//! solapen —el arranque y el reloj a medianoche, por ejemplo— la segunda no
//! duplica: la base la rechaza. Comprobar antes de insertar sería una carrera
//! con ventana; dejar que la restricción decida no la tiene.

use chrono::{Duration, NaiveDate, Utc};
use siger_core::{Motor, Regla, ReglaJson};
use sqlx::SqlitePool;

/// Cuántos días nominales por delante se miran.
///
/// Con ajuste anticipado el límite llega antes que el nominal, así que la tarea
/// tiene que existir antes de que su fecha nominal aparezca en el barrido. Un
/// puente largo mueve la fecha cuatro o cinco días; 31 cubre cualquier
/// calendario con margen y no crea nada por adelantado, porque una tarea solo
/// nace cuando llega la primera de sus dos fechas.
const VENTANA_ANTICIPACION: i64 = 31;

/// Cuánto se mira hacia atrás al generar por primera vez.
///
/// Sin tope, activar un informe con vigencia de 2015 crearía diez años de
/// entregas vencidas de golpe: cientos de incumplimientos inventados que nadie
/// pidió y que hay que borrar a mano.
const MAX_RETROACTIVO_DIAS: i64 = 730;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Resumen {
    pub informes_revisados: usize,
    pub entregas_creadas: usize,
    /// Reglas que no se pudieron leer. **No se ignoran en silencio**: un informe
    /// activo cuya regla no se entiende es un informe que nunca generará una
    /// tarea, y eso tiene que salir a la superficie.
    pub reglas_ilegibles: Vec<(i64, String)>,
}

/// Genera las entregas que faltan. Devuelve qué hizo, para que quien la llama
/// pueda registrarlo o enseñarlo.
pub async fn generar(bd: &SqlitePool, motor: &Motor, hoy: NaiveDate) -> Result<Resumen, sqlx::Error> {
    let mut resumen = Resumen::default();

    let informes = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, nombre, recurrencia FROM informes WHERE activo = 1",
    )
    .fetch_all(bd)
    .await?;

    for (id, nombre, recurrencia) in informes {
        resumen.informes_revisados += 1;

        let regla = match serde_json::from_str::<ReglaJson>(&recurrencia)
            .map_err(|e| e.to_string())
            .and_then(|j| Regla::try_from(&j).map_err(|e| e.to_string()))
        {
            Ok(r) => r,
            Err(motivo) => {
                resumen.reglas_ilegibles.push((id, format!("{nombre}: {motivo}")));
                continue;
            }
        };

        // Desde dónde barrer: el día siguiente a la última entrega generada, o
        // la vigencia si no hay ninguna.
        let ultima = sqlx::query_as::<_, (String,)>(
            "SELECT MAX(fecha_nominal) FROM entregas WHERE informe_id = ? AND fecha_nominal IS NOT NULL",
        )
        .bind(id)
        .fetch_optional(bd)
        .await?;
        // El `.ok()` que había aquí convertía una fecha ilegible en «no hay
        // ninguna entrega todavía», y desde ahí se barre **desde la vigencia**:
        // el informe volvería a generar su historia entera. Hoy es inalcanzable
        // —los dos escritores producen ISO: `generar` con `to_string()` e
        // `importar` validando con `solo_fecha`—, pero «no sale en ningún sitio»
        // y «no pasa nada» no son lo mismo, y aquí el precio de equivocarse es
        // duplicar obligaciones en la cara del cliente.
        //
        // Sale por el camino que este archivo ya tiene para lo que no se puede
        // ejecutar, y que el bucle de arriba escala a `error!`: no se toca ese
        // informe y los demás siguen.
        let ultima = match ultima {
            // **`MAX()` sobre cero filas devuelve una fila con NULL**, que aquí
            // llega como cadena vacía. Eso NO es una anomalía: es «este informe
            // todavía no ha generado nada», y pasa con cada informe nuevo. El
            // `.ok()` que había antes hacía los dos trabajos a la vez —tragarse
            // el caso legítimo y tragarse el error de verdad— y por eso no se
            // veía. Son dos significados, y ahora son dos brazos (directiva 32).
            //
            // Lo enseñó el banco: la primera versión de este arreglo mandó los
            // cuatro casos normales a `reglas_ilegibles` con el motivo «la última
            // fecha generada «» no es una fecha».
            None => None,
            Some((f,)) if f.is_empty() => None,
            Some((f,)) => match NaiveDate::parse_from_str(&f, "%Y-%m-%d") {
                Ok(d) => Some(d),
                Err(e) => {
                    resumen.reglas_ilegibles.push((
                        id,
                        format!("{nombre}: la última fecha generada «{f}» no es una fecha ({e})"),
                    ));
                    continue;
                }
            },
        };

        let mut desde = match (ultima, regla.vigencia) {
            // **La suma va `checked`, y esta lección ya estaba escrita** en el
            // CLAUDE.md para `licencia.rs`: «`NaiveDate + Duration` entra en
            // pánico al desbordar, y `%Y-%m-%d` acepta `+262142-12-31`». No se
            // aplicó aquí, y aquí es peor: `arrancar_reloj` es un `tokio::spawn`
            // sin supervisor, así que un pánico **mata el reloj para siempre**
            // sin tumbar el servidor. No se generarían más entregas, no saldría
            // ninguna alerta, y ni siquiera se escribiría la fila diaria
            // `alertas_pasada` que existe para distinguir «no hacía falta» de
            // «el reloj lleva tres semanas muerto». Un `Err` lo ve el instrumento
            // nuevo; un pánico, no — que es justo la forma que el defecto tomó.
            //
            // Y es alcanzable: `+262142-12-31` es `NaiveDate::MAX` y **atraviesa
            // `importar::solo_fecha`**, porque es un punto fijo de parsear y
            // volver a formatear. Una base heredada del cliente lo deja escrito.
            (Some(u), _) => match u.checked_add_signed(Duration::days(1)) {
                Some(d) => d,
                None => {
                    resumen.reglas_ilegibles.push((
                        id,
                        format!("{nombre}: la última fecha generada «{u}» no admite el día siguiente"),
                    ));
                    continue;
                }
            },
            (None, Some(v)) => v,
            // Sin vigencia y sin historia no hay desde dónde contar. No se
            // inventa un origen: se deja sin generar, y el informe aparece en el
            // resumen como revisado y sin novedad.
            (None, None) => continue,
        };
        if desde < hoy - Duration::days(MAX_RETROACTIVO_DIAS) {
            desde = hoy - Duration::days(MAX_RETROACTIVO_DIAS);
        }

        let hasta = hoy + Duration::days(VENTANA_ANTICIPACION);
        if desde > hasta {
            continue;
        }

        // A quién se le asigna: quien ejecuta o produce. Si nadie está asignado,
        // la entrega se crea igual, sin dueño — la obligación existe aunque la
        // organización todavía no haya decidido quién la atiende, y hacerla
        // desaparecer por eso sería ocultar el problema en vez de mostrarlo.
        let responsable = sqlx::query_as::<_, (i64,)>(
            "SELECT usuario_id FROM participacion \
             WHERE informe_id = ? AND usuario_id IS NOT NULL AND rol IN ('EJECUTA','ORIGEN') \
             ORDER BY es_principal DESC, orden LIMIT 1",
        )
        .bind(id)
        .fetch_optional(bd)
        .await?
        .map(|(u,)| u);

        for v in motor.vencimientos_entre(desde, hasta, &regla) {
            let nominal = v.nominal.fecha();
            let limite = v.limite.fecha();

            // Aparece cuando llega la primera de sus dos fechas.
            if nominal.min(limite) > hoy {
                continue;
            }

            let insertadas = sqlx::query(
                "INSERT OR IGNORE INTO entregas \
                 (informe_id, usuario_id, fecha_nominal, fecha_limite, tipo_ajuste, estado) \
                 VALUES (?, ?, ?, ?, ?, 'PENDIENTE')",
            )
            .bind(id)
            .bind(responsable)
            .bind(nominal.to_string())
            .bind(limite.to_string())
            .bind(format!("{:?}", v.ajuste).to_uppercase())
            .execute(bd)
            .await?
            .rows_affected();

            resumen.entregas_creadas += insertadas as usize;
        }
    }

    Ok(resumen)
}

/// El reloj. Genera al arrancar y después una vez al día.
///
/// Generar **al arrancar** y no solo a medianoche es deliberado: una instalación
/// que se apaga por las noches —un portátil en una oficina, que es justo el
/// perfil `standalone`— no vería nunca la medianoche, y el sistema no crearía
/// una sola tarea sin que nadie entendiera por qué.
/// **Lo que este `spawn` no puede sobrevivir, y por eso aquí dentro no se
/// permite un pánico.** No hay supervisor, no hay `JoinHandle` guardado y el
/// perfil no lleva `panic = "abort"`: un pánico desenrolla el task, el task
/// desaparece para el resto de la vida del proceso, y `axum` sigue contestando
/// 200 tan campante. No se generarían más entregas, no saldría ninguna alerta, y
/// **ni siquiera se escribiría la fila diaria de la bitácora** que existe para
/// distinguir «hoy no había a quién avisar» de «esto lleva tres semanas muerto».
///
/// Un `Err` lo ve el instrumento; un pánico, no. Por eso la aritmética de fechas
/// de este archivo va `checked` y `ALERTAS_DIAS_PROXIMIDAD` está acotado en el
/// arranque: se quita la causa en vez de añadir un supervisor que la contenga
/// (directiva 1). Quien toque este bucle hereda esa obligación.
pub fn arrancar_reloj(bd: SqlitePool, motor: Motor, correo: Option<crate::config::ConfigCorreo>) {
    tokio::spawn(async move {
        loop {
            let hoy = Utc::now().date_naive();
            match generar(&bd, &motor, hoy).await {
                Ok(r) => {
                    tracing::info!(
                        "planificador: {} informes revisados, {} entregas creadas",
                        r.informes_revisados,
                        r.entregas_creadas
                    );
                    for (id, motivo) in &r.reglas_ilegibles {
                        // Un aviso no basta para una regla rota, pero es lo
                        // único que puede hacer un proceso de fondo: quien lo
                        // arregla es una persona, y esto es lo que le llega.
                        tracing::error!("informe {id}: la regla no se puede ejecutar — {motivo}");
                    }
                }
                Err(e) => {
                    // **La generación es lo que CREA las obligaciones**, así que
                    // su fallo pesa más que el de las alertas — y hasta hoy solo
                    // constaba en el log del proceso, mientras las alertas ya
                    // dejaban su línea en la bitácora. La asimetría la señaló la
                    // revisión de seguridad.
                    tracing::error!("planificador: falló la generación — {e}");
                    crate::rutas::auditar(
                        &bd,
                        "planificador_fallido",
                        None,
                        Some(&format!("no se pudieron generar las entregas — {e}")),
                    )
                    .await;
                }
            }

            // Las alertas van DESPUÉS de generar, nunca antes: un correo caído no
            // puede impedir que las tareas existan. Solo si el correo está activo.
            if let Some(c) = &correo {
                // El resumen lo emite y lo audita `alertas::correr` por dentro,
                // escalando a `error!` cuando la pasada queda incompleta. Aquí
                // se emitía un `info!` fijo que decía «120 fallidas» con el
                // mismo tono que «todo bien» — el mismo archivo ya escalaba a
                // `error!` las reglas ilegibles, y a las alertas no.
                let _ = crate::alertas::correr(&bd, c, hoy).await;
            }

            tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use siger_core::Calendario;

    async fn base() -> SqlitePool {
        let bd = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&bd).await.unwrap();
        bd
    }

    fn motor() -> Motor {
        Motor::nuevo(Calendario::de_pais("CO", 2024, 2030).unwrap())
    }

    async fn con_regla(bd: &SqlitePool, regla: &str) {
        sqlx::query("INSERT INTO informes (id, nombre, recurrencia) VALUES (1, 'Prueba', ?)")
            .bind(regla)
            .execute(bd)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn genera_las_entregas_de_la_regla() {
        let bd = base().await;
        con_regla(&bd, r#"{"modo":"ESPECIFICO","meses":[1,2,3,4,5,6,7,8,9,10,11,12],
                          "dia_corte":15,"fecha_inicio":"2026-01-01"}"#).await;

        let r = generar(&bd, &motor(), NaiveDate::from_ymd_opt(2026, 6, 30).unwrap())
            .await
            .unwrap();
        // Enero a junio: seis días 15 ya pasados. El de julio todavía no
        // aparece, porque su primera fecha (15 de julio) no ha llegado.
        assert_eq!(r.entregas_creadas, 6, "{r:?}");
    }

    /// **Una fecha ilegible en la base no puede leerse como «este informe está
    /// recién nacido»**, porque desde ahí se barre desde la vigencia y el
    /// informe regeneraría su historia entera — duplicando obligaciones en la
    /// cara del cliente.
    ///
    /// La pareja va dentro y es la que hizo falta escribir: el mismo camino con
    /// la tabla VACÍA tiene que seguir generando con normalidad, porque `MAX()`
    /// sobre cero filas devuelve una fila con NULL —cadena vacía aquí— y ése es
    /// el caso legítimo que ocurre con cada informe nuevo. El `.ok()` anterior
    /// confundía los dos, y la primera versión de este arreglo los confundió al
    /// revés: mandó los cuatro casos normales a `reglas_ilegibles`.
    #[tokio::test]
    async fn una_fecha_ilegible_no_se_lee_como_informe_recien_nacido() {
        let regla = r#"{"modo":"ESPECIFICO","meses":[1,2,3,4,5,6,7,8,9,10,11,12],
                        "dia_corte":15,"fecha_inicio":"2026-01-01"}"#;
        let hoy = NaiveDate::from_ymd_opt(2026, 6, 30).unwrap();

        // EL CONTROL: tabla vacía → `MAX()` da NULL → se genera con normalidad.
        let bd = base().await;
        con_regla(&bd, regla).await;
        let r = generar(&bd, &motor(), hoy).await.unwrap();
        assert_eq!(r.entregas_creadas, 6, "sin entregas previas se genera: {r:?}");
        assert!(r.reglas_ilegibles.is_empty(), "y eso NO es una regla ilegible: {r:?}");

        // EL CASO ROJO: una fecha que no es una fecha, escrita en la base.
        let bd2 = base().await;
        con_regla(&bd2, regla).await;
        sqlx::query("INSERT INTO entregas (informe_id,usuario_id,fecha_nominal,fecha_limite,estado) VALUES (1,NULL,'no-es-fecha','2026-03-15','PENDIENTE')")
            .execute(&bd2).await.unwrap();
        let r2 = generar(&bd2, &motor(), hoy).await.unwrap();

        assert_eq!(r2.entregas_creadas, 0, "no se genera nada sobre una base que no se entiende");
        assert_eq!(r2.reglas_ilegibles.len(), 1, "y sale a la superficie: {r2:?}");
        assert!(
            r2.reglas_ilegibles[0].1.contains("no-es-fecha"),
            "el motivo dice cuál es el valor raro: {:?}", r2.reglas_ilegibles[0].1
        );
    }

    /// **El reloj no puede MORIR de pánico**, que es la única forma de fallo que
    /// el instrumento de la bitácora no puede ver: `arrancar_reloj` es un
    /// `tokio::spawn` sin supervisor, así que un pánico se lleva la generación y
    /// las alertas para siempre sin tumbar el servidor — y sin escribir ni
    /// siquiera la fila diaria que existe para distinguir «hoy no hacía falta»
    /// de «esto lleva tres semanas muerto».
    ///
    /// `+262142-12-31` es `NaiveDate::MAX` y **atraviesa `importar::solo_fecha`**
    /// porque es un punto fijo de parsear y volver a formatear, así que una base
    /// heredada del cliente lo deja escrito. Sumarle un día desbordaba.
    ///
    /// La pareja va dentro: una fecha grande PERO válida se suma con normalidad.
    #[tokio::test]
    async fn una_fecha_en_el_limite_no_mata_el_reloj() {
        let regla = r#"{"modo":"ESPECIFICO","meses":[1,2,3,4,5,6,7,8,9,10,11,12],
                        "dia_corte":15,"fecha_inicio":"2026-01-01"}"#;
        let hoy = NaiveDate::from_ymd_opt(2026, 6, 30).unwrap();

        // EL CASO ROJO: la fecha máxima que chrono admite. `u + 1 día` desborda.
        let bd = base().await;
        con_regla(&bd, regla).await;
        sqlx::query("INSERT INTO entregas (informe_id,usuario_id,fecha_nominal,fecha_limite,estado) VALUES (1,NULL,'+262142-12-31','2026-03-15','PENDIENTE')")
            .execute(&bd).await.unwrap();

        let r = generar(&bd, &motor(), hoy).await.expect("generar NO puede entrar en pánico ni fallar");
        assert_eq!(r.reglas_ilegibles.len(), 1, "sale a la superficie: {r:?}");
        assert!(r.reglas_ilegibles[0].1.contains("día siguiente"), "{:?}", r.reglas_ilegibles[0].1);

        // LA PAREJA: una fecha grande pero con margen se trata con normalidad y
        // NO se reporta. Sin esto, un arreglo que reportara cualquier fecha
        // lejana pasaría igual.
        let bd2 = base().await;
        con_regla(&bd2, regla).await;
        sqlx::query("INSERT INTO entregas (informe_id,usuario_id,fecha_nominal,fecha_limite,estado) VALUES (1,NULL,'2026-03-15','2026-03-15','PENDIENTE')")
            .execute(&bd2).await.unwrap();
        let r2 = generar(&bd2, &motor(), hoy).await.unwrap();
        assert!(r2.reglas_ilegibles.is_empty(), "una fecha normal no es ilegible: {r2:?}");
        assert!(r2.entregas_creadas > 0, "y se sigue generando: {r2:?}");
    }

    /// **Una regla venenosa YA GUARDADA no puede tumbar la pasada entera.**
    ///
    /// Es la regresión que el arreglo de la fecha imposible podía introducir sin
    /// que se notara: `Regla::try_from` ahora falla con una `fecha_inicio` fuera
    /// de rango, y si eso abortara `generar`, un solo informe corrupto —heredado
    /// de una base importada— dejaría a TODOS los demás sin generar entregas.
    ///
    /// La pareja va dentro: en la misma pasada, el informe sano sí genera.
    #[tokio::test]
    async fn un_informe_con_fecha_imposible_no_deja_a_los_demas_sin_generar() {
        let bd = base().await;
        // El venenoso, escrito directamente en la base como llegaría de un
        // volcado: `regla_valida` ya no lo dejaría entrar por la API.
        sqlx::query("INSERT INTO informes (id, nombre, recurrencia) VALUES (1, 'Veneno', ?)")
            .bind(r#"{"modo":"SEMANAL","dia_habil_semana":3,"cada_n_semanas":2,"fecha_inicio":"-262143-01-01"}"#)
            .execute(&bd).await.unwrap();
        // Y uno sano al lado.
        sqlx::query("INSERT INTO informes (id, nombre, recurrencia) VALUES (2, 'Sano', ?)")
            .bind(r#"{"modo":"ESPECIFICO","meses":[1,2,3,4,5,6],"dia_corte":15,"fecha_inicio":"2026-01-01"}"#)
            .execute(&bd).await.unwrap();

        let r = generar(&bd, &motor(), NaiveDate::from_ymd_opt(2026, 6, 30).unwrap())
            .await
            .expect("la pasada NO puede fallar entera por un informe corrupto");

        assert_eq!(r.informes_revisados, 2);
        assert_eq!(r.reglas_ilegibles.len(), 1, "el venenoso sale a la superficie: {r:?}");
        assert!(r.reglas_ilegibles[0].1.contains("fuera del rango"), "{:?}", r.reglas_ilegibles[0].1);
        assert!(r.entregas_creadas > 0, "y el SANO genera igual: {r:?}");
    }

    #[tokio::test]
    async fn correr_dos_veces_no_duplica() {
        // La prueba que discrimina: sin la clave única, la segunda corrida
        // crearía otras seis y nadie lo notaría hasta ver la lista repetida.
        let bd = base().await;
        con_regla(&bd, r#"{"modo":"ESPECIFICO","meses":[1,2,3,4,5,6,7,8,9,10,11,12],
                          "dia_corte":15,"fecha_inicio":"2026-01-01"}"#).await;
        let hoy = NaiveDate::from_ymd_opt(2026, 6, 30).unwrap();

        generar(&bd, &motor(), hoy).await.unwrap();
        let segunda = generar(&bd, &motor(), hoy).await.unwrap();

        assert_eq!(segunda.entregas_creadas, 0);
        let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM entregas")
            .fetch_one(&bd)
            .await
            .unwrap();
        assert_eq!(total, 6);
    }

    #[tokio::test]
    async fn el_ajuste_anticipado_nace_antes_que_su_dia_nominal() {
        // El 1 de enero de 2027 es viernes y festivo: con ajuste anticipado el
        // límite cae el 31 de diciembre de 2026. La tarea tiene que existir el
        // 31, no el 1: si naciera con la fecha nominal, nacería ya vencida.
        let bd = base().await;
        con_regla(&bd, r#"{"modo":"ESPECIFICO","meses":[1],"dia_corte":1,
                          "mover_habil_anterior":true,"fecha_inicio":"2026-12-01"}"#).await;

        let r = generar(&bd, &motor(), NaiveDate::from_ymd_opt(2026, 12, 31).unwrap())
            .await
            .unwrap();
        assert_eq!(r.entregas_creadas, 1, "{r:?}");

        let (nominal, limite): (String, String) =
            sqlx::query_as("SELECT fecha_nominal, fecha_limite FROM entregas")
                .fetch_one(&bd)
                .await
                .unwrap();
        assert_eq!(nominal, "2027-01-01");
        assert_eq!(limite, "2026-12-31");
    }

    #[tokio::test]
    async fn una_regla_ilegible_se_reporta_en_vez_de_ignorarse() {
        let bd = base().await;
        con_regla(&bd, r#"{"modo":"LO_QUE_SEA"}"#).await;

        let r = generar(&bd, &motor(), NaiveDate::from_ymd_opt(2026, 6, 30).unwrap())
            .await
            .unwrap();
        assert_eq!(r.entregas_creadas, 0);
        assert_eq!(r.reglas_ilegibles.len(), 1, "una regla rota tiene que salir a la superficie");
    }

    #[tokio::test]
    async fn no_genera_diez_anios_de_atrasos_al_activar_un_informe_viejo() {
        let bd = base().await;
        con_regla(&bd, r#"{"modo":"ESPECIFICO","meses":[1,2,3,4,5,6,7,8,9,10,11,12],
                          "dia_corte":15,"fecha_inicio":"2010-01-01"}"#).await;

        let r = generar(&bd, &motor(), NaiveDate::from_ymd_opt(2026, 6, 30).unwrap())
            .await
            .unwrap();
        // Dos años de tope, no dieciséis.
        assert!(r.entregas_creadas <= 25, "generó {} entregas", r.entregas_creadas);
        assert!(r.entregas_creadas >= 23, "generó {} entregas", r.entregas_creadas);
    }
}
