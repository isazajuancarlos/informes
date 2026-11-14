//! La puerta de entrada desde lo que ya está guardado.
//!
//! `cat_informes.recurrencia_json` lleva años recibiendo diccionarios sueltos, y
//! esas filas existen en la base de cada cliente. Este módulo las lee y las
//! convierte **una vez** a la forma tipada de [`crate::recurrencia::Regla`]; a
//! partir de ahí ninguna capa vuelve a ver un campo opcional que puede
//! contradecir a otro.
//!
//! Traducir al leer, y no al usar, es lo que permite que una regla imposible
//! —«día de corte 5 **y** último día del mes»— deje de existir aguas abajo en
//! lugar de resolverse en silencio cada vez que alguien la consulta.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::recurrencia::{Ajuste, DiaDelMes, Especifico, Patron, Regla, Semanal};

#[derive(Debug, thiserror::Error)]
pub enum ErrorRegla {
    #[error("modo de recurrencia desconocido: «{0}»")]
    ModoDesconocido(String),
    #[error("el modo {0} no trae los campos que necesita: {1}")]
    CamposIncompletos(&'static str, &'static str),
    #[error(
        "el modo «{0}» está RETIRADO: una obligación por horas no se puede \
         representar. La tabla `entregas` guarda una FECHA nominal y su clave \
         única es (informe, fecha_nominal), así que de «cada cuatro horas» solo \
         sobrevivía una entrega al día — sin error y sin aviso. Convierta la \
         regla a diaria (CICLICO, intervalo 1) si eso era lo que hacía, o \
         enumere los días con FECHAS."
    )]
    ModoRetirado(String),
    #[error(
        "la fecha «{0}» está fuera del rango de trabajo ({1}..={2}). `chrono` \
         acepta hasta el año 262142, pero el motor de calendario entra en pánico \
         cerca de esos extremos —y una obligación de reporte con vigencia en el \
         año -262143 no es un cliente raro, es un dato corrupto."
    )]
    FechaFueraDeRango(String, i32, i32),
}

/// Espejo exacto del diccionario que guarda el sistema actual. Todos los campos
/// son opcionales porque en la base lo son: eso es precisamente el problema que
/// la conversión resuelve.
/// La regla tal como viaja y se guarda.
///
/// Todo campo vacío se OMITE al serializar (`skip_serializing_if`). No es
/// cosmética: `informes::crear` guarda la regla verbatim «para que los informes
/// nuevos queden en el mismo JSON compacto que los migrados, sin campos nulos de
/// relleno», y sin esto una regla construida en Rust —como las que devuelve la
/// inferencia— llegaba a la base con veinte `null` explícitos mientras las
/// migradas tenían cuatro claves. Dos formas del mismo dato, y dentro de un año
/// nadie sabría cuál es la canónica.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReglaJson {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fecha_inicio: Option<String>,

    // Cíclico
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intervalo: Option<i64>,

    // Específico
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meses: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dia_corte: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dias_corte: Option<Vec<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub es_ultimo_dia: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dia_habil: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dia_semana_mes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semana_del_mes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cada_n_anios: Option<i64>,

    // Semanal
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dias_semana: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dia_habil_semana: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cada_n_semanas: Option<i64>,

    // Los campos del modo horario (`hora_inicio`, `hora_fin`, `intervalo_horas`)
    // se quitaron con el modo. Una regla guardada que los traiga los pierde al
    // reescribirse, y da igual: su modo ya no se puede leer, así que esa regla
    // hay que rehacerla de todos modos.

    // Fechas explícitas
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fechas: Option<Vec<String>>,

    // Ajuste. Dos parejas de banderas para lo mismo, porque el específico usaba
    // una y el cíclico la otra. Esa divergencia ya costó que el ajuste
    // anticipado no se aplicara nunca en modo semanal; aquí se leen las dos y se
    // colapsan en un único valor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ajuste_habil: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mover_habil: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ajuste_habil_anterior: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mover_habil_anterior: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub omitir_festivos: Option<bool>,
}

fn si(v: Option<bool>) -> bool {
    v.unwrap_or(false)
}

/// Positivo o nada. Un `0` o un negativo en un campo que cuenta cosas no es un
/// valor: es un campo sin llenar o un dedazo.
fn positivo(v: Option<i64>) -> Option<u32> {
    match v {
        Some(n) if n > 0 => Some(n as u32),
        _ => None,
    }
}

impl ReglaJson {
    /// El ajuste declarado, con la precedencia del motor anterior: gracia gana a
    /// omitir, y omitir gana a anticipado.
    pub fn ajuste(&self) -> Ajuste {
        if si(self.ajuste_habil) || si(self.mover_habil) {
            Ajuste::Gracia
        } else if si(self.omitir_festivos) {
            Ajuste::OmitirFestivos
        } else if si(self.ajuste_habil_anterior) || si(self.mover_habil_anterior) {
            Ajuste::Anticipado
        } else {
            Ajuste::Ninguno
        }
    }

    fn dia_del_mes(&self) -> DiaDelMes {
        // El orden es la precedencia del motor anterior, medida y no supuesta.
        if let (Some(ds), Some(sem)) = (self.dia_semana_mes, self.semana_del_mes) {
            if sem != 0 {
                return DiaDelMes::OrdinalDeSemana {
                    dia_semana: ds.rem_euclid(7) as u32,
                    ordinal: sem as i32,
                };
            }
        }
        if let Some(dias) = &self.dias_corte {
            if !dias.is_empty() {
                return DiaDelMes::Cortes(
                    dias.iter().map(|d| (*d).clamp(0, 31) as u32).collect(),
                );
            }
        }
        if let Some(n) = positivo(self.dia_habil) {
            return DiaDelMes::Habil(n);
        }
        if si(self.es_ultimo_dia) {
            return DiaDelMes::Ultimo;
        }
        DiaDelMes::Corte(self.dia_corte.unwrap_or(1).clamp(0, 31) as u32)
    }
}

impl TryFrom<&ReglaJson> for Regla {
    type Error = ErrorRegla;

    fn try_from(j: &ReglaJson) -> Result<Self, Self::Error> {
        // Una fecha de vigencia ILEGIBLE se trata como ausente, igual que antes:
        // la regla sigue siendo válida, simplemente no acota desde cuándo.
        //
        // Una fecha LEGIBLE PERO IMPOSIBLE es otra cosa, y no puede compartir
        // camino con la anterior (directiva 32): tratarla como ausente
        // convertiría «desde el año -262143» en «sin vigencia», que genera un
        // calendario de entregas DISTINTO y en silencio. Es `Err`.
        //
        // **Aquí, y no en los manejadores.** Éste es el embudo único por el que
        // `fecha_inicio` y `fechas[]` se vuelven `NaiveDate`, y lo cruzan los
        // cuatro consumidores: `regla_valida` al crear o editar un informe,
        // `previsualizar_de`, `planificador::generar` y el volcado. Acotar en la
        // entrada de dos manejadores dejaba las otras dos puertas abiertas — y
        // eso ya pasó: la guardia anterior miraba `desde`/`hasta` y la fecha
        // venenosa viajaba DENTRO de la regla.
        let vigencia = match j.fecha_inicio.as_deref().map(str::trim) {
            None => None,
            Some(s) => match NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                Err(_) => None,
                Ok(d) if crate::fechas::en_rango_de_trabajo(d) => Some(d),
                Ok(_) => {
                    return Err(ErrorRegla::FechaFueraDeRango(
                        s.to_string(),
                        crate::fechas::ANIO_MIN,
                        crate::fechas::ANIO_MAX,
                    ))
                }
            },
        };

        let modo = j.modo.as_deref().unwrap_or("CICLICO").trim().to_uppercase();
        let patron = match modo.as_str() {
            "CICLICO" => Patron::Ciclico {
                intervalo: j.intervalo.unwrap_or(0).max(0) as u32,
            },
            "ESPECIFICO" => Patron::Especifico(Especifico {
                meses: j.meses.clone().unwrap_or_default(),
                dia: j.dia_del_mes(),
                cada_n_anios: positivo(j.cada_n_anios),
            }),
            "SEMANAL" => Patron::Semanal(Semanal {
                dias: j.dias_semana.clone().unwrap_or_default(),
                dia_habil: positivo(j.dia_habil_semana),
                cada_n_semanas: positivo(j.cada_n_semanas),
            }),
            // Retirado a propósito, y con un error PROPIO en vez de caer en
            // «modo desconocido»: quien lo vea tiene que entender que no es un
            // dedazo suyo sino una decisión, y qué poner en su lugar. En los 80
            // patrones reales del catálogo migrado no aparece ni una vez.
            "HOURLY" | "HORARIO" => return Err(ErrorRegla::ModoRetirado(modo)),
            "FECHAS" => {
                let mut dias = Vec::new();
                for s in j.fechas.clone().unwrap_or_default() {
                    // Una fecha ilegible no invalida las demás — pero una
                    // imposible sí, por lo mismo que la vigencia: descartarla en
                    // silencio cambiaría la lista de vencimientos sin decírselo
                    // a nadie, y dejarla pasar revienta el motor.
                    match NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d") {
                        Err(_) => continue,
                        Ok(d) if crate::fechas::en_rango_de_trabajo(d) => dias.push(d),
                        Ok(_) => {
                            return Err(ErrorRegla::FechaFueraDeRango(
                                s.trim().to_string(),
                                crate::fechas::ANIO_MIN,
                                crate::fechas::ANIO_MAX,
                            ))
                        }
                    }
                }
                Patron::Fechas(dias)
            }
            // El sistema anterior escribía `INMEDIATO`; se admiten también las dos
            // formas del nombre nuevo. No fija días: es a solicitud.
            "INMEDIATO" | "A_SOLICITUD" | "ASOLICITUD" => Patron::ASolicitud,
            otro => return Err(ErrorRegla::ModoDesconocido(otro.to_string())),
        };

        Ok(Regla { vigencia, patron, ajuste: j.ajuste() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inmediato_del_sistema_viejo_se_lee_como_a_solicitud() {
        // Los 16 informes «INMEDIATO» del catálogo real: existen, pero no vencen
        // por calendario. Si esto se rompe, el volcado vuelve a fallar en ellos.
        let rj: ReglaJson = serde_json::from_str(
            r#"{"modo":"INMEDIATO","descripcion":"Por solicitud","fecha_inicio":"2026-01-01"}"#,
        )
        .unwrap();
        let regla = Regla::try_from(&rj).unwrap();
        assert_eq!(regla.patron, Patron::ASolicitud);
    }

    #[test]
    fn el_nombre_nuevo_tambien_se_admite() {
        for modo in ["A_SOLICITUD", "ASOLICITUD", "a_solicitud"] {
            let rj = ReglaJson { modo: Some(modo.to_string()), ..Default::default() };
            assert_eq!(Regla::try_from(&rj).unwrap().patron, Patron::ASolicitud);
        }
    }

    #[test]
    fn el_modo_horario_esta_retirado_y_lo_dice_con_su_nombre() {
        // Antes esto se leía y producía UNA entrega al día en silencio, porque
        // `entregas` guarda una fecha y su clave única es (informe, fecha). El
        // motor sabía expresar «cada cuatro horas» y la persistencia no, así que
        // el sistema prometía una obligación que no cumplía.
        //
        // Ahora falla, y falla con un error PROPIO: caer en «modo desconocido»
        // haría pensar en un dedazo en vez de en una decisión.
        for modo in ["HORARIO", "HOURLY", "horario"] {
            let rj = ReglaJson { modo: Some(modo.to_string()), ..Default::default() };
            let e = Regla::try_from(&rj).unwrap_err();
            assert!(
                matches!(e, ErrorRegla::ModoRetirado(_)),
                "«{modo}» no dio ModoRetirado sino {e}"
            );
            let texto = e.to_string();
            assert!(texto.contains("RETIRADO"), "{texto}");
            assert!(
                texto.contains("CICLICO") && texto.contains("FECHAS"),
                "el error no dice qué poner en su lugar: {texto}"
            );
        }
    }

    #[test]
    fn un_modo_de_verdad_desconocido_sigue_fallando() {
        // La puerta no se abrió de más: un modo inventado sigue siendo un error,
        // no cae a «a solicitud» por descarte.
        let rj = ReglaJson { modo: Some("QUINCENAL_LUNAR".into()), ..Default::default() };
        assert!(matches!(Regla::try_from(&rj), Err(ErrorRegla::ModoDesconocido(_))));
    }

    /// **La fecha imposible se rechaza en el EMBUDO, que es por donde pasan las
    /// cuatro puertas** — crear un informe, editarlo, previsualizar y volcar.
    ///
    /// El caso que revienta no es el que parece: `+262142-12-31` sale por el
    /// portón de `vencimientos_para` (`fecha < vigencia`) sin tocar nada, y el
    /// que mata es **`-262143-01-01`**, cuatro días por encima de
    /// `NaiveDate::MIN`, con una regla semanal de cada N semanas: el ancla resta
    /// días para buscar el lunes de esa semana y desborda por abajo
    /// (`calendario.rs`, `n_esimo_habil_de_la_semana`). Medido, no razonado.
    ///
    /// Y es `Err` y no «vigencia ausente» a propósito: tratarla como ausente
    /// convertiría «desde el año -262143» en «sin vigencia», que produce un
    /// calendario de entregas distinto y en silencio.
    #[test]
    fn una_fecha_imposible_en_la_regla_se_rechaza_en_vez_de_reventar_el_motor() {
        let regla = |fi: &str| -> ReglaJson {
            serde_json::from_str(&format!(
                r#"{{"modo":"SEMANAL","dia_habil_semana":3,"cada_n_semanas":2,"fecha_inicio":"{fi}"}}"#
            ))
            .unwrap()
        };

        // EL CONTROL, y va primero: una fecha antigua PERO posible se acepta, y
        // el motor la ejecuta. Sin esto, una regla que rechazara todo pasaría.
        let r = Regla::try_from(&regla("1900-01-01")).expect("1900 es legítimo");
        let cal = crate::Calendario::de_pais("CO", 2024, 2030).unwrap();
        let motor = crate::recurrencia::Motor::nuevo(cal);
        let d = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let h = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
        assert!(
            !motor.vencimientos_entre(d, h, &r).is_empty(),
            "el control tiene que generar vencimientos o el rojo no dice nada"
        );

        // EL ROJO por abajo, que es el que revienta.
        assert!(
            matches!(Regla::try_from(&regla("-262143-01-01")), Err(ErrorRegla::FechaFueraDeRango(..))),
            "una vigencia imposible tiene que rechazarse"
        );
        // Y por arriba, para que la guardia no dependa de por qué extremo venga.
        assert!(matches!(
            Regla::try_from(&regla("+262142-12-31")),
            Err(ErrorRegla::FechaFueraDeRango(..))
        ));
        // Una ilegible SIGUE siendo vigencia ausente, no un error: son dos cosas
        // distintas y esta prueba fija que no se confundan.
        assert!(Regla::try_from(&regla("no-es-fecha")).is_ok(), "la ilegible no cambia de trato");

        // Y el modo FECHAS, por el mismo embudo.
        let por_fechas = |f: &str| -> ReglaJson {
            serde_json::from_str(&format!(r#"{{"modo":"FECHAS","fechas":["2026-03-15","{f}"]}}"#)).unwrap()
        };
        assert!(Regla::try_from(&por_fechas("2026-06-15")).is_ok(), "el control");
        assert!(matches!(
            Regla::try_from(&por_fechas("-262143-01-01")),
            Err(ErrorRegla::FechaFueraDeRango(..))
        ));
    }
}
