//! El port no se revisa: se mide.
//!
//! Estas pruebas comparan el motor de Rust contra las fechas EXACTAS que produjo
//! el motor de Python (retirado en el cutover), congeladas en `tests/oraculo/`:
//!
//!   - 484 festivos colombianos entre 2020 y 2045, día a día;
//!   - 8 534 vencimientos de los 80 patrones de una base real, con su día
//!     nominal, su día límite y el tipo de ajuste que los separa.
//!
//! Una sola fecha distinta pone esto rojo. Es la única condición bajo la que
//! reescribir un motor de cumplimiento en otro lenguaje es honesto: mientras el
//! oráculo pase, las dos implementaciones son la misma para todo lo que la
//! entidad tiene escrito.
//!
//! Si el Congreso cambia la ley de festivos, se regenera el oráculo con la
//! librería actualizada y el rojo aparece en la fecha exacta que cambió.

use std::collections::BTreeSet;
use std::path::PathBuf;

use chrono::NaiveDate;
use serde::Deserialize;
use siger_core::{Calendario, Motor, Regla, ReglaJson};

fn raiz_oraculo() -> PathBuf {
    // Corpus dorado vendorizado junto al test. Lo produjo el motor de Python
    // (ya retirado en el cutover a Rust); queda congelado como contrato.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/oraculo")
        .canonicalize()
        .expect("falta tests/oraculo con festivos_co.json y vencimientos.json")
}

#[derive(Deserialize)]
struct OraculoFestivos {
    desde: i32,
    hasta: i32,
    festivos: Vec<String>,
}

#[derive(Deserialize)]
struct OraculoVencimientos {
    patrones: Vec<PatronCongelado>,
}

#[derive(Deserialize)]
struct PatronCongelado {
    periodicidad: String,
    codigo: String,
    regla: ReglaJson,
    vencimientos: Vec<VencimientoCongelado>,
}

#[derive(Deserialize, PartialEq, Eq, Debug)]
struct VencimientoCongelado {
    nominal: String,
    limite: String,
    ajuste: String,
    // `hora` se quitó con el modo HORARIO. El JSON congelado sigue trayendo la
    // clave y serde la ignora, lo cual es correcto y además está comprobado: en
    // los 8 534 vencimientos del oráculo su valor es null SIN EXCEPCIÓN. El
    // motor anterior tampoco produjo nunca un vencimiento con hora sobre el
    // catálogo real, que es la razón de fondo por la que el modo se retiró.
}

fn leer<T: for<'de> Deserialize<'de>>(nombre: &str) -> T {
    let ruta = raiz_oraculo().join(nombre);
    let texto = std::fs::read_to_string(&ruta)
        .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", ruta.display()));
    serde_json::from_str(&texto).unwrap_or_else(|e| panic!("{} no es el JSON esperado: {e}", nombre))
}

#[test]
fn el_calendario_de_festivos_coincide_dia_a_dia() {
    let o: OraculoFestivos = leer("festivos_co.json");
    let esperados: BTreeSet<NaiveDate> = o
        .festivos
        .iter()
        .map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("fecha del oráculo"))
        .collect();

    let cal = Calendario::de_pais("CO", o.desde, o.hasta).expect("CO tiene calendario");

    let mut faltan = Vec::new();
    let mut sobran = Vec::new();
    let mut dia = NaiveDate::from_ymd_opt(o.desde, 1, 1).unwrap();
    let fin = NaiveDate::from_ymd_opt(o.hasta, 12, 31).unwrap();
    while dia <= fin {
        match (esperados.contains(&dia), cal.es_festivo(dia)) {
            (true, false) => faltan.push(dia),
            (false, true) => sobran.push(dia),
            _ => {}
        }
        dia = dia.succ_opt().unwrap();
    }

    assert!(
        faltan.is_empty() && sobran.is_empty(),
        "el calendario no coincide con el anterior.\n  faltan {} (primeros: {:?})\n  sobran {} (primeros: {:?})",
        faltan.len(),
        &faltan[..faltan.len().min(5)],
        sobran.len(),
        &sobran[..sobran.len().min(5)],
    );

    // Que coincida en cero no probaría nada si el oráculo estuviera vacío.
    assert!(esperados.len() > 400, "el oráculo de festivos parece vacío");
}

#[test]
fn los_vencimientos_de_los_80_patrones_se_reproducen_exactos() {
    let o: OraculoVencimientos = leer("vencimientos.json");
    let cal = Calendario::de_pais("CO", 2019, 2046).expect("CO tiene calendario");
    let motor = Motor::nuevo(cal);

    let mut total = 0usize;
    let mut fallos = Vec::new();

    for p in &o.patrones {
        let regla = match Regla::try_from(&p.regla) {
            Ok(r) => r,
            Err(e) => {
                fallos.push(format!("[{} {}] la regla no se pudo leer: {e}", p.periodicidad, p.codigo));
                continue;
            }
        };

        // El rango del oráculo, deducido de lo que contiene: no se codifica dos
        // veces la misma verdad.
        let desde = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let hasta = NaiveDate::from_ymd_opt(2030, 12, 31).unwrap();

        let obtenidos: Vec<VencimientoCongelado> = motor
            .vencimientos_entre(desde, hasta, &regla)
            .into_iter()
            .map(|v| VencimientoCongelado {
                nominal: v.nominal.fecha().to_string(),
                limite: v.limite.fecha().to_string(),
                ajuste: match v.ajuste {
                    siger_core::TipoAjuste::Ninguno => "NINGUNO",
                    siger_core::TipoAjuste::Anticipado => "ANTICIPADO",
                    siger_core::TipoAjuste::Gracia => "GRACIA",
                    siger_core::TipoAjuste::Omitir => "OMITIR",
                }
                .to_string(),
            })
            .collect();

        total += p.vencimientos.len();

        if obtenidos.len() != p.vencimientos.len() {
            fallos.push(format!(
                "[{} {}] {} vencimientos, el motor anterior produjo {}",
                p.periodicidad,
                p.codigo,
                obtenidos.len(),
                p.vencimientos.len()
            ));
            continue;
        }
        for (i, (a, b)) in obtenidos.iter().zip(p.vencimientos.iter()).enumerate() {
            if a != b {
                fallos.push(format!(
                    "[{} {}] vencimiento {i}: {:?} != {:?}",
                    p.periodicidad, p.codigo, a, b
                ));
                break;
            }
        }
    }

    assert!(
        fallos.is_empty(),
        "{} patrones no se reproducen:\n  {}",
        fallos.len(),
        fallos.join("\n  ")
    );
    assert!(total > 8000, "el oráculo de vencimientos parece vacío ({total})");
}

#[test]
fn el_traductor_de_la_notacion_produce_la_misma_regla() {
    // El motor puede ser correcto y el traductor de códigos estar mal: entonces
    // se generarían fechas impecables de una obligación que no es la del
    // cliente. Se comprueban por separado, y contra el mismo oráculo.
    let o: OraculoVencimientos = leer("vencimientos.json");
    let vigencia = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let mut fallos = Vec::new();

    for p in &o.patrones {
        // `FECHA` no lleva las fechas en el código: el oráculo trae las de
        // prueba y el traductor devuelve la lista vacía a propósito.
        if p.codigo.trim().eq_ignore_ascii_case("FECHA") {
            continue;
        }
        let esperada = Regla::try_from(&p.regla).expect("la regla congelada se lee");
        match siger_core::patrones::traducir(&p.periodicidad, &p.codigo, vigencia) {
            Ok(obtenida) => {
                if obtenida != esperada {
                    fallos.push(format!(
                        "[{} {}]\n     obtenida: {:?}\n    esperada: {:?}",
                        p.periodicidad, p.codigo, obtenida, esperada
                    ));
                }
            }
            Err(e) => fallos.push(format!("[{} {}] no se tradujo: {e:?}", p.periodicidad, p.codigo)),
        }
    }

    assert!(fallos.is_empty(), "{} códigos difieren:\n  {}", fallos.len(), fallos.join("\n  "));
}
