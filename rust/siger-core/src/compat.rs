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
}

/// Espejo exacto del diccionario que guarda el sistema actual. Todos los campos
/// son opcionales porque en la base lo son: eso es precisamente el problema que
/// la conversión resuelve.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReglaJson {
    #[serde(default)]
    pub modo: Option<String>,
    #[serde(default)]
    pub fecha_inicio: Option<String>,

    // Cíclico
    #[serde(default)]
    pub intervalo: Option<i64>,

    // Específico
    #[serde(default)]
    pub meses: Option<Vec<u32>>,
    #[serde(default)]
    pub dia_corte: Option<i64>,
    #[serde(default)]
    pub dias_corte: Option<Vec<i64>>,
    #[serde(default)]
    pub es_ultimo_dia: Option<bool>,
    #[serde(default)]
    pub dia_habil: Option<i64>,
    #[serde(default)]
    pub dia_semana_mes: Option<i64>,
    #[serde(default)]
    pub semana_del_mes: Option<i64>,
    #[serde(default)]
    pub cada_n_anios: Option<i64>,

    // Semanal
    #[serde(default)]
    pub dias_semana: Option<Vec<u32>>,
    #[serde(default)]
    pub dia_habil_semana: Option<i64>,
    #[serde(default)]
    pub cada_n_semanas: Option<i64>,

    // Horario
    #[serde(default)]
    pub hora_inicio: Option<i64>,
    #[serde(default)]
    pub hora_fin: Option<i64>,
    #[serde(default)]
    pub intervalo_horas: Option<i64>,

    // Fechas explícitas
    #[serde(default)]
    pub fechas: Option<Vec<String>>,

    // Ajuste. Dos parejas de banderas para lo mismo, porque el específico usaba
    // una y el cíclico la otra. Esa divergencia ya costó que el ajuste
    // anticipado no se aplicara nunca en modo semanal; aquí se leen las dos y se
    // colapsan en un único valor.
    #[serde(default)]
    pub ajuste_habil: Option<bool>,
    #[serde(default)]
    pub mover_habil: Option<bool>,
    #[serde(default)]
    pub ajuste_habil_anterior: Option<bool>,
    #[serde(default)]
    pub mover_habil_anterior: Option<bool>,
    #[serde(default)]
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
        // Una fecha de vigencia ilegible se trata como ausente, igual que antes:
        // la regla sigue siendo válida, simplemente no acota desde cuándo.
        let vigencia = j
            .fecha_inicio
            .as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok());

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
            "HOURLY" | "HORARIO" => Patron::Horario {
                hora_inicio: j.hora_inicio.unwrap_or(8).clamp(0, 23) as u32,
                hora_fin: j.hora_fin.unwrap_or(18).clamp(0, 23) as u32,
                intervalo_horas: j.intervalo_horas.unwrap_or(1).max(0) as u32,
            },
            "FECHAS" => Patron::Fechas(
                j.fechas
                    .clone()
                    .unwrap_or_default()
                    .iter()
                    // Una fecha ilegible no invalida las demás.
                    .filter_map(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok())
                    .collect(),
            ),
            otro => return Err(ErrorRegla::ModoDesconocido(otro.to_string())),
        };

        Ok(Regla { vigencia, patron, ajuste: j.ajuste() })
    }
}
