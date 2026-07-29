//! Del calendario a la regla: se marcan fechas y el sistema dice qué son.
//!
//! Port de `app/services/inferir_periodicidad.py`. La dirección es la inversa
//! del resto del motor: quien crea la obligación TIENE las fechas —están en la
//! norma— y lo que no tiene es el nombre de la abstracción. Aquí marca las
//! fechas y el sistema responde «esto es MENSUAL, el 5º día hábil de cada mes».
//!
//! # La regla que lo hace fiable
//!
//! Cada patrón candidato se **ejecuta contra el motor** y se compara con lo
//! marcado. Si no reproduce EXACTAMENTE esas fechas, se descarta. Nada de «se
//! parece a mensual»: o las genera todas y ninguna de más, o no es. Un patrón
//! mal inferido es peor que ninguno, porque el usuario lo da por bueno.
//!
//! Cuando no hay patrón se devuelve la lista literal (`FECHAS`), que reproduce
//! lo marcado por construcción: ninguna obligación se queda sin registrar.
//!
//! La conformidad con el Python se mide en `tests/oraculo_inferencia.rs` contra
//! el corpus dorado que produjo la implementación anterior.

use std::collections::BTreeSet;

use chrono::{Datelike, Duration, NaiveDate};

use crate::calendario::{dias_del_mes, Calendario};
use crate::compat::ReglaJson;
use crate::recurrencia::{Motor, Regla};

const MESES_ES: [&str; 13] = [
    "", "enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto",
    "septiembre", "octubre", "noviembre", "diciembre",
];
const DIAS: [&str; 7] = [
    "lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo",
];

/// Cómo se llama una periodicidad que el catálogo no nombra. No es un error: es
/// una obligación bien definida a la que ningún nombre del catálogo le queda.
const NOMBRE_SIN_CATALOGO: &str = "Periodicidad propia";

/// El resultado de inferir: la regla y cómo se le dice al usuario.
#[derive(Debug, Clone)]
pub struct Inferencia {
    /// Clave del catálogo, o `None` si ninguna la nombra sin mentir.
    pub periodicidad: Option<String>,
    /// La regla lista para guardar, en la forma que ya usa el sistema.
    pub regla: ReglaJson,
    /// Cómo se le dice al usuario lo que acaba de definir.
    pub explicacion: String,
    /// Si la regla reproduce EXACTAMENTE las fechas marcadas.
    pub exacta: bool,
    /// Patrones que se probaron y no reprodujeron lo marcado.
    pub descartados: Vec<String>,
}

/// El nombre del catálogo para una clave, o «Periodicidad propia» si no la hay.
fn nombre_de(clave: Option<&str>) -> String {
    match clave {
        Some("DIARIA") => "Diaria",
        Some("VARIAS_AL_DIA") => "Varias veces al día",
        Some("SEMANAL") => "Semanal",
        Some("CADA_N_SEMANAS") => "Cada varias semanas",
        Some("MENSUAL") => "Mensual",
        Some("QUINCENAL") => "Quincenal",
        Some("BIMESTRAL") => "Bimestral",
        Some("TRIMESTRAL") => "Trimestral",
        Some("SEMESTRAL") => "Semestral",
        Some("ANUAL") => "Anual",
        Some("PLURIANUAL") => "Cada varios años",
        Some("CADA_N_DIAS") => "Cada cierto número de días",
        Some("SIN_PATRON") => "Fechas sueltas, sin patrón",
        _ => NOMBRE_SIN_CATALOGO,
    }
    .to_string()
}

/// Cada cuántos meses se repite, si es regular. `None` si no lo es. Julio y
/// agosto son dos meses y NO son un semestre: lo que hace semestral a una
/// obligación es que sus meses estén a SEIS de distancia, no que sean dos.
fn separacion_regular(meses: &[u32]) -> Option<u32> {
    if meses.len() < 2 {
        return None;
    }
    let mut extendido: Vec<i64> = meses.iter().map(|&m| m as i64).collect();
    extendido.push(meses[0] as i64 + 12);
    let saltos: BTreeSet<i64> = extendido
        .windows(2)
        .map(|w| (w[1] - w[0]).rem_euclid(12))
        .collect();
    if saltos.len() == 1 {
        Some(*saltos.iter().next().unwrap() as u32)
    } else {
        None
    }
}

/// La clave del catálogo, o `None` si ninguna dice la verdad. `dias_por_mes`
/// importa tanto como los meses: dos días en cada mes NO es mensual —son
/// veinticuatro al año—, es quincenal.
fn periodicidad_de(meses: &[u32], dias_por_mes: u32) -> Option<&'static str> {
    if dias_por_mes > 1 {
        return if meses.len() == 12 && dias_por_mes == 2 {
            Some("QUINCENAL")
        } else {
            None
        };
    }
    if meses.len() == 12 {
        return Some("MENSUAL");
    }
    if meses.len() == 1 {
        return Some("ANUAL");
    }
    match separacion_regular(meses) {
        Some(6) => Some("SEMESTRAL"),
        Some(3) => Some("TRIMESTRAL"),
        Some(2) => Some("BIMESTRAL"),
        Some(1) => Some("MENSUAL"),
        _ => None,
    }
}

/// El nombre de uno o varios días de la semana: «lunes», «lunes y miércoles».
fn dias_es(indices: &[u32]) -> String {
    let nombres: Vec<&str> = indices.iter().map(|&i| DIAS[i as usize]).collect();
    match nombres.len() {
        0 => String::new(),
        1 => nombres[0].to_string(),
        n => format!("{} y {}", nombres[..n - 1].join(", "), nombres[n - 1]),
    }
}

/// (ordinal, ordinal desde el final) de ese día de la semana dentro del mes.
fn ordinal_en_el_mes(f: NaiveDate) -> (i32, i32) {
    let objetivo = f.weekday().num_days_from_monday();
    let mismos: Vec<u32> = (1..=dias_del_mes(f.year(), f.month()))
        .filter(|&d| {
            NaiveDate::from_ymd_opt(f.year(), f.month(), d)
                .unwrap()
                .weekday()
                .num_days_from_monday()
                == objetivo
        })
        .collect();
    let idx = mismos.iter().position(|&d| d == f.day()).unwrap() as i32;
    (idx + 1, idx - mismos.len() as i32)
}

/// En qué puesto de días hábiles del mes cae `f`. `None` si no es hábil.
fn n_esimo_habil_del_mes(f: NaiveDate, cal: &Calendario) -> Option<u32> {
    if !cal.es_habil(f) {
        return None;
    }
    Some(
        (1..=f.day())
            .filter(|&d| cal.es_habil(NaiveDate::from_ymd_opt(f.year(), f.month(), d).unwrap()))
            .count() as u32,
    )
}

/// Un `ReglaJson` con solo la vigencia puesta, para ir añadiéndole campos.
fn base(inicio: NaiveDate) -> ReglaJson {
    ReglaJson {
        fecha_inicio: Some(inicio.format("%Y-%m-%d").to_string()),
        ..Default::default()
    }
}

fn especifico(inicio: NaiveDate, meses: &[u32]) -> ReglaJson {
    ReglaJson {
        modo: Some("ESPECIFICO".to_string()),
        meses: Some(meses.to_vec()),
        ..base(inicio)
    }
}

/// Los días nominales que la regla produce en `[desde, hasta]`, corriendo el
/// motor real. Es el `_nominales` de Python: la única forma honesta de saber si
/// un candidato reproduce lo marcado es ejecutarlo.
fn nominales(motor: &Motor, rj: &ReglaJson, desde: NaiveDate, hasta: NaiveDate) -> BTreeSet<NaiveDate> {
    match Regla::try_from(rj) {
        Ok(regla) => motor
            .vencimientos_entre(desde, hasta, &regla)
            .iter()
            .map(|v| v.nominal.fecha())
            .collect(),
        Err(_) => BTreeSet::new(),
    }
}

/// Reglas plausibles, de la más específica a la más genérica. El orden importa:
/// «el último martes» y «el día 26» pueden coincidir un mes, y se prefiere la
/// que EXPLICA el patrón sobre la coincidencia numérica.
fn candidatos(
    fechas: &[NaiveDate],
    cal: &Calendario,
) -> Vec<(Option<String>, ReglaJson, String)> {
    let inicio = fechas[0];
    let meses: Vec<u32> = {
        let s: BTreeSet<u32> = fechas.iter().map(|f| f.month()).collect();
        s.into_iter().collect()
    };
    let dias: Vec<u32> = {
        let s: BTreeSet<u32> = fechas.iter().map(|f| f.day()).collect();
        s.into_iter().collect()
    };
    let semanas: Vec<u32> = {
        let s: BTreeSet<u32> = fechas.iter().map(|f| f.weekday().num_days_from_monday()).collect();
        s.into_iter().collect()
    };
    let clave = |c: Option<&str>| c.map(|s| s.to_string());
    let mut salida: Vec<(Option<String>, ReglaJson, String)> = Vec::new();

    // --- patrones de día de la semana ---
    if fechas.len() >= 3 {
        let seps: BTreeSet<i64> = fechas.windows(2).map(|w| (w[1] - w[0]).num_days()).collect();
        let semanal = |cada_n: Option<u32>| ReglaJson {
            modo: Some("SEMANAL".to_string()),
            dias_semana: Some(semanas.clone()),
            cada_n_semanas: cada_n.map(|n| n as i64),
            ..base(inicio)
        };
        if !seps.is_empty() && seps.iter().all(|&d| d == 7) {
            salida.push((
                clave(Some("SEMANAL")),
                semanal(None),
                format!("todas las semanas, {}", dias_es(&semanas)),
            ));
        }
        for n in [2u32, 3, 4] {
            if !seps.is_empty() && seps.iter().all(|&d| d == 7 * n as i64) {
                salida.push((
                    clave(Some("CADA_N_SEMANAS")),
                    semanal(Some(n)),
                    format!("cada {} semanas, {}", n, dias_es(&semanas)),
                ));
            }
        }
        if semanas.len() > 1 && !seps.is_empty() && *seps.iter().max().unwrap() <= 7 {
            salida.push((
                clave(Some("SEMANAL")),
                semanal(None),
                format!("todas las semanas, {}", dias_es(&semanas)),
            ));
        }
    }

    // Los patrones DERIVADOS necesitan varias observaciones: con una sola fecha
    // casi cualquiera la explica, y sería una invención. Con pocos datos se
    // prefiere la lectura LITERAL, que es lo que el usuario marcó.
    let suficientes = {
        let s: BTreeSet<(i32, u32)> = fechas.iter().map(|f| (f.year(), f.month())).collect();
        s.len() >= 3
    };

    // n-ésimo día hábil
    let habiles: BTreeSet<Option<u32>> = fechas.iter().map(|&f| n_esimo_habil_del_mes(f, cal)).collect();
    if suficientes && habiles.len() == 1 && !habiles.contains(&None) {
        let n = habiles.iter().next().unwrap().unwrap();
        salida.push((
            clave(periodicidad_de(&meses, 1)),
            ReglaJson { dia_habil: Some(n as i64), ..especifico(inicio, &meses) },
            format!("el {}º día hábil de cada mes", n),
        ));
    }

    // último / n-ésimo día de la semana del mes
    let ordinales: BTreeSet<i32> = fechas.iter().map(|&f| ordinal_en_el_mes(f).0).collect();
    let desde_el_final: BTreeSet<i32> = fechas.iter().map(|&f| ordinal_en_el_mes(f).1).collect();
    if suficientes && semanas.len() == 1 && desde_el_final.len() == 1 && desde_el_final.contains(&-1) {
        salida.push((
            clave(periodicidad_de(&meses, 1)),
            ReglaJson {
                dia_semana_mes: Some(semanas[0] as i64),
                semana_del_mes: Some(-1),
                ..especifico(inicio, &meses)
            },
            format!("el último {} de cada mes", dias_es(&semanas)),
        ));
    } else if suficientes && semanas.len() == 1 && ordinales.len() == 1 {
        let n = *ordinales.iter().next().unwrap();
        salida.push((
            clave(periodicidad_de(&meses, 1)),
            ReglaJson {
                dia_semana_mes: Some(semanas[0] as i64),
                semana_del_mes: Some(n as i64),
                ..especifico(inicio, &meses)
            },
            format!("el {}º {} de cada mes", n, dias_es(&semanas)),
        ));
    }

    // último día del mes
    if suficientes && fechas.iter().all(|f| f.day() == dias_del_mes(f.year(), f.month())) {
        salida.push((
            clave(periodicidad_de(&meses, 1)),
            ReglaJson { es_ultimo_dia: Some(true), ..especifico(inicio, &meses) },
            "el último día de cada mes".to_string(),
        ));
    }

    // día(s) corrido(s)
    if dias.len() == 1 {
        salida.push((
            clave(periodicidad_de(&meses, 1)),
            ReglaJson { dia_corte: Some(dias[0] as i64), ..especifico(inicio, &meses) },
            format!("el día {}", dias[0]),
        ));
    } else if dias.len() <= 4 {
        salida.push((
            clave(periodicidad_de(&meses, dias.len() as u32)),
            ReglaJson {
                dias_corte: Some(dias.iter().map(|&d| d as i64).collect()),
                ..especifico(inicio, &meses)
            },
            format!(
                "los días {}",
                dias.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", ")
            ),
        ));
    }

    // ciclo por días corridos
    if fechas.len() >= 3 {
        let seps: BTreeSet<i64> = fechas.windows(2).map(|w| (w[1] - w[0]).num_days()).collect();
        if seps.len() == 1 {
            let n = *seps.iter().next().unwrap();
            salida.push((
                clave(Some("CADA_N_DIAS")),
                ReglaJson { modo: Some("CICLICO".to_string()), intervalo: Some(n), ..base(inicio) },
                format!("cada {} días", n),
            ));
        }
    }

    salida
}

/// Qué periodicidad describe EXACTAMENTE las fechas marcadas. Si ninguna lo
/// hace, la lista literal. Nunca un patrón aproximado.
pub fn inferir(fechas: &[NaiveDate], motor: &Motor) -> Inferencia {
    let mut fs: Vec<NaiveDate> = fechas.to_vec();
    fs.sort();
    fs.dedup();

    if fs.is_empty() {
        return Inferencia {
            periodicidad: Some("SIN_PATRON".to_string()),
            regla: ReglaJson { modo: Some("FECHAS".to_string()), fechas: Some(vec![]), ..Default::default() },
            explicacion: "no se marcó ninguna fecha".to_string(),
            exacta: false,
            descartados: vec![],
        };
    }

    let marcadas: BTreeSet<NaiveDate> = fs.iter().copied().collect();
    let (inicio, fin) = (fs[0], *fs.last().unwrap());
    let mut descartados: Vec<String> = Vec::new();

    for (clave, regla, texto) in candidatos(&fs, motor.calendario()) {
        if nominales(motor, &regla, inicio, fin) == marcadas {
            return Inferencia {
                explicacion: format!("{}: {}", nombre_de(clave.as_deref()), texto),
                periodicidad: clave,
                regla,
                exacta: true,
                descartados,
            };
        }
        descartados.push(texto);
    }

    Inferencia {
        periodicidad: Some("SIN_PATRON".to_string()),
        regla: ReglaJson {
            modo: Some("FECHAS".to_string()),
            fechas: Some(fs.iter().map(|f| f.format("%Y-%m-%d").to_string()).collect()),
            fecha_inicio: Some(inicio.format("%Y-%m-%d").to_string()),
            ..Default::default()
        },
        explicacion: format!("no siguen ningún patrón: quedan como {} fecha(s) sueltas", fs.len()),
        exacta: true,
        descartados,
    }
}

// ===========================================================================
// interpretaciones: qué PUEDE ser lo que el usuario acaba de marcar
// ===========================================================================
//
// `inferir` responde «qué patrón es este» y necesita muchas fechas. Al marcar en
// un calendario la pregunta de verdad es otra: «qué PUEDE ser esto». Marcar el
// lunes 6 de julio no determina nada, pero acota: diario, todos los lunes, el 6
// de cada mes, el primer lunes… Son obligaciones distintas y la selección es
// idéntica en todas. La lista no es un menú que estorba: es la pregunta que la
// selección deja abierta, y que solo el usuario puede contestar.

/// La etiqueta EXACTA de una selección por meses y días. La etiqueta es el
/// significado, así que se construye igual que en el Python.
fn como_se_lee(meses: &[u32], dias: &[u32]) -> String {
    let que_dia = if dias.len() == 1 {
        format!("el día {}", dias[0])
    } else {
        format!(
            "los días {}",
            dias.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", ")
        )
    };
    if meses.len() == 1 {
        let de_mes = MESES_ES[meses[0] as usize];
        return if dias.len() == 1 {
            format!("cada {} de {}", dias[0], de_mes)
        } else {
            format!("cada año, en {}, {}", de_mes, que_dia)
        };
    }
    let cadencia = match separacion_regular(meses) {
        Some(6) => Some("cada semestre"),
        Some(3) => Some("cada trimestre"),
        Some(2) => Some("cada dos meses"),
        Some(1) => Some("cada mes"),
        _ => None,
    };
    if let Some(c) = cadencia {
        return format!("{}, {}", c, que_dia);
    }
    // Meses irregulares: se nombran, en vez de inventarles una cadencia.
    let nombres: Vec<&str> = meses.iter().map(|&m| MESES_ES[m as usize]).collect();
    let lista = format!("{} y {}", nombres[..nombres.len() - 1].join(", "), nombres[nombres.len() - 1]);
    format!("en {}, {}", lista, que_dia)
}

/// Una interpretación posible de lo que se marcó.
#[derive(Debug, Clone)]
pub struct Lectura {
    /// Clave del catálogo, o `None` cuando ninguna la nombra sin mentir.
    pub periodicidad: Option<String>,
    pub nombre: String,
    /// Qué significa, en los términos de quien crea la obligación.
    pub dice: String,
    pub regla: ReglaJson,
    /// Las primeras fechas que produciría, para verlo antes de decidir.
    pub proximas: Vec<NaiveDate>,
}

/// Las primeras `cuantas` fechas nominales que la regla produce desde `desde`.
fn proximas(motor: &Motor, rj: &ReglaJson, desde: NaiveDate, cuantas: usize) -> Vec<NaiveDate> {
    let regla = match Regla::try_from(rj) {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let mut out = Vec::new();
    let tope = desde + Duration::days(800);
    let mut dia = desde;
    while dia <= tope && out.len() < cuantas {
        for v in motor.vencimientos_para(dia, &regla) {
            out.push(v.nominal.fecha());
            if out.len() >= cuantas {
                break;
            }
        }
        dia += Duration::days(1);
    }
    out
}

/// Qué puede ser lo que el usuario marcó. Solo lecturas que CONTIENEN la
/// selección: si una interpretación no generase alguna fecha marcada, no es una
/// lectura de lo que se marcó. Comprobado ejecutando, no razonado.
pub fn interpretaciones(seleccion: &[NaiveDate], motor: &Motor) -> Vec<Lectura> {
    let mut fechas = seleccion.to_vec();
    fechas.sort();
    fechas.dedup();
    if fechas.is_empty() {
        return vec![];
    }
    let (inicio, fin) = (fechas[0], *fechas.last().unwrap());
    let unicos = |f: &dyn Fn(&NaiveDate) -> u32| -> Vec<u32> {
        let s: BTreeSet<u32> = fechas.iter().map(f).collect();
        s.into_iter().collect()
    };
    let semanas = unicos(&|f| f.weekday().num_days_from_monday());
    let dias = unicos(&|f| f.day());
    let meses = unicos(&|f| f.month());
    let todos: Vec<u32> = (1..=12).collect();
    let clave = |c: Option<&str>| c.map(|s| s.to_string());
    let cal = motor.calendario();
    let lista_dias = |ds: &[u32]| ds.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", ");

    let mut candidatas: Vec<(Option<String>, String, ReglaJson)> = Vec::new();

    candidatas.push((
        clave(Some("DIARIA")),
        "todos los días desde esa fecha".to_string(),
        ReglaJson { modo: Some("CICLICO".into()), intervalo: Some(1), ..base(inicio) },
    ));
    candidatas.push((
        clave(Some("SEMANAL")),
        format!("todas las semanas, {}", dias_es(&semanas)),
        ReglaJson { modo: Some("SEMANAL".into()), dias_semana: Some(semanas.clone()), ..base(inicio) },
    ));
    candidatas.push((
        clave(Some("CADA_N_SEMANAS")),
        format!("cada dos semanas, {}", dias_es(&semanas)),
        ReglaJson {
            modo: Some("SEMANAL".into()),
            dias_semana: Some(semanas.clone()),
            cada_n_semanas: Some(2),
            ..base(inicio)
        },
    ));

    if dias.len() == 1 {
        candidatas.push((
            clave(Some("MENSUAL")),
            format!("el día {} de cada mes", dias[0]),
            ReglaJson { dia_corte: Some(dias[0] as i64), ..especifico(inicio, &todos) },
        ));
    } else {
        candidatas.push((
            clave(periodicidad_de(&todos, dias.len() as u32)),
            format!("los días {} de cada mes", lista_dias(&dias)),
            ReglaJson {
                dias_corte: Some(dias.iter().map(|&d| d as i64).collect()),
                ..especifico(inicio, &todos)
            },
        ));
    }

    if semanas.len() == 1 {
        let (ordinal, desde_final) = ordinal_en_el_mes(fechas[0]);
        candidatas.push((
            clave(Some("MENSUAL")),
            format!("el {}º {} de cada mes", ordinal, dias_es(&semanas)),
            ReglaJson {
                dia_semana_mes: Some(semanas[0] as i64),
                semana_del_mes: Some(ordinal as i64),
                ..especifico(inicio, &todos)
            },
        ));
        if desde_final == -1 {
            candidatas.push((
                clave(Some("MENSUAL")),
                format!("el último {} de cada mes", dias_es(&semanas)),
                ReglaJson {
                    dia_semana_mes: Some(semanas[0] as i64),
                    semana_del_mes: Some(-1),
                    ..especifico(inicio, &todos)
                },
            ));
        }
        if let Some(habil) = n_esimo_habil_del_mes(fechas[0], cal) {
            candidatas.push((
                clave(Some("MENSUAL")),
                format!("el {}º día hábil de cada mes", habil),
                ReglaJson { dia_habil: Some(habil as i64), ..especifico(inicio, &todos) },
            ));
        }
    }

    if meses.len() < 12 {
        let rj = if dias.len() == 1 {
            ReglaJson { dia_corte: Some(dias[0] as i64), ..especifico(inicio, &meses) }
        } else {
            ReglaJson {
                dias_corte: Some(dias.iter().map(|&d| d as i64).collect()),
                ..especifico(inicio, &meses)
            }
        };
        candidatas.push((clave(periodicidad_de(&meses, dias.len() as u32)), como_se_lee(&meses, &dias), rj));
    }

    candidatas.push((
        clave(Some("SIN_PATRON")),
        if fechas.len() == 1 {
            "solo esa fecha".to_string()
        } else {
            format!("solo esas {} fechas", fechas.len())
        },
        ReglaJson {
            modo: Some("FECHAS".into()),
            fechas: Some(fechas.iter().map(|f| f.format("%Y-%m-%d").to_string()).collect()),
            ..base(inicio)
        },
    ));

    let mut lecturas = Vec::new();
    let mut vistas: BTreeSet<String> = BTreeSet::new();
    for (clave, dice, regla) in candidatas {
        // La lectura tiene que CONTENER lo marcado.
        let noms = nominales(motor, &regla, inicio, fin);
        if !fechas.iter().all(|f| noms.contains(f)) {
            continue;
        }
        if vistas.contains(&dice) {
            continue;
        }
        vistas.insert(dice.clone());
        let prox = proximas(motor, &regla, inicio, 4);
        lecturas.push(Lectura {
            nombre: nombre_de(clave.as_deref()),
            periodicidad: clave,
            dice,
            regla,
            proximas: prox,
        });
    }
    lecturas
}
