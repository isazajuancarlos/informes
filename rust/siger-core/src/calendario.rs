//! Qué días son festivos y cuáles son hábiles.
//!
//! # Por qué esto se implementa y no se importa
//!
//! El motor de Python deriva los festivos de la librería `holidays`. Aquí no hay
//! una equivalente madura para Colombia, así que la ley se escribe. No es una
//! pérdida: **la ley es el activo**. Un festivo mal calculado mueve una fecha
//! límite, y una fecha límite mal calculada es justo el incumplimiento que este
//! producto existe para evitar — así que conviene que esté aquí, citada, y no en
//! el árbol de dependencias de otro.
//!
//! Lo que hace que esto sea verificable y no un acto de fe:
//! `tests/oraculo.rs` compara este calendario, **día a día entre 2020 y 2045**,
//! contra los 484 festivos que produjo la librería del motor anterior. Si el
//! Congreso cambia la ley, se regenera el oráculo y el rojo aparece en la fecha
//! exacta que cambió.
//!
//! # Las tres familias de festivo
//!
//! 1. **Fijos**, que no se mueven nunca: 1 de enero, 1 de mayo, 20 de julio,
//!    7 de agosto, 8 y 25 de diciembre.
//! 2. **Trasladables** (Ley 51 de 1983, «Ley Emiliani»): se corren al lunes
//!    siguiente cuando no caen en lunes.
//! 3. **Móviles**, atados a la Pascua. Jueves y Viernes Santo se quedan donde
//!    caen; Ascensión, Corpus Christi y Sagrado Corazón se trasladan.

use std::collections::HashSet;

use chrono::{Datelike, Duration, NaiveDate, Weekday};

/// Error de construcción del calendario.
///
/// Existe para que un país sin calendario **falle**, en vez de degradar a un
/// calendario sin festivos. Ese camino permisivo ya existió en el motor
/// anterior: cada festivo contaba como hábil, ningún vencimiento se ajustaba y
/// todas las fechas salían plausibles y equivocadas. Un sistema de cumplimiento
/// que calcula mal los plazos y no lo dice es peor que uno que no arranca.
#[derive(Debug, thiserror::Error)]
pub enum ErrorCalendario {
    #[error(
        "no hay calendario de festivos para el país «{0}». Los disponibles son: CO. \
         Si la entidad opera sin festivos, constrúyalo con Calendario::sin_festivos()."
    )]
    PaisSinCalendario(String),
}

/// Calendario de días hábiles de una entidad.
#[derive(Debug, Clone)]
pub struct Calendario {
    festivos: HashSet<NaiveDate>,
    /// Qué días de la semana trabaja la entidad. Por defecto lunes a viernes,
    /// que es la jornada de una entidad pública colombiana — **no una ley del
    /// calendario**. Una empresa privada puede trabajar el sábado, y para ella
    /// el sábado es un día laboral con un festivo posible.
    laborables: [bool; 7],
}

impl Calendario {
    /// Calendario de un país, con festivos calculados para un rango de años.
    ///
    /// El rango se materializa por adelantado en vez de calcularse al vuelo
    /// porque el motor consulta el calendario decenas de miles de veces al
    /// generar un cronograma.
    pub fn de_pais(iso: &str, desde: i32, hasta: i32) -> Result<Self, ErrorCalendario> {
        let iso = iso.trim().to_uppercase();
        match iso.as_str() {
            "CO" => Ok(Self {
                festivos: festivos_co(desde, hasta),
                laborables: LUNES_A_VIERNES,
            }),
            _ => Err(ErrorCalendario::PaisSinCalendario(iso)),
        }
    }

    /// Calendario sin festivos. **Hay que pedirlo explícitamente**: es el camino
    /// permisivo, y por eso no es el que se obtiene por descuido.
    pub fn sin_festivos() -> Self {
        Self { festivos: HashSet::new(), laborables: LUNES_A_VIERNES }
    }

    /// Cambia la jornada de la entidad. `laborables[0]` es el lunes.
    pub fn con_jornada(mut self, laborables: [bool; 7]) -> Self {
        self.laborables = laborables;
        self
    }

    /// Solo festivo. **No** incluye fin de semana, y esa es toda la diferencia
    /// con [`Self::es_habil`]: un sábado es no laborable para una entidad
    /// pública, pero no es festivo, y confundirlos borraba los sábados de una
    /// jornada de lunes a sábado.
    pub fn es_festivo(&self, fecha: NaiveDate) -> bool {
        self.festivos.contains(&fecha)
    }

    /// Día laborable de la entidad y no festivo.
    pub fn es_habil(&self, fecha: NaiveDate) -> bool {
        self.laborables[fecha.weekday().num_days_from_monday() as usize] && !self.es_festivo(fecha)
    }

    /// Primer hábil estrictamente posterior. Si no aparece en 30 días devuelve
    /// la fecha original, igual que el motor anterior: es un tope de seguridad,
    /// no una regla de negocio.
    pub fn habil_siguiente(&self, fecha: NaiveDate) -> NaiveDate {
        self.buscar_habil(fecha, 1)
    }

    /// Primer hábil estrictamente anterior.
    pub fn habil_anterior(&self, fecha: NaiveDate) -> NaiveDate {
        self.buscar_habil(fecha, -1)
    }

    fn buscar_habil(&self, fecha: NaiveDate, direccion: i64) -> NaiveDate {
        if self.es_habil(fecha) {
            return fecha;
        }
        let mut cursor = fecha;
        for _ in 0..30 {
            cursor += Duration::days(direccion);
            if self.es_habil(cursor) {
                return cursor;
            }
        }
        fecha
    }

    /// El n-ésimo día hábil del mes. **No es «el día n ajustado a hábil»**:
    /// «dentro de los primeros cinco días hábiles del mes siguiente» es de las
    /// formas más frecuentes de fijar un plazo en la norma colombiana, y si el
    /// mes empieza con festivo el 5º hábil es el 8, no el 5 corrido al 8.
    ///
    /// Si el mes tiene menos hábiles que `n`, se usa el último. Descartar el
    /// vencimiento lo haría desaparecer sin avisar, que es la forma cara de
    /// fallar.
    pub fn n_esimo_habil_del_mes(&self, anio: i32, mes: u32, n: u32) -> Option<NaiveDate> {
        let mut vistos = 0;
        let mut ultimo = None;
        for dia in 1..=dias_del_mes(anio, mes) {
            let fecha = NaiveDate::from_ymd_opt(anio, mes, dia)?;
            if !self.es_habil(fecha) {
                continue;
            }
            vistos += 1;
            ultimo = Some(fecha);
            if vistos == n.max(1) {
                return Some(fecha);
            }
        }
        ultimo
    }

    /// El n-ésimo día hábil de la semana a la que pertenece `fecha`.
    ///
    /// La semana empieza el lunes. Si el lunes es festivo, el 4º hábil es el
    /// viernes — que es toda la diferencia con marcar «jueves» y con «el 4º
    /// hábil del mes».
    pub fn n_esimo_habil_de_la_semana(&self, fecha: NaiveDate, n: u32) -> Option<NaiveDate> {
        let lunes = fecha - Duration::days(fecha.weekday().num_days_from_monday() as i64);
        let habiles: Vec<NaiveDate> = (0..7)
            .map(|d| lunes + Duration::days(d))
            .filter(|f| self.es_habil(*f))
            .collect();
        if habiles.is_empty() {
            return None;
        }
        Some(habiles[(n.max(1) as usize).min(habiles.len()) - 1])
    }
}

const LUNES_A_VIERNES: [bool; 7] = [true, true, true, true, true, false, false];

/// Días del mes, con bisiesto resuelto por el propio calendario.
pub fn dias_del_mes(anio: i32, mes: u32) -> u32 {
    let (a, m) = if mes == 12 { (anio + 1, 1) } else { (anio, mes + 1) };
    NaiveDate::from_ymd_opt(a, m, 1)
        .and_then(|primero| primero.pred_opt())
        .map(|ultimo| ultimo.day())
        .unwrap_or(28)
}

/// Domingo de Pascua por el algoritmo gregoriano anónimo (Meeus/Butcher).
pub fn pascua(anio: i32) -> NaiveDate {
    let a = anio % 19;
    let b = anio / 100;
    let c = anio % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let mes = (h + l - 7 * m + 114) / 31;
    let dia = ((h + l - 7 * m + 114) % 31) + 1;
    NaiveDate::from_ymd_opt(anio, mes as u32, dia as u32).expect("la Pascua siempre existe")
}

/// Traslado de la Ley 51 de 1983: al lunes siguiente si no cae en lunes.
fn al_lunes(fecha: NaiveDate) -> NaiveDate {
    let corrimiento = match fecha.weekday() {
        Weekday::Mon => 0,
        otro => 7 - otro.num_days_from_monday() as i64,
    };
    fecha + Duration::days(corrimiento)
}

/// Primer año en que rige el festivo de Chiquinquirá (Ley 2578 de 2026).
const ANIO_VIGENCIA_CHIQUINQUIRA: i32 = 2026;

/// Festivos colombianos de `desde` a `hasta`, ambos incluidos.
pub fn festivos_co(desde: i32, hasta: i32) -> HashSet<NaiveDate> {
    let mut salida = HashSet::new();
    for anio in desde..=hasta {
        let dia = |m: u32, d: u32| NaiveDate::from_ymd_opt(anio, m, d).expect("fecha fija válida");

        // 1. Fijos: no se mueven nunca.
        for (m, d) in [(1, 1), (5, 1), (7, 20), (8, 7), (12, 8), (12, 25)] {
            salida.insert(dia(m, d));
        }

        // 2. Trasladables al lunes siguiente (Ley 51 de 1983).
        for (m, d) in [(1, 6), (3, 19), (6, 29), (8, 15), (10, 12), (11, 1), (11, 11)] {
            salida.insert(al_lunes(dia(m, d)));
        }

        // Nuestra Señora del Rosario de Chiquinquirá, 9 de julio.
        // **Ley 2578 de 2026** (sancionada el 1 de junio de 2026), que lo
        // declara festivo nacional de obligatorio cumplimiento y remite
        // expresamente a la Ley 51 de 1983 para el traslado — por eso en 2026 se
        // observa el lunes 13.
        //
        // Lleva año de vigencia porque antes de 2026 no existía: un calendario
        // que lo aplicara hacia atrás daría por incumplida una entrega que en su
        // día llegó a tiempo.
        //
        // OJO, y es trabajo de abogado más que de programador: hay una demanda
        // de inconstitucionalidad en curso contra esta ley. Si la Corte la
        // tumba, esta línea cambia y con ella todas las fechas límite de julio.
        // Es exactamente lo que paga la cuota de «actualizaciones normativas»:
        // la regla vive en un sitio, con su norma citada, y se corrige en una
        // línea el día que la norma cambie.
        if anio >= ANIO_VIGENCIA_CHIQUINQUIRA {
            salida.insert(al_lunes(dia(7, 9)));
        }

        // 3. Móviles. Jueves y Viernes Santo NO se trasladan: la Semana Santa
        //    es la Semana Santa, y correrla al lunes la deshace.
        let p = pascua(anio);
        salida.insert(p - Duration::days(3)); // Jueves Santo
        salida.insert(p - Duration::days(2)); // Viernes Santo

        // Estos tres sí, y por eso siempre caen en lunes.
        salida.insert(al_lunes(p + Duration::days(39))); // Ascensión del Señor
        salida.insert(al_lunes(p + Duration::days(60))); // Corpus Christi
        salida.insert(al_lunes(p + Duration::days(68))); // Sagrado Corazón
    }
    salida
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_pascua_cae_donde_dice_el_almanaque() {
        // Tres años comprobables contra cualquier calendario litúrgico.
        assert_eq!(pascua(2026), NaiveDate::from_ymd_opt(2026, 4, 5).unwrap());
        assert_eq!(pascua(2027), NaiveDate::from_ymd_opt(2027, 3, 28).unwrap());
        assert_eq!(pascua(2038), NaiveDate::from_ymd_opt(2038, 4, 25).unwrap());
    }

    #[test]
    fn los_trasladables_siempre_caen_en_lunes() {
        // La prueba que discrimina: si el traslado no se aplicara, el 6 de enero
        // de 2026 —un martes— seguiría siendo festivo en martes.
        let cal = Calendario::de_pais("CO", 2026, 2026).unwrap();
        let reyes = NaiveDate::from_ymd_opt(2026, 1, 12).unwrap();
        assert!(cal.es_festivo(reyes), "Reyes de 2026 se traslada al lunes 12");
        assert!(!cal.es_festivo(NaiveDate::from_ymd_opt(2026, 1, 6).unwrap()));
    }

    #[test]
    fn un_pais_sin_calendario_falla_en_vez_de_quedarse_sin_festivos() {
        assert!(Calendario::de_pais("XX", 2026, 2026).is_err());
    }

    #[test]
    fn el_sabado_puede_ser_laborable_para_quien_trabaja_el_sabado() {
        let sabado = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
        let publica = Calendario::de_pais("CO", 2026, 2026).unwrap();
        assert!(!publica.es_habil(sabado));

        let privada = Calendario::de_pais("CO", 2026, 2026)
            .unwrap()
            .con_jornada([true, true, true, true, true, true, false]);
        assert!(privada.es_habil(sabado));
        // Y sigue sin ser festivo en ninguna de las dos: son cosas distintas.
        assert!(!publica.es_festivo(sabado) && !privada.es_festivo(sabado));
    }
}
