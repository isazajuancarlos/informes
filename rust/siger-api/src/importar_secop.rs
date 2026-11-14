//! Siembra el calendario de obligaciones a partir de una foto de contratos de
//! SECOP II (un CSV descargado a mano, no una integración viva).
//!
//! # Por qué una importación y no un conector
//!
//! SIGER no habla con SECOP en producción, y es deliberado (ficha #608): un
//! conector a un sistema del Estado es un mantenimiento que se rompe —en
//! silencio— el día que el Estado cambia el formato, y mete a un tercero en el
//! camino crítico de un producto que se vende como «funciona sin depender de
//! nadie». Esto es lo contrario: se lee un archivo UNA vez, se siembra, y se
//! acabó. Si el formato cambia, no se cae nada; deja de servir el importador,
//! ruidosamente y en el momento de usarlo.
//!
//! Convierte la demostración en «aquí está SU contrato con SUS fechas», que es
//! lo que el plan comercial dice que convierte a un prospecto.
//!
//! # El calendario, y lo que NO se inventa
//!
//! El CSV trae el INICIO y el FIN de cada contrato, pero **no la periodicidad**
//! de los informes. Inventarla sería un dato falso (directiva 20), así que la
//! periodicidad es un PARÁMETRO declarado —`--periodicidad mensual` por
//! defecto— y el cronograma se guarda como una regla de FECHAS EXPLÍCITAS
//! (`compat::ReglaJson` modo `FECHAS`), que a diferencia de una regla periódica
//! **acota en el fin del contrato** en vez de generar vencimientos para siempre
//! (una obligación de SIGER no tiene fecha de fin: `Regla` solo lleva la
//! vigencia de inicio, y el planificador barre por ventana).
//!
//! Las entregas FUTURAS de los contratos vivos se materializan aquí, para que el
//! calendario completo se vea al instante y para que el planificador no rellene
//! hacia atrás fabricando incumplimientos de meses pasados que nadie midió
//! (`MAX_RETROACTIVO_DIAS`): al quedar la última entrega en el fin del contrato,
//! su barrido empieza después del fin y no encuentra nada. De los contratos
//! históricos se guarda el registro —con su estado, para prospectar— pero no se
//! fabrica ni una entrega.
//!
//! # Principio
//!
//! Como el volcado (`importar.rs`), falla RUIDOSAMENTE y en una sola
//! transacción: una fecha ilegible, un contrato sin entidad, dos filas que
//! producen el mismo nombre de informe —o entra todo, o no entra nada—. Un dato
//! de relleno produce una demostración que *parece* sembrada, y eso es peor que
//! una que se cae.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use siger_core::{Regla, ReglaJson};

#[derive(Debug, thiserror::Error)]
pub enum ErrorSecop {
    #[error("no se pudo leer el CSV «{0}»: {1}")]
    Leer(String, std::io::Error),
    #[error("no se pudo preparar la base de destino: {0}")]
    Destino(#[from] crate::db::ErrorBd),
    #[error(transparent)]
    Bd(#[from] sqlx::Error),
    #[error("el CSV está vacío: ni siquiera trae la fila de cabecera")]
    Vacio,
    #[error(
        "el CSV está mal formado: {0}. Seguir sembraría de menos en silencio, así \
         que se detiene (directiva 20)."
    )]
    CsvMalformado(&'static str),
    #[error(
        "a la cabecera del CSV le falta la columna «{0}». Las que necesita el \
         importador: nombre_entidad, objeto_del_contrato, \
         fecha_de_inicio_del_contrato, fecha_de_fin_del_contrato, estado_contrato \
         y referencia_del_contrato."
    )]
    FaltaColumna(&'static str),
    #[error("fila {0}: la columna «{1}» está vacía, y sin ella no se puede sembrar el contrato")]
    CampoVacio(usize, &'static str),
    #[error("fila {0}: el cronograma generado no se pudo leer: {1}")]
    Cronograma(usize, String),
    #[error(
        "las filas {0} y {1} producen el mismo nombre de informe «{2}». El nombre \
         se compone de la referencia del contrato y la entidad; dos contratos con \
         la misma referencia en la misma entidad no se pueden distinguir. Revise el \
         CSV."
    )]
    NombreDuplicado(usize, usize, String),
    #[error(
        "periodicidad «{0}» desconocida. Hoy solo se admite «mensual» — es la \
         cadencia del informe de supervisión de un contrato de prestación. Añadir \
         otra es un cambio deliberado, no un dedazo."
    )]
    Periodicidad(String),
    #[error(
        "no se sembró nada: {0}. No se escribe una base vacía en silencio: eso \
         dejaría una demo sin contratos y nadie entendería por qué (directiva 20)."
    )]
    NadaQueSembrar(String),
}

/// Cadencia con la que se generan las fechas del cronograma dentro de la
/// vigencia del contrato. Es un parámetro, no un valor supuesto: el CSV no la
/// trae y adivinarla sería relleno (directiva 20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Periodicidad {
    Mensual,
}

impl Periodicidad {
    pub fn desde(s: &str) -> Result<Self, ErrorSecop> {
        match s.trim().to_lowercase().as_str() {
            "mensual" => Ok(Periodicidad::Mensual),
            otra => Err(ErrorSecop::Periodicidad(otra.to_string())),
        }
    }
}

/// Lo sembrado, para imprimirlo al terminar. `vivos` + `historicos` = contratos.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ResumenSecop {
    pub contratos: u64,
    pub vivos: u64,
    pub historicos: u64,
    /// Contratos con fin futuro que NO quedaron vivos porque su estado SECOP es
    /// de cierre (Cerrado, terminado, Cancelado, cedido, Borrador). Se cuenta
    /// aparte para que «fin futuro» y «genera calendario» no se confundan.
    pub futuros_pero_cerrados: u64,
    /// Contratos importados como registro pero SIN calendario, porque les falta
    /// una fecha usable (vacía, ilegible o fuera de rango) o el inicio es
    /// posterior al fin. No se inventa la fecha (directiva 20); el contrato entra
    /// para prospectar y se cuenta aquí para que el hueco no pase en silencio.
    pub sin_calendario: u64,
    pub stakeholders: u64,
    pub participaciones: u64,
    pub entregas: u64,
}

/// Los estados de SECOP en los que un contrato está de verdad en curso. Solo
/// éstos, con fin futuro, siembran calendario vivo; el resto es historia para
/// prospectar. Se comparan en mayúsculas y sin depender del acento de «ejecución».
fn estado_en_curso(estado: &str) -> bool {
    let e = estado.trim().to_uppercase();
    e.starts_with("EN EJECUCI") || e == "MODIFICADO" || e == "APROBADO"
}

/// Los días de un mes, sin tabla ni casos especiales: el día 1 del mes siguiente
/// menos un día. `inicio`/`fin` vienen de fechas reales del CSV (2017..2026), así
/// que los `unwrap` no son alcanzables con datos válidos; aun así se acotan por
/// `en_rango_de_trabajo` antes de llamar aquí.
fn dias_del_mes(anio: i32, mes: u32) -> u32 {
    let (na, nm) = if mes == 12 { (anio + 1, 1) } else { (anio, mes + 1) };
    NaiveDate::from_ymd_opt(na, nm, 1)
        .and_then(|primero_sig| primero_sig.pred_opt())
        .map(|ultimo| ultimo.day())
        .unwrap_or(28)
}

/// Las fechas nominales del cronograma: una por mes entre `inicio` y `fin`,
/// ambos inclusive, ancladas al día del FIN del contrato (el día en que vence el
/// informe). El día se recorta a los que tiene cada mes —un ancla en 31 cae en
/// el 30 o el 28— para no saltarse meses cortos.
fn mensuales(inicio: NaiveDate, fin: NaiveDate) -> Vec<NaiveDate> {
    let ancla = fin.day();
    let mut fechas = Vec::new();
    let (mut a, mut m) = (inicio.year(), inicio.month());
    loop {
        let dia = ancla.min(dias_del_mes(a, m));
        if let Some(f) = NaiveDate::from_ymd_opt(a, m, dia) {
            if f > fin {
                break;
            }
            if f >= inicio {
                fechas.push(f);
            }
        }
        if m == 12 {
            a += 1;
            m = 1;
        } else {
            m += 1;
        }
        // Cinturón: los datos reales acotan esto, pero un bucle sobre fechas no
        // se deja sin freno (el planificador aprendió la lección con el pánico
        // del año 262142).
        if a > fin.year() + 1 {
            break;
        }
    }
    fechas
}

/// La fecha USABLE para el calendario, o `None`. SECOP entrega
/// `2024-01-15T00:00:00.000`; nos quedamos con el día. Devuelve `None` —no un
/// error— cuando la fecha está vacía, es ilegible, o cae fuera del rango de
/// trabajo del motor (que entra en pánico cerca de los extremos de `chrono`).
/// Las tres significan lo mismo aquí: no hay con qué armar un cronograma. El
/// contrato se importa igual, como registro sin calendario; no se inventa una
/// fecha (directiva 20).
fn fecha_usable(s: &str) -> Option<NaiveDate> {
    let dia = s.trim().split(['T', ' ']).next().unwrap_or("").trim();
    let d = NaiveDate::parse_from_str(dia, "%Y-%m-%d").ok()?;
    siger_core::fechas::en_rango_de_trabajo(d).then_some(d)
}

/// Un lector de CSV mínimo pero correcto (RFC 4180): campos separados por coma,
/// registros por salto de línea, campos entrecomillados que pueden contener
/// coma y salto, y `""` como comilla literal dentro de un campo. Se escribe a
/// mano —en vez de traer el crate `csv`— porque la construcción de este
/// repositorio es VENDORIZADA y sin red (`.cargo/config.toml`): una dependencia
/// nueva obliga a `cargo vendor` y a re-anclar los bytes, un precio alto para
/// leer una foto de un formato fijo. Es un lexer, no una expresión regular sobre
/// código (directiva 17).
///
/// Devuelve las filas ya separadas en campos. No interpreta la cabecera: de eso
/// se encarga quien llama. Falla (`Err`) si el archivo termina DENTRO de un campo
/// entrecomillado —una comilla sin cerrar—, porque a partir de ahí el resto del
/// archivo se absorbería en un solo campo y las filas siguientes desaparecerían
/// sin error: exactamente el «siembra de menos y sale con 0» que la directiva 20
/// prohíbe.
fn leer_csv(texto: &str) -> Result<Vec<Vec<String>>, ErrorSecop> {
    let mut filas = Vec::new();
    let mut campo = String::new();
    let mut fila: Vec<String> = Vec::new();
    let mut en_comillas = false;
    let mut chars = texto.chars().peekable();
    // Rastro de si el registro en curso tiene algún contenido, para no emitir una
    // fila vacía por el salto de línea final del archivo.
    let mut hay_algo = false;

    // Cierra el registro en curso. Se llama tanto en '\n' como en '\r': así el
    // lector trata LF, CRLF y CR-a-secas igual. Antes '\r' se ignoraba «porque el
    // salto lo marca \n», y un archivo con terminador solo-CR colapsaba entero en
    // un registro sin que nadie lo notara.
    macro_rules! cerrar_registro {
        () => {
            if hay_algo || !campo.is_empty() || !fila.is_empty() {
                fila.push(std::mem::take(&mut campo));
                filas.push(std::mem::take(&mut fila));
            }
            hay_algo = false;
        };
    }

    while let Some(c) = chars.next() {
        if en_comillas {
            match c {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        campo.push('"');
                    } else {
                        en_comillas = false;
                    }
                }
                otro => campo.push(otro),
            }
            continue;
        }
        match c {
            '"' => {
                en_comillas = true;
                hay_algo = true;
            }
            ',' => {
                fila.push(std::mem::take(&mut campo));
                hay_algo = true;
            }
            '\r' => {
                // Fin de registro. Un '\n' que le siga (CRLF) es el mismo salto,
                // no uno nuevo: se consume.
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                cerrar_registro!();
            }
            '\n' => {
                cerrar_registro!();
            }
            otro => {
                campo.push(otro);
                hay_algo = true;
            }
        }
    }

    // Un archivo que termina dentro de comillas está cortado o mal formado.
    if en_comillas {
        return Err(ErrorSecop::CsvMalformado(
            "termina dentro de un campo entrecomillado (una comilla quedó sin cerrar)",
        ));
    }
    // Última fila sin salto de línea final.
    if hay_algo || !campo.is_empty() || !fila.is_empty() {
        fila.push(campo);
        filas.push(fila);
    }
    Ok(filas)
}

/// El índice de cada columna que el importador necesita, leído de la cabecera
/// por NOMBRE y no por posición: así un reordenamiento de columnas en SECOP no
/// siembra datos en la columna equivocada en silencio.
struct Columnas {
    entidad: usize,
    objeto: usize,
    inicio: usize,
    fin: usize,
    estado: usize,
    referencia: usize,
    // Opcionales: si faltan, el contrato se siembra igual sin esa parte.
    proveedor: Option<usize>,
    supervisor: Option<usize>,
    valor: Option<usize>,
    id_contrato: Option<usize>,
}

impl Columnas {
    fn de_cabecera(cab: &[String]) -> Result<Self, ErrorSecop> {
        let idx = |n: &str| cab.iter().position(|c| c.trim() == n);
        let obl = |n: &'static str| idx(n).ok_or(ErrorSecop::FaltaColumna(n));
        Ok(Columnas {
            entidad: obl("nombre_entidad")?,
            objeto: obl("objeto_del_contrato")?,
            inicio: obl("fecha_de_inicio_del_contrato")?,
            fin: obl("fecha_de_fin_del_contrato")?,
            estado: obl("estado_contrato")?,
            referencia: obl("referencia_del_contrato")?,
            proveedor: idx("proveedor_adjudicado"),
            supervisor: idx("nombre_supervisor"),
            valor: idx("valor_del_contrato"),
            id_contrato: idx("id_contrato"),
        })
    }
}

/// Un nombre de supervisor real, o `None` si SECOP lo dejó sin definir. «No
/// definido»/«No aplica» no son personas: crear un stakeholder con ese nombre
/// fabricaría una parte que no existe.
fn parte_nombrada(v: Option<&str>) -> Option<String> {
    let s = v?.trim();
    if s.is_empty() {
        return None;
    }
    let bajo = s.to_lowercase();
    if bajo == "no definido" || bajo == "no aplica" || bajo == "n/a" {
        return None;
    }
    Some(s.to_string())
}

/// Sube el stakeholder si es nuevo y devuelve su id, reutilizándolo entre
/// contratos: una entidad que aparece en veinte filas es UNA parte, no veinte.
async fn stakeholder_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    cache: &mut HashMap<String, i64>,
    resumen: &mut ResumenSecop,
    nombre: &str,
) -> Result<i64, ErrorSecop> {
    if let Some(id) = cache.get(nombre) {
        return Ok(*id);
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO stakeholders (nombre, externo) VALUES (?, 1) RETURNING id",
    )
    .bind(nombre)
    .fetch_one(&mut **tx)
    .await?;
    cache.insert(nombre.to_string(), id);
    resumen.stakeholders += 1;
    Ok(id)
}

/// Siembra el calendario desde el CSV. Todo dentro de una transacción.
///
/// `hoy` es un parámetro y no `Utc::now()` dentro para que la clasificación
/// vivo/histórico —que depende de si el fin ya pasó— se pueda probar con fechas
/// fijas.
pub async fn importar_secop(
    csv: &str,
    destino: &str,
    periodicidad: Periodicidad,
    filtro: Option<&str>,
    hoy: NaiveDate,
) -> Result<ResumenSecop, ErrorSecop> {
    let texto = std::fs::read_to_string(csv).map_err(|e| ErrorSecop::Leer(csv.to_string(), e))?;
    let filas = leer_csv(&texto)?;
    let (cabecera, datos) = filas.split_first().ok_or(ErrorSecop::Vacio)?;
    let col = Columnas::de_cabecera(cabecera)?;

    // A DIFERENCIA del volcado hermano (`importar.rs`), esta siembra NO exige una
    // base destino vacía, Y ES DELIBERADO: siembra de forma ADITIVA sobre la base
    // de demostración, que `sembrar_demo.py` ya pobló con cuentas y datos. Copiar
    // aquí la guardia `destino_vacio` de `importar.rs` rompería el uso previsto.
    // No amplía alcance ni corrompe (revisado 2026-09-01): los stakeholders se
    // insertan siempre nuevos —id autoincremental por encima de los existentes—,
    // así que jamás colisionan con el `stakeholder_id` de un Tercero ya dado de
    // alta, y una re-siembra choca en `informes.nombre UNIQUE` (falla ruidoso, no
    // duplica). El argumento `destino` es entrada del operador (confiable).
    let dst = crate::db::abrir(&format!("sqlite://{destino}?mode=rwc")).await?;
    let mut resumen = ResumenSecop::default();
    let mut cache: HashMap<String, i64> = HashMap::new();
    // Nombre de informe -> número de fila que lo produjo, para nombrar las dos
    // filas cuando choquen en vez de dejar que el UNIQUE lance un error opaco.
    let mut nombres: HashMap<String, usize> = HashMap::new();
    let mut tx = dst.begin().await?;

    let Periodicidad::Mensual = periodicidad;

    for (i, fila) in datos.iter().enumerate() {
        let n = i + 2; // +1 por base 1, +1 por la cabecera: el número de línea del archivo.
        let campo = |idx: usize| fila.get(idx).map(|s| s.trim()).unwrap_or("");
        let opc = |idx: Option<usize>| idx.and_then(|k| fila.get(k)).map(|s| s.trim());

        let entidad = campo(col.entidad);
        let objeto = campo(col.objeto);
        let referencia = campo(col.referencia);
        let estado = campo(col.estado);

        // Filtro por-prospecto — ANTES de validar campos: una fila que se va a
        // descartar no debe abortar la siembra por un campo vacío suyo. Si se
        // pidió `--solo`, se siembra únicamente el contrato cuya referencia, id de
        // contrato o entidad contenga el texto. Es lo que hace posible la demo
        // «aquí está SU contrato» sin volcar los 336 a una base que verá un
        // tercero. Substring sin distinguir mayúsculas, sobre los tres campos que
        // un prospecto reconoce como suyos.
        if let Some(f) = filtro {
            let f = f.trim().to_lowercase();
            let idc = opc(col.id_contrato).unwrap_or("").to_lowercase();
            let coincide = referencia.to_lowercase().contains(&f)
                || entidad.to_lowercase().contains(&f)
                || idc.contains(&f);
            if !coincide {
                continue;
            }
        }

        // Campos obligatorios: sin ellos el contrato no se puede sembrar. (En el
        // CSV real nunca vienen vacíos; medido.)
        if entidad.is_empty() {
            return Err(ErrorSecop::CampoVacio(n, "nombre_entidad"));
        }
        if objeto.is_empty() {
            return Err(ErrorSecop::CampoVacio(n, "objeto_del_contrato"));
        }
        if referencia.is_empty() {
            return Err(ErrorSecop::CampoVacio(n, "referencia_del_contrato"));
        }

        let inicio = fecha_usable(campo(col.inicio));
        let fin = fecha_usable(campo(col.fin));
        // El calendario necesita las DOS fechas y en orden. Si falta una, es
        // ilegible o el inicio va tras el fin, el contrato entra igual pero SIN
        // calendario: no se inventa una fecha (directiva 20).
        let con_calendario = matches!((inicio, fin), (Some(i), Some(f)) if i <= f);

        // La regla del cronograma, si el estado del contrato lo hace vivo, y las
        // entregas futuras que se van a materializar. Tres cosas que se deciden
        // juntas porque dependen de las mismas dos fechas.
        let (regla, vivo, entregas_futuras): (ReglaJson, bool, Vec<String>) = if con_calendario {
            let (i, f) = (inicio.expect("con_calendario ⇒ inicio"), fin.expect("con_calendario ⇒ fin"));
            // Vivo = fin no pasó Y el estado dice que está en curso. Un contrato
            // Cerrado con fin futuro NO genera obligaciones: marcarlo vivo las
            // fabricaría.
            let fin_futuro = f >= hoy;
            let vivo = fin_futuro && estado_en_curso(estado);
            if fin_futuro && !estado_en_curso(estado) {
                resumen.futuros_pero_cerrados += 1;
            }
            // El cronograma completo del contrato, como regla de FECHAS
            // explícitas. Las entregas materializadas son solo las FUTURAS de los
            // vivos: las pasadas no se fabrican (no sabemos si se cumplieron), y
            // al dejar la última entrega en el fin el planificador no rellena
            // hacia atrás.
            let cronograma = mensuales(i, f);
            let futuras = if vivo {
                cronograma.iter().filter(|d| **d >= hoy).map(|d| d.to_string()).collect()
            } else {
                Vec::new()
            };
            let regla = ReglaJson {
                modo: Some("FECHAS".to_string()),
                fecha_inicio: Some(i.to_string()),
                fechas: Some(cronograma.iter().map(|d| d.to_string()).collect()),
                ..Default::default()
            };
            (regla, vivo, futuras)
        } else {
            resumen.sin_calendario += 1;
            // A solicitud = sin calendario. La vigencia se conserva solo si el
            // inicio era una fecha usable; si no, ni eso se inventa.
            let regla = ReglaJson {
                modo: Some("A_SOLICITUD".to_string()),
                fecha_inicio: inicio.map(|d| d.to_string()),
                ..Default::default()
            };
            (regla, false, Vec::new())
        };

        // Se valida que la regla se pueda leer ANTES de guardarla: una regla que
        // el motor no entiende no generaría entregas nunca, y el fallo aparecería
        // lejos de aquí (misma cautela que el volcado con `recurrencia`).
        let recurrencia = serde_json::to_string(&regla).expect("ReglaJson serializa");
        Regla::try_from(&regla).map_err(|e| ErrorSecop::Cronograma(n, e.to_string()))?;

        let valor = opc(col.valor).filter(|s| !s.is_empty());
        let id_contrato = opc(col.id_contrato).filter(|s| !s.is_empty());

        // El nombre del informe es legible —referencia y entidad— pero tiene que
        // ser único (columna UNIQUE). Dos contratos DISTINTOS pueden compartir
        // referencia y entidad (la referencia se reutiliza); ahí desambigua el id
        // del contrato, que es su PK en SECOP. Si ni con eso hay unicidad —o no
        // hay id—, se falla ruidosamente nombrando las dos filas.
        let base = format!("{referencia} · {entidad}");
        let nombre = if nombres.contains_key(&base) {
            match id_contrato {
                Some(idc) => format!("{base} · {idc}"),
                None => format!("{base} · fila {n}"),
            }
        } else {
            base
        };
        if let Some(previa) = nombres.insert(nombre.clone(), n) {
            return Err(ErrorSecop::NombreDuplicado(previa, n, nombre));
        }
        let descripcion = {
            let mut d = objeto.to_string();
            d.push_str(&format!("\n\n— Contrato {referencia}"));
            if let Some(idc) = id_contrato {
                d.push_str(&format!(" ({idc})"));
            }
            d.push_str(&format!(" · {entidad} · estado SECOP: {estado}"));
            if let Some(v) = valor {
                d.push_str(&format!(" · valor {v}"));
            }
            d
        };

        let informe_id: i64 = sqlx::query_scalar(
            "INSERT INTO informes (nombre, descripcion, recurrencia, activo) \
             VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(&nombre)
        .bind(&descripcion)
        .bind(&recurrencia)
        .bind(i64::from(vivo))
        .fetch_one(&mut *tx)
        .await?;
        resumen.contratos += 1;
        if vivo {
            resumen.vivos += 1;
        } else {
            resumen.historicos += 1;
        }

        // Las partes, como stakeholders externos deduplicados. Orden por papel
        // para que el panel las liste igual cada vez.
        let entidad_id = stakeholder_id(&mut tx, &mut cache, &mut resumen, entidad).await?;
        let mut partes: Vec<(i64, &str, i64)> = vec![(entidad_id, "DESTINO", 0)];
        if let Some(prov) = parte_nombrada(opc(col.proveedor)) {
            let pid = stakeholder_id(&mut tx, &mut cache, &mut resumen, &prov).await?;
            partes.push((pid, "ORIGEN", 1));
        }
        if let Some(sup) = parte_nombrada(opc(col.supervisor)) {
            let sid = stakeholder_id(&mut tx, &mut cache, &mut resumen, &sup).await?;
            partes.push((sid, "SUPERVISA", 2));
        }
        for (sid, rol, orden) in partes {
            sqlx::query(
                "INSERT INTO participacion (informe_id, stakeholder_id, rol, es_principal, orden) \
                 VALUES (?, ?, ?, 1, ?)",
            )
            .bind(informe_id)
            .bind(sid)
            .bind(rol)
            .bind(orden)
            .execute(&mut *tx)
            .await?;
            resumen.participaciones += 1;
        }

        // Las entregas FUTURAS de los vivos (ya filtradas arriba). Los históricos
        // y los contratos sin calendario no reciben ninguna.
        for fecha in &entregas_futuras {
            sqlx::query(
                "INSERT INTO entregas (informe_id, fecha_nominal, fecha_limite, estado) \
                 VALUES (?, ?, ?, 'PENDIENTE')",
            )
            .bind(informe_id)
            .bind(fecha)
            .bind(fecha)
            .execute(&mut *tx)
            .await?;
            resumen.entregas += 1;
        }
    }

    // Nada sembrado = no se commitea una base vacía. Pasa si `--solo` no casa
    // ningún contrato, o si el CSV solo trae la cabecera. La transacción se
    // deshace al retornar (rollback por Drop).
    if resumen.contratos == 0 {
        return Err(ErrorSecop::NadaQueSembrar(match filtro {
            Some(f) => format!("ningún contrato casa el filtro «{f}»"),
            None => "el CSV no trae ninguna fila de contrato".to_string(),
        }));
    }

    sqlx::query("INSERT INTO auditoria (momento, evento, email, detalle) VALUES (?, 'IMPORTACION_SECOP', NULL, ?)")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(format!(
            "siembra desde «{csv}»: {} contratos ({} vivos, {} históricos), \
             {} stakeholders, {} participaciones, {} entregas",
            resumen.contratos, resumen.vivos, resumen.historicos,
            resumen.stakeholders, resumen.participaciones, resumen.entregas
        ))
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(resumen)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── El lector de CSV: cada regla del RFC con su pareja ──────────────────

    #[test]
    fn el_csv_separa_campos_y_filas_simples() {
        let f = leer_csv("a,b,c\n1,2,3\n").unwrap();
        assert_eq!(f, vec![vec!["a", "b", "c"], vec!["1", "2", "3"]]);
    }

    /// La razón de ser del lexer: un campo entrecomillado con COMA dentro es UN
    /// campo, no dos. Es exactamente el `objeto_del_contrato` de SECOP, que lleva
    /// comas y punto y coma. El control es la misma fila SIN comillas, que sí se
    /// parte.
    #[test]
    fn una_coma_dentro_de_comillas_no_parte_el_campo() {
        let con = leer_csv("\"presta servicios, y asesora\",activo\n").unwrap();
        assert_eq!(con, vec![vec!["presta servicios, y asesora", "activo"]]);
        // Pareja: sin las comillas, la coma sí separa — tres campos.
        let sin = leer_csv("presta servicios, y asesora,activo\n").unwrap();
        assert_eq!(sin, vec![vec!["presta servicios", " y asesora", "activo"]]);
    }

    #[test]
    fn una_comilla_escapada_y_un_salto_dentro_del_campo() {
        // `""` es una comilla literal; un salto de línea dentro de comillas NO
        // termina el registro.
        let f = leer_csv("\"dijo \"\"hola\"\"\nen dos líneas\",fin\n").unwrap();
        assert_eq!(f, vec![vec!["dijo \"hola\"\nen dos líneas", "fin"]]);
    }

    #[test]
    fn la_ultima_fila_sin_salto_final_tambien_entra() {
        // Y un CRLF se trata como un solo salto (no deja un campo con '\r').
        let f = leer_csv("a,b\r\n1,2").unwrap();
        assert_eq!(f, vec![vec!["a", "b"], vec!["1", "2"]]);
    }

    /// **Una comilla sin cerrar es un error, no un archivo medio leído.** Si el
    /// archivo termina dentro de comillas, el resto se absorbería en un solo campo
    /// y las filas siguientes desaparecerían sin señal — el «siembra de menos y
    /// sale con 0» que la directiva 20 prohíbe. El control es el mismo texto CON
    /// la comilla cerrada, que sí parsea.
    #[test]
    fn una_comilla_sin_cerrar_es_un_error_ruidoso() {
        assert!(
            matches!(leer_csv("\"sin cerrar,y se traga el resto\notra fila"), Err(ErrorSecop::CsvMalformado(_))),
            "una comilla sin cerrar tiene que fallar, no leer a medias"
        );
        // Control: cerrada, parsea a un campo con la coma y el salto dentro.
        let ok = leer_csv("\"sin cerrar,y se traga el resto\notra fila\"\n").unwrap();
        assert_eq!(ok, vec![vec!["sin cerrar,y se traga el resto\notra fila"]]);
    }

    /// Un archivo con terminador SOLO-CR (viejo Mac / algún export) no colapsa en
    /// un registro: cada '\r' cierra su fila igual que '\n'. El control es que un
    /// '\r' DENTRO de comillas siga siendo dato.
    #[test]
    fn un_terminador_solo_cr_separa_las_filas() {
        let f = leer_csv("a,b\r1,2\r").unwrap();
        assert_eq!(f, vec![vec!["a", "b"], vec!["1", "2"]]);
    }

    // ── Fechas ──────────────────────────────────────────────────────────────

    #[test]
    fn la_fecha_de_secop_se_queda_en_el_dia() {
        assert_eq!(
            fecha_usable("2024-01-15T00:00:00.000"),
            Some(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap())
        );
        assert_eq!(
            fecha_usable("2026-12-31"),
            Some(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap())
        );
        // Vacía o ilegible NO es un error ni una fecha inventada: es «sin fecha
        // usable» (None), y el contrato entrará sin calendario (directiva 20).
        assert_eq!(fecha_usable("ayer"), None);
        assert_eq!(fecha_usable(""), None);
        assert_eq!(fecha_usable("   "), None);
        // Fuera del rango de trabajo del motor: también None, para no darle una
        // fecha con la que entra en pánico. El control es una fecha real, que sí
        // pasa (la de arriba).
        assert_eq!(fecha_usable("+262142-12-31"), None);
    }

    // ── El cronograma mensual ────────────────────────────────────────────────

    fn d(a: i32, m: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(a, m, dia).unwrap()
    }

    #[test]
    fn mensual_una_fecha_por_mes_anclada_al_dia_del_fin() {
        // Contrato de un año que vence el 29: una fecha por mes, el día 29.
        let v = mensuales(d(2024, 1, 15), d(2024, 12, 29));
        assert_eq!(v.first(), Some(&d(2024, 1, 29)));
        assert_eq!(v.last(), Some(&d(2024, 12, 29)));
        assert_eq!(v.len(), 12);
    }

    /// El caso que un generador ingenuo rompe: ancla en 31 sobre un mes que no
    /// tiene 31. Febrero de un bisiesto es el 29, no un salto ni un pánico.
    #[test]
    fn el_ancla_en_31_se_recorta_a_los_dias_del_mes() {
        let v = mensuales(d(2024, 1, 31), d(2024, 3, 31));
        assert_eq!(v, vec![d(2024, 1, 31), d(2024, 2, 29), d(2024, 3, 31)]);
    }

    #[test]
    fn mensual_dentro_de_un_solo_mes_da_una_fecha() {
        let v = mensuales(d(2024, 5, 3), d(2024, 5, 20));
        assert_eq!(v, vec![d(2024, 5, 20)]);
    }

    // ── Clasificación vivo / histórico ───────────────────────────────────────

    #[test]
    fn en_curso_reconoce_los_estados_de_secop_y_rechaza_los_de_cierre() {
        // Los que SÍ están en curso (con y sin acento, con mayúsculas raras).
        for e in ["En ejecución", "EN EJECUCION", "Modificado", "Aprobado", "aprobado"] {
            assert!(estado_en_curso(e), "«{e}» debería estar en curso");
        }
        // Pareja: los de cierre NO — si esto pasara, un contrato Cerrado con fin
        // futuro fabricaría obligaciones.
        for e in ["Cerrado", "terminado", "Cancelado", "cedido", "Borrador", ""] {
            assert!(!estado_en_curso(e), "«{e}» NO debería estar en curso");
        }
    }

    #[test]
    fn una_parte_sin_definir_no_fabrica_un_stakeholder() {
        assert_eq!(parte_nombrada(Some("Rafael Bernal")).as_deref(), Some("Rafael Bernal"));
        for vacio in ["No definido", "no aplica", "N/A", "   ", ""] {
            assert_eq!(parte_nombrada(Some(vacio)), None, "«{vacio}» no es una persona");
        }
        assert_eq!(parte_nombrada(None), None);
    }
}
