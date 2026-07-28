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
        .await?
        .and_then(|(f,)| NaiveDate::parse_from_str(&f, "%Y-%m-%d").ok());

        let mut desde = match (ultima, regla.vigencia) {
            (Some(u), _) => u + Duration::days(1),
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
pub fn arrancar_reloj(bd: SqlitePool, motor: Motor) {
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
                Err(e) => tracing::error!("planificador: falló la generación — {e}"),
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
