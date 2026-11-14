//! Oráculo diferencial de la inferencia de periodicidad.
//!
//! `inferir` no se «revisa»: se MIDE contra la implementación anterior. El corpus
//! `oraculo_inferencia.json` lo produjo la `inferir` de Python sobre reglas
//! conocidas (round-trip: regla → sus fechas por el motor → inferir debe
//! recuperarla) y sobre los casos límite que los docstrings subrayan. Si el port
//! Rust no reproduce esa salida EXACTAMENTE —periodicidad, explicación, la regla
//! y los descartados—, falla nombrando el caso.
//!
//! La regla se compara vía [`Regla`], no como JSON crudo: dos diccionarios que
//! difieren solo en campos ausentes vs. `null` describen la MISMA regla, y lo que
//! importa es la regla, no su serialización.

use chrono::NaiveDate;
use serde::Deserialize;
use siger_core::calendario::Calendario;
use siger_core::compat::ReglaJson;
use siger_core::inferencia::{inferir, interpretaciones};
use siger_core::recurrencia::{Motor, Regla};

#[derive(Deserialize)]
struct Caso {
    caso: String,
    fechas: Vec<String>,
    esperado: Esperado,
}

#[derive(Deserialize)]
struct Esperado {
    periodicidad: Option<String>,
    explicacion: String,
    exacta: bool,
    regla: ReglaJson,
    descartados: Vec<String>,
}

fn fecha(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("fecha del corpus")
}

/// La regla, normalizada a su forma tipada. `None` si ni siquiera parsea (no
/// debería pasar en el corpus, y si pasa el test lo grita).
fn regla_de(rj: &ReglaJson) -> Option<Regla> {
    Regla::try_from(rj).ok()
}

#[test]
fn la_inferencia_conforma_con_el_corpus_dorado() {
    let texto = include_str!("oraculo_inferencia.json");
    let casos: Vec<Caso> = serde_json::from_str(texto).expect("el corpus es JSON válido");
    assert!(!casos.is_empty(), "el corpus está vacío: eso NO es un aprobado");

    // Mismo calendario que el oráculo del motor: CO, 2019-2046.
    let motor = Motor::nuevo(Calendario::de_pais("CO", 2019, 2046).expect("CO tiene calendario"));

    let mut fallos: Vec<String> = Vec::new();
    for c in &casos {
        let fechas: Vec<NaiveDate> = c.fechas.iter().map(|s| fecha(s)).collect();
        let got = inferir(&fechas, &motor);
        let e = &c.esperado;

        if got.periodicidad != e.periodicidad {
            fallos.push(format!(
                "{}: periodicidad {:?}, se esperaba {:?}",
                c.caso, got.periodicidad, e.periodicidad
            ));
        }
        if got.explicacion != e.explicacion {
            fallos.push(format!(
                "{}: explicación «{}», se esperaba «{}»",
                c.caso, got.explicacion, e.explicacion
            ));
        }
        if got.exacta != e.exacta {
            fallos.push(format!("{}: exacta {}, se esperaba {}", c.caso, got.exacta, e.exacta));
        }
        if regla_de(&got.regla) != regla_de(&e.regla) {
            fallos.push(format!(
                "{}: la regla no coincide: {:?} vs {:?}",
                c.caso,
                regla_de(&got.regla),
                regla_de(&e.regla)
            ));
        }
        if got.descartados != e.descartados {
            fallos.push(format!(
                "{}: descartados {:?}, se esperaba {:?}",
                c.caso, got.descartados, e.descartados
            ));
        }
    }

    assert!(
        fallos.is_empty(),
        "la inferencia Rust no reproduce el corpus en {} punto(s):\n  {}",
        fallos.len(),
        fallos.join("\n  ")
    );
    println!("inferencia contra corpus dorado: {} casos conformes", casos.len());
}

#[derive(Deserialize)]
struct CasoInterp {
    caso: String,
    seleccion: Vec<String>,
    esperado: Vec<LecturaEsperada>,
}

#[derive(Deserialize)]
struct LecturaEsperada {
    periodicidad: Option<String>,
    nombre: String,
    dice: String,
    regla: ReglaJson,
    proximas: Vec<String>,
}

#[test]
fn las_interpretaciones_conforman_con_el_corpus_dorado() {
    let casos: Vec<CasoInterp> =
        serde_json::from_str(include_str!("oraculo_interpretaciones.json")).expect("corpus válido");
    assert!(!casos.is_empty(), "corpus vacío: NO es un aprobado");
    let motor = Motor::nuevo(Calendario::de_pais("CO", 2019, 2046).expect("CO tiene calendario"));

    let mut fallos: Vec<String> = Vec::new();
    for c in &casos {
        let sel: Vec<NaiveDate> = c.seleccion.iter().map(|s| fecha(s)).collect();
        let got = interpretaciones(&sel, &motor);

        if got.len() != c.esperado.len() {
            fallos.push(format!(
                "{}: {} lecturas, se esperaban {} ({:?} vs {:?})",
                c.caso,
                got.len(),
                c.esperado.len(),
                got.iter().map(|l| &l.dice).collect::<Vec<_>>(),
                c.esperado.iter().map(|l| &l.dice).collect::<Vec<_>>(),
            ));
            continue;
        }
        for (i, (g, e)) in got.iter().zip(&c.esperado).enumerate() {
            let donde = format!("{}[{}]", c.caso, i);
            if g.periodicidad != e.periodicidad {
                fallos.push(format!("{donde}: periodicidad {:?} vs {:?}", g.periodicidad, e.periodicidad));
            }
            if g.nombre != e.nombre {
                fallos.push(format!("{donde}: nombre «{}» vs «{}»", g.nombre, e.nombre));
            }
            if g.dice != e.dice {
                fallos.push(format!("{donde}: dice «{}» vs «{}»", g.dice, e.dice));
            }
            if regla_de(&g.regla) != regla_de(&e.regla) {
                fallos.push(format!("{donde}: regla distinta"));
            }
            let prox_e: Vec<NaiveDate> = e.proximas.iter().map(|s| fecha(s)).collect();
            if g.proximas != prox_e {
                fallos.push(format!("{donde}: próximas {:?} vs {:?}", g.proximas, prox_e));
            }
        }
    }

    assert!(
        fallos.is_empty(),
        "las interpretaciones no reproducen el corpus en {} punto(s):\n  {}",
        fallos.len(),
        fallos.join("\n  ")
    );
    println!("interpretaciones contra corpus dorado: {} casos conformes", casos.len());
}
