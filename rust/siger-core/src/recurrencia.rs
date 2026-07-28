//! Qué días fija una obligación, y con qué plazo.
//!
//! # El cambio de forma respecto del motor anterior
//!
//! Antes la regla era un diccionario suelto: `{'modo': 'ESPECIFICO', 'meses':
//! [...], 'dia_corte': 15, 'es_ultimo_dia': False, 'dia_habil': None, ...}`.
//! Todos los campos coexistían siempre, así que era posible escribir «día de
//! corte 5 **y** último día del mes» —dos cosas contradictorias— y el motor
//! resolvía en silencio a favor de una. La obligación quedaba creada y no hacía
//! lo que su autor creía, que es la familia de defectos que este sistema existe
//! para impedir.
//!
//! Aquí cada modo lleva **solo sus campos**. La combinación imposible no se
//! rechaza en tiempo de ejecución: no se puede escribir.
//!
//! [`ReglaJson`] es la puerta de entrada desde lo que ya está guardado en
//! `cat_informes.recurrencia_json`. Traduce y valida una vez, al leer; a partir
//! de ahí el motor trabaja con la forma tipada.

use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::calendario::{dias_del_mes, Calendario};
use crate::fechas::{TipoAjuste, Vencimiento};

/// Qué se hace cuando el día nominal no es hábil. Es una decisión de la regla,
/// ortogonal al patrón: cualquier patrón puede llevar cualquier ajuste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Ajuste {
    /// El día nominal es el límite, sea hábil o no.
    #[default]
    Ninguno,
    /// Se admite entregar hasta el hábil siguiente.
    Gracia,
    /// Se entrega el hábil anterior.
    Anticipado,
    /// Si el nominal es festivo, no hay vencimiento.
    OmitirFestivos,
}

/// El patrón que fija los días nominales.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Patron {
    /// Cada N días desde la vigencia.
    Ciclico { intervalo: u32 },
    /// Días concretos de meses concretos.
    Especifico(Especifico),
    /// Días de la semana, o el n-ésimo hábil de la semana.
    Semanal(Semanal),
    /// Varias veces dentro del mismo día.
    Horario { hora_inicio: u32, hora_fin: u32, intervalo_horas: u32 },
    /// Lista literal. La salida de emergencia del diseño: una obligación que no
    /// sigue ningún patrón se enumera, y así nadie se queda fuera por no encajar
    /// en una forma que se nos ocurrió.
    Fechas(Vec<NaiveDate>),
}

/// El día del mes se fija de **una** de estas maneras, nunca de dos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiaDelMes {
    /// Día corrido. Si el mes no lo tiene (31 en un mes de 30), se usa el
    /// último: descartar el vencimiento lo haría desaparecer sin avisar.
    Corte(u32),
    /// Varios días corridos en el mismo mes — una quincenal, por ejemplo, que
    /// el motor anterior no podía expresar porque el día era un escalar.
    Cortes(Vec<u32>),
    /// El último día del mes, sea el 28, el 30 o el 31.
    Ultimo,
    /// El n-ésimo día **hábil**. No es «el día n ajustado a hábil»: uno de cada
    /// ocho informes de una base real de 399 usa esta forma.
    Habil(u32),
    /// «El último martes», «el primer lunes»: la forma en que se fijan comités y
    /// juntas. `ordinal` negativo cuenta desde el final.
    OrdinalDeSemana { dia_semana: u32, ordinal: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Especifico {
    /// Meses en los que hay obligación (1-12).
    pub meses: Vec<u32>,
    pub dia: DiaDelMes,
    /// Cada cuántos años. Sin esto, «cada dos años el 15 de marzo» no se podía
    /// expresar: con periodicidad cíclica tampoco, porque 730 días no son dos
    /// años y el desfase se acumula.
    pub cada_n_anios: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Semanal {
    /// Días de la semana marcados; 0 es lunes.
    pub dias: Vec<u32>,
    /// El n-ésimo día hábil **de la semana**. Excluyente con `dias`: si el lunes
    /// es festivo, el 4º hábil es el viernes, y eso no es «marcar el jueves».
    pub dia_habil: Option<u32>,
    /// «Cada dos martes». El día se elige y el sistema cuenta las semanas desde
    /// la primera ocurrencia real — no desde la semana de la vigencia, que se
    /// saltaba la primera entrega.
    pub cada_n_semanas: Option<u32>,
}

/// Una regla completa: desde cuándo rige, qué días fija y con qué plazo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Regla {
    pub vigencia: Option<NaiveDate>,
    pub patron: Patron,
    pub ajuste: Ajuste,
}

/// El motor. No hace E/S ni sabe nada de la base de datos: recibe una fecha y
/// una regla, y responde qué vencimientos fija esa regla **ese día**.
///
/// La pregunta es «¿la regla fija un vencimiento para hoy?», no «¿cae hoy algún
/// vencimiento ya ajustado?». La segunda obligaba a barrer meses y semanas
/// vecinas, porque el ajuste puede mover el límite a otro mes — el nominal, no.
pub struct Motor {
    calendario: Calendario,
}

impl Motor {
    pub fn nuevo(calendario: Calendario) -> Self {
        Self { calendario }
    }

    pub fn calendario(&self) -> &Calendario {
        &self.calendario
    }

    /// Vencimientos cuyo día **nominal** es `fecha`.
    pub fn vencimientos_para(&self, fecha: NaiveDate, regla: &Regla) -> Vec<Vencimiento> {
        // La vigencia se comprueba contra el nominal. Contra el límite borraba
        // obligaciones reales: un anual del 1 de enero con ajuste anticipado
        // tiene su límite el 31 de diciembre anterior y desaparecía entero.
        if let Some(desde) = regla.vigencia {
            if fecha < desde {
                return Vec::new();
            }
        }

        match &regla.patron {
            Patron::Ciclico { intervalo } => self.ciclico(fecha, regla, *intervalo),
            Patron::Especifico(e) => self.especifico(fecha, regla, e),
            Patron::Semanal(s) => self.semanal(fecha, regla, s),
            Patron::Fechas(fechas) => {
                if fechas.contains(&fecha) {
                    self.con_ajuste(fecha, regla.ajuste).into_iter().collect()
                } else {
                    Vec::new()
                }
            }
            Patron::Horario { hora_inicio, hora_fin, intervalo_horas } => {
                self.horario(fecha, *hora_inicio, *hora_fin, *intervalo_horas)
            }
        }
    }

    /// Todos los vencimientos de un rango. El barrido va por día nominal, así
    /// que un ajuste que empuje el límite fuera del rango no pierde nada.
    pub fn vencimientos_entre(
        &self,
        desde: NaiveDate,
        hasta: NaiveDate,
        regla: &Regla,
    ) -> Vec<Vencimiento> {
        let mut salida = Vec::new();
        let mut dia = desde;
        while dia <= hasta {
            salida.extend(self.vencimientos_para(dia, regla));
            dia += Duration::days(1);
        }
        salida
    }

    fn ciclico(&self, fecha: NaiveDate, regla: &Regla, intervalo: u32) -> Vec<Vencimiento> {
        let Some(inicio) = regla.vigencia else {
            // Sin vigencia no hay desde dónde contar. No se inventa un origen:
            // una regla cíclica sin fecha de inicio no fija ningún día.
            return Vec::new();
        };
        let diff = (fecha - inicio).num_days();
        let toca = (intervalo == 0 && diff == 0) || (intervalo > 0 && diff % intervalo as i64 == 0);
        if toca {
            self.con_ajuste(fecha, regla.ajuste).into_iter().collect()
        } else {
            Vec::new()
        }
    }

    fn especifico(&self, fecha: NaiveDate, regla: &Regla, e: &Especifico) -> Vec<Vencimiento> {
        if let Some(cada) = e.cada_n_anios {
            let base = regla.vigencia.map(|v| v.year()).unwrap_or(fecha.year());
            if cada > 1 && (fecha.year() - base).rem_euclid(cada as i32) != 0 {
                return Vec::new();
            }
        }
        if !e.meses.contains(&fecha.month()) {
            return Vec::new();
        }
        // Un vencimiento por día, aunque dos reglas del mismo mes coincidan.
        for nominal in self.nominales_del_mes(fecha.year(), fecha.month(), &e.dia) {
            if nominal == fecha {
                return self.con_ajuste(nominal, regla.ajuste).into_iter().collect();
            }
        }
        Vec::new()
    }

    fn semanal(&self, fecha: NaiveDate, regla: &Regla, s: &Semanal) -> Vec<Vencimiento> {
        if let Some(cada) = s.cada_n_semanas {
            if cada > 1 {
                let Some(inicio) = regla.vigencia else { return Vec::new() };
                let Some(ancla) = self.primera_ocurrencia_semanal(inicio, s) else {
                    return Vec::new();
                };
                let lunes_de = |f: NaiveDate| {
                    f - Duration::days(f.weekday().num_days_from_monday() as i64)
                };
                let semanas = (lunes_de(fecha) - lunes_de(ancla)).num_days() / 7;
                if semanas < 0 || semanas % cada as i64 != 0 {
                    return Vec::new();
                }
            }
        }

        if let Some(n) = s.dia_habil {
            // Ya es hábil por construcción: no hay ajuste que aplicar.
            if self.calendario.n_esimo_habil_de_la_semana(fecha, n) == Some(fecha) {
                return vec![Vencimiento::sin_ajuste(fecha)];
            }
            return Vec::new();
        }

        if s.dias.contains(&fecha.weekday().num_days_from_monday()) {
            return self.con_ajuste(fecha, regla.ajuste).into_iter().collect();
        }
        Vec::new()
    }

    fn horario(
        &self,
        fecha: NaiveDate,
        inicio: u32,
        fin: u32,
        intervalo: u32,
    ) -> Vec<Vencimiento> {
        if intervalo == 0 {
            return Vec::new();
        }
        // La franja puede cruzar la medianoche: de 18 a 8 es un turno nocturno,
        // no un error. Se recorre hasta `fin + 24` y se toma el resto, para que
        // «cada dos horas» siga siendo cada dos horas al pasar de las 23 a las 0.
        let final_ = if fin >= inicio { fin } else { fin + 24 };
        let mut salida = Vec::new();
        let mut cursor = inicio;
        while cursor <= final_ {
            salida.push(Vencimiento::sin_ajuste(fecha).con_hora(cursor % 24));
            cursor += intervalo;
        }
        salida
    }

    /// Envuelve un día nominal con su día límite. `None` cuando el ajuste dice
    /// que ese día no hay obligación.
    fn con_ajuste(&self, nominal: NaiveDate, ajuste: Ajuste) -> Option<Vencimiento> {
        match ajuste {
            Ajuste::Gracia => Some(Vencimiento::nuevo(
                nominal,
                self.calendario.habil_siguiente(nominal),
                TipoAjuste::Gracia,
            )),
            Ajuste::OmitirFestivos => {
                // Se mira `es_festivo`, no `es_habil`: los días que la entidad
                // trabaja ya los declaró su jornada. Mirar `es_habil` borraba
                // todos los sábados de una jornada de lunes a sábado.
                if self.calendario.es_festivo(nominal) {
                    None
                } else {
                    Some(Vencimiento::nuevo(nominal, nominal, TipoAjuste::Omitir))
                }
            }
            Ajuste::Anticipado => Some(Vencimiento::nuevo(
                nominal,
                self.calendario.habil_anterior(nominal),
                TipoAjuste::Anticipado,
            )),
            Ajuste::Ninguno => Some(Vencimiento::sin_ajuste(nominal)),
        }
    }

    /// Los días nominales que la regla fija en ese mes. Sin ajustar: el nominal
    /// es el día que fija la regla, y el ajuste se calcula aparte. Cuando esta
    /// función devolvía la fecha ya ajustada, el mes al que «pertenecía» un
    /// vencimiento dependía del calendario de festivos.
    fn nominales_del_mes(&self, anio: i32, mes: u32, dia: &DiaDelMes) -> Vec<NaiveDate> {
        let ultimo = |d: u32| NaiveDate::from_ymd_opt(anio, mes, d.min(dias_del_mes(anio, mes)));
        match dia {
            DiaDelMes::Cortes(dias) => {
                let mut salida: Vec<NaiveDate> = dias
                    .iter()
                    .filter_map(|d| self.dia_corte(anio, mes, *d))
                    .collect();
                salida.sort_unstable();
                salida.dedup();
                salida
            }
            DiaDelMes::Corte(d) => self.dia_corte(anio, mes, *d).into_iter().collect(),
            DiaDelMes::Ultimo => ultimo(31).into_iter().collect(),
            DiaDelMes::Habil(n) => self
                .calendario
                .n_esimo_habil_del_mes(anio, mes, *n)
                .into_iter()
                .collect(),
            DiaDelMes::OrdinalDeSemana { dia_semana, ordinal } => {
                self.ordinal_de_semana(anio, mes, *dia_semana, *ordinal).into_iter().collect()
            }
        }
    }

    /// Día corrido del mes. Si el mes no lo tiene, el último día real.
    fn dia_corte(&self, anio: i32, mes: u32, dia: u32) -> Option<NaiveDate> {
        let tope = dias_del_mes(anio, mes);
        NaiveDate::from_ymd_opt(anio, mes, if dia >= 1 && dia <= tope { dia } else { tope })
    }

    /// El n-ésimo `dia_semana` del mes; `ordinal` negativo es el último.
    ///
    /// «El último martes» no es «el martes de la última semana»: un mes puede
    /// tener cuatro o cinco martes y la última semana puede no tenerlo.
    fn ordinal_de_semana(
        &self,
        anio: i32,
        mes: u32,
        dia_semana: u32,
        ordinal: i32,
    ) -> Option<NaiveDate> {
        let candidatos: Vec<NaiveDate> = (1..=dias_del_mes(anio, mes))
            .filter_map(|d| NaiveDate::from_ymd_opt(anio, mes, d))
            .filter(|f| f.weekday().num_days_from_monday() == dia_semana)
            .collect();
        if candidatos.is_empty() {
            return None;
        }
        if ordinal < 0 {
            return candidatos.last().copied();
        }
        // Si se pide el quinto y solo hay cuatro, se usa el último: mismo
        // criterio que el día 31 en un mes de 30.
        Some(candidatos[(ordinal.max(1) as usize).min(candidatos.len()) - 1])
    }

    /// El primer día, a partir de la vigencia, que la regla semanal marca. Es el
    /// ancla de «cada N semanas»: anclar en la semana de la vigencia se saltaba
    /// la primera entrega cuando la vigencia caía después del día marcado.
    fn primera_ocurrencia_semanal(&self, inicio: NaiveDate, s: &Semanal) -> Option<NaiveDate> {
        for salto in 0..14 {
            let candidato = inicio + Duration::days(salto);
            match s.dia_habil {
                Some(n) => {
                    if self.calendario.n_esimo_habil_de_la_semana(candidato, n) == Some(candidato) {
                        return Some(candidato);
                    }
                }
                None => {
                    if s.dias.contains(&candidato.weekday().num_days_from_monday()) {
                        return Some(candidato);
                    }
                }
            }
        }
        None
    }
}
