//! La notación de una base real: `A11<31`, `MH>5`, `S=2`, `BM>7-20`, `MUM`.
//!
//! Son los códigos con los que una entidad que opera lleva escritas sus 399
//! obligaciones en una hoja de cálculo. Traducirlos es lo que permite importar
//! su realidad sin pedirle que la reescriba, y por eso el traductor es parte del
//! producto y no una herramienta de migración de usar y tirar.
//!
//! # Lo que NO hace, y es deliberado
//!
//! `M3D<7` («tres días al mes, el primero el 7») devuelve [`None`]: los otros
//! dos días no están en el código, viven en la descripción en texto libre. No es
//! un hueco del motor —la forma existe, `dias_corte` la expresa— sino un límite
//! de la notación de origen. Inventar los días que faltan sería exactamente el
//! defecto que este sistema persigue: una obligación creada que no hace lo que
//! su autor cree.

use crate::recurrencia::{Ajuste, DiaDelMes, Especifico, Patron, Regla, Semanal};
use chrono::NaiveDate;

/// Por qué un código no se pudo traducir. Se devuelve el motivo, no un
/// `None` mudo: quien importa 399 filas necesita saber cuáles quedaron fuera y
/// por qué, o volverá a teclearlas todas por si acaso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinTraducir {
    /// El código no lleva la información: está en otra columna.
    FaltaEnLaNotacion(&'static str),
    /// La forma del código no se reconoce.
    FormaDesconocida(String),
}

const TODOS_LOS_MESES: [u32; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];

/// Meses de entrega según la periodicidad declarada, cuando el código no los da.
fn meses_de(periodicidad: &str) -> Vec<u32> {
    match periodicidad {
        "BIMESTRAL" => vec![1, 3, 5, 7, 9, 11],
        "TRIMESTRAL" => vec![3, 6, 9, 12],
        "SEMESTRAL" => vec![6, 12],
        "ANUAL" => vec![12],
        _ => TODOS_LOS_MESES.to_vec(),
    }
}

/// Traduce un código a una regla ejecutable.
///
/// `vigencia` se pasa desde fuera porque el código no la lleva: la notación dice
/// **qué** forma tiene la obligación, no desde cuándo rige.
///
/// **Y por eso quien la pase tiene que haberla acotado.** Esta función construye
/// `Regla` DIRECTAMENTE, sin pasar por `TryFrom<&ReglaJson>`, que es donde vive
/// la guardia de `fechas::en_rango_de_trabajo`. Hoy no hay consumidor de
/// producción —solo la llaman sus propias pruebas— así que no es una puerta
/// abierta; el día que lo haya, lo será. Una vigencia cerca de `NaiveDate::MIN`
/// hace que el motor de calendario entre en PÁNICO al buscar el lunes de esa
/// semana, y el pánico de un manejador no devuelve 500: devuelve una respuesta
/// vacía que en el panel no se ve.
pub fn traducir(
    periodicidad: &str,
    codigo: &str,
    vigencia: NaiveDate,
) -> Result<Regla, SinTraducir> {
    let c: String = codigo.trim().to_uppercase().chars().filter(|ch| !ch.is_whitespace()).collect();
    let p = periodicidad.trim().to_uppercase();

    if c == "FECHA" {
        // Sin patrón: las fechas viven en otra hoja del libro. La regla que se
        // devuelve es la forma correcta —una lista— con la lista vacía; quien
        // importa la rellena con las fechas reales de esa hoja.
        return Ok(Regla { vigencia: Some(vigencia), patron: Patron::Fechas(vec![]), ajuste: Ajuste::Ninguno });
    }
    if c == "MUM" {
        return Ok(Regla {
            vigencia: Some(vigencia),
            patron: Patron::Especifico(Especifico {
                meses: TODOS_LOS_MESES.to_vec(),
                dia: DiaDelMes::OrdinalDeSemana { dia_semana: 1, ordinal: -1 }, // último martes
                cada_n_anios: None,
            }),
            ajuste: Ajuste::Ninguno,
        });
    }

    let Some(partes) = descomponer(&c) else {
        if es_forma_m_d(&c) {
            return Err(SinTraducir::FaltaEnLaNotacion(
                "el código no lleva los días: están solo en la descripción",
            ));
        }
        return Err(SinTraducir::FormaDesconocida(codigo.to_string()));
    };

    let ajuste = match partes.signo {
        '<' => Ajuste::Anticipado,
        '>' => Ajuste::Gracia,
        _ => Ajuste::Ninguno,
    };

    // SEMANAL: el número es el día de la SEMANA (1=lunes … 5=viernes), y dos
    // dígitos son dos días. No es un día del mes, y confundirlos es justo el
    // error que la línea lógica del asistente existe para impedir.
    let es_semanal = (p == "SEMANAL" || p == "2 VECES POR SEMANA")
        && partes.letras.starts_with('S')
        && p != "SEMESTRAL";
    if es_semanal {
        if partes.letras.contains('H') {
            // El n-ésimo hábil DE LA SEMANA: ya es hábil, no lleva ajuste.
            return Ok(Regla {
                vigencia: Some(vigencia),
                patron: Patron::Semanal(Semanal {
                    dias: vec![],
                    dia_habil: Some(partes.dia),
                    cada_n_semanas: None,
                }),
                ajuste: Ajuste::Ninguno,
            });
        }
        let dias: Vec<u32> = if partes.dia > 6 {
            partes.dia.to_string().chars().filter_map(|d| d.to_digit(10)).map(|d| d - 1).collect()
        } else {
            vec![partes.dia - 1]
        };
        return Ok(Regla {
            vigencia: Some(vigencia),
            patron: Patron::Semanal(Semanal { dias, dia_habil: None, cada_n_semanas: None }),
            ajuste,
        });
    }

    let meses = meses_de(&p);
    let (dia, ajuste) = if partes.letras.contains('H') {
        // El n-ésimo día hábil ya es hábil por construcción: el ajuste no tiene
        // nada que hacer y arrastrarlo movería la fecha dos veces.
        (DiaDelMes::Habil(partes.dia), Ajuste::Ninguno)
    } else if let Some(d2) = partes.dia2 {
        (DiaDelMes::Cortes(vec![partes.dia, d2]), ajuste)
    } else if partes.dia == 31 {
        // «31» en esta notación significa «último», no «el día treinta y uno»:
        // por eso un febrero no se queda sin obligación.
        (DiaDelMes::Ultimo, ajuste)
    } else {
        (DiaDelMes::Corte(partes.dia), ajuste)
    };

    Ok(Regla {
        vigencia: Some(vigencia),
        patron: Patron::Especifico(Especifico { meses, dia, cada_n_anios: None }),
        ajuste,
    })
}

struct Partes {
    letras: String,
    dia: u32,
    dia2: Option<u32>,
    signo: char,
}

/// `MH>5` → letras `MH`, día 5. `BM>7-20` → letras `BM`, días 7 y 20.
/// `AX1<31` → letras `A` (la `X` es «indefinido» y no altera el cálculo).
fn descomponer(c: &str) -> Option<Partes> {
    let pos = c.find(['<', '>', '='])?;
    let (izquierda, derecha) = c.split_at(pos);
    let signo = derecha.chars().next()?;
    let derecha = &derecha[1..];

    // Izquierda: letras, una `X` opcional de relleno y un número que la notación
    // usa como orden y que el cálculo no necesita.
    let alfa: String = izquierda.chars().take_while(|ch| ch.is_ascii_alphabetic()).collect();
    let resto = &izquierda[alfa.len()..];
    if !resto.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let letras = match alfa.strip_suffix('X') {
        Some(sin_x) if !sin_x.is_empty() => sin_x.to_string(),
        _ => alfa,
    };
    if letras.is_empty() {
        return None;
    }

    // Derecha: `dd` o `dd-dd`, uno o dos dígitos cada uno.
    let (a, b) = match derecha.split_once('-') {
        Some((a, b)) => (a, Some(b)),
        None => (derecha, None),
    };
    let dia = numero_corto(a)?;
    let dia2 = match b {
        Some(b) => Some(numero_corto(b)?),
        None => None,
    };

    Some(Partes { letras, dia, dia2, signo })
}

fn numero_corto(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 2 || !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// `M3D<7` y compañía: la forma existe, los datos no.
fn es_forma_m_d(c: &str) -> bool {
    let b = c.as_bytes();
    b.len() >= 5
        && b[0] == b'M'
        && b[1].is_ascii_digit()
        && b[2] == b'D'
        && matches!(b[3], b'<' | b'>' | b'=')
        && b[4..].iter().all(|d| d.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vigencia() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
    }

    #[test]
    fn el_31_de_la_notacion_significa_ultimo_dia() {
        let r = traducir("ANUAL", "A11<31", vigencia()).unwrap();
        match r.patron {
            Patron::Especifico(e) => assert_eq!(e.dia, DiaDelMes::Ultimo),
            otro => panic!("se esperaba específico, salió {otro:?}"),
        }
        assert_eq!(r.ajuste, Ajuste::Anticipado);
    }

    #[test]
    fn el_habil_no_arrastra_ajuste() {
        // `MH>5` lleva `>`, pero el 5º día hábil YA es hábil: aplicarle el ajuste
        // lo movería una segunda vez, al 6º.
        let r = traducir("MENSUAL", "MH>5", vigencia()).unwrap();
        assert_eq!(r.ajuste, Ajuste::Ninguno);
    }

    #[test]
    fn dos_digitos_en_semanal_son_dos_dias() {
        let r = traducir("SEMANAL", "S>35", vigencia()).unwrap();
        match r.patron {
            Patron::Semanal(s) => assert_eq!(s.dias, vec![2, 4]), // miércoles y viernes
            otro => panic!("se esperaba semanal, salió {otro:?}"),
        }
    }

    #[test]
    fn lo_que_la_notacion_no_dice_no_se_inventa() {
        assert!(matches!(
            traducir("MENSUAL", "M3D<7", vigencia()),
            Err(SinTraducir::FaltaEnLaNotacion(_))
        ));
    }
}
