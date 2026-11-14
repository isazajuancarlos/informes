//! Validación del CONTENIDO de un informe recibido, sin I/O.
//!
//! SIGER recibe el archivo de una entrega y hoy lo califica por sus *magic
//! bytes* (`infer`, en `siger-api`). Pero un XBRL/iXBRL es XML: **no tiene firma
//! binaria**, así que esa comprobación sale `None` y el documento entra sin que
//! nadie mire su estructura. Este módulo cierra ese hueco para el formato de
//! reporte estructurado más extendido —XBRL— con **dos piezas puras**:
//!
//! 1. un **lector XML endurecido** ([`xml`]) que es, a propósito, más estricto
//!    que configurar una librería: **rechaza de plano** DTD/`DOCTYPE`,
//!    declaraciones de entidad e instrucciones de proceso, así que no puede
//!    expandir una entidad externa (XXE) ni una recursiva («billion laughs») —
//!    no las tolera, las rechaza—; y acota profundidad, tamaño y número de nodos;
//! 2. un **evaluador de instancia XBRL** ([`evaluar`]) que, sobre ese lector,
//!    extrae hechos/contextos/unidades y comprueba la **integridad referencial**
//!    (todo `contextRef`/`unitRef` apunta a un `context`/`unit` declarado).
//!
//! # Qué NO hace, dicho de frente (directiva 20: no fingir lo que no se mide)
//!
//! No valida la *taxonomía* ni los *linkbases* de cálculo/presentación (eso exige
//! traer los esquemas referenciados, que es otra puerta). **Sí** reconoce iXBRL
//! (Inline XBRL, los hechos embebidos en un XHTML: lo que emite la SEC/EDGAR y el
//! ESEF europeo): la raíz `html` que declara el NS inline se mide con el MISMO
//! evaluador de integridad referencial que una instancia suelta, porque el lector
//! ya recoge hechos/contextos/unidades por su nombre local (D9). Lo que mide es lo
//! que afirma: bien-formación endurecida + estructura de instancia/iXBRL +
//! integridad referencial.
//!
//! # Por qué es un FACTOR, no una compuerta
//!
//! El llamante ([`siger-api`]) usa esto para *calificar* una entrega
//! (`archivo_valido`), nunca para rechazarla: un informe mal formado no puede ser
//! la causa del incumplimiento del cliente ante su ente de control (invariante 2
//! de SIGER). Por eso aquí no hay `Err` que bloquee: se devuelve un veredicto que
//! el llamante registra y marca para revisión.
//!
//! La técnica (hechos/contextos/unidades + integridad) se reimplementó propia a
//! partir del estudio de `crabrl` (AGPL): se toma la técnica, no el código
//! (D9/D38).

/// Hasta dónde se lee antes de declarar el documento hostil. No son gustos: son
/// topes que convierten un documento patológico (anidamiento o tamaño que agota
/// memoria/pila) en un rechazo limpio en vez de un pánico.
#[derive(Clone, Copy, Debug)]
pub struct Limites {
    /// Tamaño máximo del documento en bytes.
    pub max_bytes: usize,
    /// Profundidad máxima de anidamiento de elementos.
    pub max_profundidad: usize,
    /// Número máximo de elementos.
    pub max_nodos: usize,
}

impl Default for Limites {
    fn default() -> Self {
        // 64 MiB cubre un filing XBRL grande con holgura; la defensa de verdad
        // contra la bomba de expansión no es el tamaño sino rechazar entidades.
        Self { max_bytes: 64 * 1024 * 1024, max_profundidad: 256, max_nodos: 2_000_000 }
    }
}

/// Lo que el lector endurecido se niega a procesar. Cada variante es un documento
/// que un XML «normal» aceptaría y que aquí se rechaza por seguridad o por tope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Peligro {
    /// `<!DOCTYPE …>` o cualquier declaración de marcado (`<!ENTITY …>`, etc.).
    /// La puerta de la XXE y de la expansión de entidades: se cierra entera.
    Doctype,
    /// Una referencia a entidad que el lector no honra: ni una de las cinco
    /// predefinidas, ni numérica, ni una del conjunto XHTML respaldada por un DTD
    /// XHTML declarado (`&cualquiera;`, o `&nbsp;` sin el `PUBLIC` que lo define).
    /// Cubre dos cosas a la vez: el vector de la expansión recursiva, y la entidad
    /// con nombre usada sin el DTD que la declara —que un parser XML estricto
    /// también rechaza (diferencial de parser)—.
    Entidad(String),
    /// Una instrucción de proceso (`<?… ?>`) que no es la declaración XML inicial.
    InstruccionProceso,
    /// Anidamiento por encima de [`Limites::max_profundidad`].
    DemasiadoProfundo,
    /// Más elementos que [`Limites::max_nodos`].
    DemasiadosNodos,
    /// Documento por encima de [`Limites::max_bytes`].
    DemasiadoGrande,
}

/// Por qué un documento que SÍ es una instancia XBRL no es válido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MotivoXbrl {
    /// Un hecho referencia por `contextRef` un contexto que no está declarado.
    ContextoHuerfano(String),
    /// Un hecho referencia por `unitRef` una unidad que no está declarada.
    UnidadHuerfana(String),
    /// La instancia no declara ni un solo contexto: no puede sostener ningún
    /// hecho con sentido.
    SinContextos,
}

/// El veredicto sobre el contenido de un archivo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Veredicto {
    /// No se pudo DECODIFICAR (no es UTF-8: p. ej. XML en ISO-8859-1 o UTF-16).
    /// NO es un juicio sobre el contenido — no se leyó. El llamante lo traduce a
    /// «sin opinión», nunca a «inválido» (D20: no se inventa un veredicto que no
    /// se tiene; el contrato de `archivo_valido` reserva eso para `NULL`).
    NoLegible,
    /// Se decodificó, pero no empieza como XML (texto plano, binario con texto…).
    NoEsXml,
    /// El lector endurecido lo rechazó por seguridad o por tope.
    Rechazado(Peligro),
    /// Empieza como XML pero no está bien formado (etiqueta sin cerrar, cierre
    /// que no casa, …). El motivo es para el registro, no para el usuario.
    MalFormado(String),
    /// XML bien formado que NO es una instancia XBRL ni un iXBRL (p. ej. otro XML,
    /// o un XHTML normal sin el espacio de nombres inline).
    XmlNoXbrl,
    /// Instancia XBRL bien formada y con integridad referencial.
    XbrlValido { hechos: usize, contextos: usize, unidades: usize },
    /// Instancia XBRL con uno o más defectos de estructura.
    XbrlInvalido(Vec<MotivoXbrl>),
}

/// El espacio de nombres de una instancia XBRL 2.1.
const NS_INSTANCIA: &str = "http://www.xbrl.org/2003/instance";

/// El espacio de nombres de Inline XBRL 1.1 (2013) — el que usa la SEC (EDGAR) y
/// el formato ESEF europeo. Un documento XHTML que lo declara en su raíz lleva los
/// hechos XBRL EMBEBIDOS (`ix:nonFraction`/`ix:nonNumeric`) con sus contextos y
/// unidades en `ix:resources`.
const NS_INLINE_XBRL_11: &str = "http://www.xbrl.org/2013/inlineXBRL";
/// Inline XBRL 1.0 (2008). Se acepta además de la 1.1 porque hay filings vivos con
/// la versión vieja y marcarlos como «no XBRL» sería un falso negativo (D19/D20).
const NS_INLINE_XBRL_10: &str = "http://www.xbrl.org/2008/inlineXBRL";

/// Evalúa el contenido de un archivo con los [`Limites`] por defecto.
#[must_use]
pub fn evaluar(datos: &[u8]) -> Veredicto {
    evaluar_con(datos, &Limites::default())
}

/// Evalúa el contenido de un archivo con límites explícitos.
#[must_use]
pub fn evaluar_con(datos: &[u8], lim: &Limites) -> Veredicto {
    if datos.len() > lim.max_bytes {
        return Veredicto::Rechazado(Peligro::DemasiadoGrande);
    }
    let Ok(texto) = std::str::from_utf8(datos) else {
        // No es UTF-8 (ISO-8859-1, UTF-16…). XML 1.0 admite otras codificaciones,
        // así que esto NO es «no es XML»: es «no lo pude leer». No se marca como
        // inválido —sería un falso positivo sobre un cliente que cumplió (D18)—;
        // el llamante lo trata como sin opinión.
        return Veredicto::NoLegible;
    };
    match xml::leer(texto, lim) {
        Err(ErrorLectura::Peligro(p)) => Veredicto::Rechazado(p),
        Err(ErrorLectura::NoEsXml) => Veredicto::NoEsXml,
        Err(ErrorLectura::MalFormado(m)) => Veredicto::MalFormado(m),
        Ok(doc) => clasificar(&doc),
    }
}

fn clasificar(doc: &xml::Documento) -> Veredicto {
    // Una instancia XBRL suelta y un iXBRL se validan IGUAL: los dos tienen hechos
    // que referencian contextos/unidades, y el lector los recoge por nombre local
    // sin importar si viven bajo `<xbrl>` o embebidos en un XHTML (D1/D9).
    if !doc.es_instancia_xbrl && !doc.es_ixbrl {
        return Veredicto::XmlNoXbrl;
    }
    let mut motivos = Vec::new();
    if doc.contextos.is_empty() {
        motivos.push(MotivoXbrl::SinContextos);
    }
    for r in &doc.ref_contexto {
        if !doc.contextos.contains(r) {
            motivos.push(MotivoXbrl::ContextoHuerfano(r.clone()));
        }
    }
    for r in &doc.ref_unidad {
        if !doc.unidades.contains(r) {
            motivos.push(MotivoXbrl::UnidadHuerfana(r.clone()));
        }
    }
    if motivos.is_empty() {
        Veredicto::XbrlValido {
            hechos: doc.hechos,
            contextos: doc.contextos.len(),
            unidades: doc.unidades.len(),
        }
    } else {
        Veredicto::XbrlInvalido(motivos)
    }
}

enum ErrorLectura {
    NoEsXml,
    MalFormado(String),
    Peligro(Peligro),
}

/// El lector XML endurecido: un solo paso sobre el texto, sin construir árbol.
mod xml {
    use super::{ErrorLectura, Limites, NS_INLINE_XBRL_10, NS_INLINE_XBRL_11, NS_INSTANCIA, Peligro};
    use std::collections::HashSet;

    /// Lo que el lector extrae en una pasada: lo justo para calificar el
    /// documento, no un DOM.
    pub(super) struct Documento {
        pub(super) es_instancia_xbrl: bool,
        /// La raíz es un XHTML que declara el NS de Inline XBRL: los hechos van
        /// embebidos. Se valida con el mismo evaluador que una instancia suelta.
        pub(super) es_ixbrl: bool,
        pub(super) hechos: usize,
        pub(super) contextos: HashSet<String>,
        pub(super) unidades: HashSet<String>,
        pub(super) ref_contexto: Vec<String>,
        pub(super) ref_unidad: Vec<String>,
    }

    struct Lector<'a> {
        b: &'a [u8],
        i: usize,
        lim: &'a Limites,
        /// Un `<!DOCTYPE` ya consumido: el segundo es marcado fuera de lugar.
        doctype_visto: bool,
        /// El documento declaró un DTD XHTML de la allowlist (`PUBLIC` hacia el
        /// W3C). Ese DTD es el que DEFINE las entidades con nombre del conjunto
        /// XHTML (`&nbsp;`, `&copy;`), así que **solo con él se honran**. Un
        /// DOCTYPE desnudo (`<!DOCTYPE html>`) o ausente no define ninguna —igual
        /// que un parser XML estricto, para el que solo existen las cinco
        /// predefinidas y las numéricas—, de modo que ahí un `&nbsp;` es una
        /// referencia a entidad no declarada. Sin este acople, ese `&nbsp;`
        /// recibía veredicto LIMPIO mientras un consumidor aguas abajo lo
        /// rechazaría: el mismo diferencial de parser que el `SYSTEM` de atacante
        /// (obs conf-7 del concilio 2026-11-10). El DOCTYPE va siempre ANTES de la
        /// raíz, así que esta bandera queda fijada antes de validar la primera
        /// entidad.
        ///
        /// El MODELO DE CONSUMIDOR que esto cubre es el que RESUELVE el DTD
        /// declarado (navegadores, `xmllint` con red, un lector que trae el DTD de
        /// la allowlist): para él, `&nbsp;` con el `PUBLIC` declarado SÍ está
        /// definido y por eso aquí se honra, y sin el `PUBLIC` NO lo está. Un
        /// consumidor MÁS estricto que rehúsa todo DTD (p. ej. ElementTree por
        /// defecto) rechazaría `&nbsp;` incluso con el `PUBLIC`: contra ÉL queda un
        /// diferencial residual que NO se cierra a propósito, porque marcarlo
        /// obligaría a marcar los filings XHTML-1.0 legítimos de EDGAR/ESEF que
        /// usan el DTD (D18). Se elige el modelo del consumidor que resuelve el DTD.
        dtd_define_entidades: bool,
        /// La declaración XML trae `standalone="yes"`. En ese caso, XML 1.0 §4.1
        /// (WFC «Entity Declared») exige que una referencia a entidad declarada
        /// SOLO en el subconjunto EXTERNO —el DTD XHTML del `PUBLIC`— sea un error
        /// FATAL para TODO procesador conforme, descargue o no el DTD. Es decir:
        /// aun con un DTD de la allowlist declarado, un `&nbsp;` bajo
        /// `standalone="yes"` lo rechaza cualquier parser estricto. Por eso, si
        /// esto es `true`, el `PUBLIC` NO habilita las entidades con nombre: el
        /// acople las sigue marcando (hallazgo del concilio 2026-11-12, verificado
        /// contra expat). Se lee en `consumir_declaracion`, antes del DOCTYPE.
        standalone_yes: bool,
    }

    /// Los únicos system-id de DTD externo que se toleran: el conjunto XHTML del
    /// W3C, lo que llevan los filings XHTML reales. Cualquier otra URL se marca
    /// (diferencial de parser: no se le da veredicto limpio a un `SYSTEM`/`PUBLIC`
    /// hacia un host arbitrario que un consumidor aguas abajo podría traer).
    const DTDS_XHTML_W3C: &[&str] = &[
        // `http` y `https`: el W3C sirve ambos y los filings de 2026 ya usan `https`
        // (obs del concilio: marcarlos sería un falso positivo sobre un cliente que
        // cumple, D18). Igualdad EXACTA, no prefijo — `.contains`, no `binary_search`.
        "http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd",
        "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd",
        "http://www.w3.org/TR/xhtml1/DTD/xhtml1-frameset.dtd",
        "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd",
        "https://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd",
        "https://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd",
        "https://www.w3.org/TR/xhtml1/DTD/xhtml1-frameset.dtd",
        "https://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd",
    ];

    pub(super) fn leer(texto: &str, lim: &Limites) -> Result<Documento, ErrorLectura> {
        let mut l = Lector {
            b: texto.as_bytes(),
            i: 0,
            lim,
            doctype_visto: false,
            dtd_define_entidades: false,
            standalone_yes: false,
        };
        l.saltar_bom();
        l.recorrer()
    }

    impl Lector<'_> {
        fn saltar_bom(&mut self) {
            if self.b.starts_with(&[0xEF, 0xBB, 0xBF]) {
                self.i = 3;
            }
        }

        fn saltar_espacios(&mut self) {
            while self.i < self.b.len() && es_espacio(self.b[self.i]) {
                self.i += 1;
            }
        }

        fn recorrer(&mut self) -> Result<Documento, ErrorLectura> {
            let mut doc = Documento {
                es_instancia_xbrl: false,
                es_ixbrl: false,
                hechos: 0,
                contextos: HashSet::new(),
                unidades: HashSet::new(),
                ref_contexto: Vec::new(),
                ref_unidad: Vec::new(),
            };

            self.saltar_espacios();
            // Declaración XML opcional, SOLO al inicio, y SOLO si tras «<?xml»
            // viene espacio o «?»: así «<?xml-stylesheet …?>» NO se confunde con
            // la declaración —cae al bucle, que la rechaza como instrucción de
            // proceso—.
            if self.b[self.i..].starts_with(b"<?xml")
                && matches!(self.b.get(self.i + 5), Some(c) if es_espacio(*c) || *c == b'?')
            {
                self.consumir_declaracion()?;
            }

            let mut pila: Vec<String> = Vec::new();
            let mut raiz_vista = false;
            let mut nodos: usize = 0;

            loop {
                // El texto entre etiquetas se recorre validando sus entidades;
                // fuera de todo elemento (antes de la raíz o tras cerrarla) solo
                // se tolera espacio.
                self.texto(pila.is_empty(), raiz_vista)?;
                if self.i >= self.b.len() {
                    break;
                }
                // self.b[self.i] == b'<'
                let sig = self.b.get(self.i + 1).copied();
                match sig {
                    Some(b'?') => return Err(ErrorLectura::Peligro(Peligro::InstruccionProceso)),
                    Some(b'!') => self.marcado_declarativo(raiz_vista)?,
                    Some(b'/') => {
                        let nombre = self.cerrar()?;
                        match pila.pop() {
                            Some(abierto) if abierto == nombre => {}
                            _ => {
                                return Err(ErrorLectura::MalFormado(format!(
                                    "cierre </{nombre}> sin apertura que case"
                                )))
                            }
                        }
                    }
                    Some(_) => {
                        if raiz_vista && pila.is_empty() {
                            return Err(ErrorLectura::MalFormado(
                                "hay más de un elemento raíz".into(),
                            ));
                        }
                        nodos += 1;
                        if nodos > self.lim.max_nodos {
                            return Err(ErrorLectura::Peligro(Peligro::DemasiadosNodos));
                        }
                        let et = self.abrir(&mut doc, !raiz_vista)?;
                        raiz_vista = true;
                        if !et.autocierre {
                            pila.push(et.nombre);
                            if pila.len() > self.lim.max_profundidad {
                                return Err(ErrorLectura::Peligro(Peligro::DemasiadoProfundo));
                            }
                        }
                    }
                    None => return Err(ErrorLectura::MalFormado("«<» al final del texto".into())),
                }
            }

            if !raiz_vista {
                return Err(ErrorLectura::NoEsXml);
            }
            if !pila.is_empty() {
                return Err(ErrorLectura::MalFormado(format!(
                    "quedaron {} elementos sin cerrar",
                    pila.len()
                )));
            }
            Ok(doc)
        }

        /// Recorre el texto hasta el próximo `<`, validando las referencias de
        /// entidad. Deja `self.i` en el `<` o al final. Si está FUERA de todo
        /// elemento (antes de la raíz o tras cerrarla), un carácter que no sea
        /// espacio es texto suelto ilegal → mal formado (XML 1.0 §2.1).
        fn texto(&mut self, fuera_de_elemento: bool, raiz_vista: bool) -> Result<(), ErrorLectura> {
            while self.i < self.b.len() {
                let c = self.b[self.i];
                if c == b'<' {
                    return Ok(());
                }
                if fuera_de_elemento {
                    if !es_espacio(c) {
                        // Antes de la raíz, texto suelto = no empieza como XML;
                        // tras cerrarla, es contenido ilegal tras la raíz.
                        return Err(if raiz_vista {
                            ErrorLectura::MalFormado("contenido tras el elemento raíz".into())
                        } else {
                            ErrorLectura::NoEsXml
                        });
                    }
                    self.i += 1;
                } else if c == b'&' {
                    self.validar_entidad()?;
                } else {
                    self.i += 1;
                }
            }
            Ok(())
        }

        /// Consume la declaración XML inicial `<?xml … ?>`. Rechaza un `<` dentro
        /// —una declaración no contiene marcado—, para que un `<!DOCTYPE …>`
        /// camuflado entre «<?xml» y «?>» no se trague en silencio. De paso lee el
        /// pseudo-atributo `standalone`: si vale `yes`, lo anota (afecta al acople
        /// de entidades, ver [`Lector::standalone_yes`]).
        fn consumir_declaracion(&mut self) -> Result<(), ErrorLectura> {
            self.i += 5; // «<?xml»
            let inicio = self.i;
            while self.i < self.b.len() {
                if self.b[self.i..].starts_with(b"?>") {
                    self.standalone_yes = declara_standalone_yes(&self.b[inicio..self.i]);
                    self.i += 2;
                    return Ok(());
                }
                if self.b[self.i] == b'<' {
                    return Err(ErrorLectura::MalFormado(
                        "«<» dentro de la declaración XML".into(),
                    ));
                }
                self.i += 1;
            }
            Err(ErrorLectura::MalFormado("declaración XML sin cerrar".into()))
        }

        /// `<!-- … -->`, `<![CDATA[ … ]]>` pasan; cualquier otro `<!…` (DOCTYPE,
        /// ENTITY, …) se RECHAZA: es la puerta de la XXE.
        fn marcado_declarativo(&mut self, raiz_vista: bool) -> Result<(), ErrorLectura> {
            if self.b[self.i..].starts_with(b"<!--") {
                self.i += 4;
                return self.saltar_hasta(b"-->");
            }
            if self.b[self.i..].starts_with(b"<![CDATA[") {
                self.i += 9;
                return self.saltar_hasta(b"]]>");
            }
            if self.b[self.i..].starts_with(b"<!DOCTYPE") {
                // El DOCTYPE solo vale UNA vez y ANTES de la raíz: uno repetido o
                // tras la raíz es marcado fuera de lugar (y una vía de confundir al
                // parseo). Comentarios/CDATA sí van en cualquier parte.
                if raiz_vista || self.doctype_visto {
                    return Err(ErrorLectura::MalFormado(
                        "«<!DOCTYPE» repetido o tras la raíz".into(),
                    ));
                }
                self.doctype_visto = true;
                return self.consumir_doctype();
            }
            Err(ErrorLectura::Peligro(Peligro::Doctype))
        }

        /// Un `<!DOCTYPE …>` se tolera SOLO en dos formas, ambas benignas y las que
        /// llevan los filings XHTML reales (EDGAR/ESEF):
        /// 1. **desnudo** `<!DOCTYPE html>` (sin identificador externo);
        /// 2. `PUBLIC "<fpi>" "<system>"` cuyo **system-id esté en la allowlist de
        ///    DTD XHTML del W3C** ([`DTDS_XHTML_W3C`]).
        ///
        /// Todo lo demás es `Peligro::Doctype`: el **subconjunto interno** `[ … ]`
        /// —donde viven `<!ENTITY …>`: XXE y *billion laughs*—, un `SYSTEM "…"`, y un
        /// `PUBLIC` cuya URL no conozcamos. Motivo (hallazgo del review, 2026-11-10):
        /// aunque ESTE lector no trae el DTD, dar veredicto LIMPIO a un documento con
        /// `SYSTEM "http://atacante/"` es un **diferencial de parser** —un consumidor
        /// aguas abajo que SÍ resuelva el id externo haría XXE/SSRF—, así que esa
        /// señal no se borra: se marca. Deja `self.i` tras el `>`.
        fn consumir_doctype(&mut self) -> Result<(), ErrorLectura> {
            self.i += 9; // «<!DOCTYPE»
            self.saltar_espacios();
            let _nombre = self.leer_nombre(); // el tipo de documento (p. ej. «html»)
            self.saltar_espacios();
            match self.b.get(self.i) {
                None => return Err(ErrorLectura::MalFormado("«<!DOCTYPE» sin «>»".into())),
                Some(b'>') => {
                    self.i += 1;
                    return Ok(()); // desnudo
                }
                // Subconjunto interno (XXE / billion laughs): cerrado.
                Some(b'[') => return Err(ErrorLectura::Peligro(Peligro::Doctype)),
                Some(_) => {}
            }
            // Hay un identificador externo. Solo `PUBLIC` con un system-id de la
            // allowlist XHTML del W3C pasa; `SYSTEM` y las URL desconocidas se marcan.
            let palabra = self.leer_nombre();
            let system = if palabra == "PUBLIC" {
                self.saltar_espacios();
                let _fpi = self.leer_literal()?;
                self.saltar_espacios();
                self.leer_literal()? // el system-id es obligatorio en XHTML PUBLIC
            } else {
                // SYSTEM, o una palabra que no esperamos → marcado.
                return Err(ErrorLectura::Peligro(Peligro::Doctype));
            };
            if !DTDS_XHTML_W3C.contains(&system.as_str()) {
                return Err(ErrorLectura::Peligro(Peligro::Doctype));
            }
            self.saltar_espacios();
            // Tras el id externo no se tolera subconjunto interno ni nada más que `>`.
            match self.b.get(self.i) {
                Some(b'>') => {
                    self.i += 1;
                    // Es un DTD XHTML de la allowlist bien formado: a partir de aquí
                    // se honran las entidades con nombre del conjunto XHTML
                    // (`&nbsp;`, `&copy;`), que ESTE DTD es el que define. Un DOCTYPE
                    // desnudo (la rama de arriba) no las habilita.
                    //
                    // SALVO `standalone="yes"`: ahí XML 1.0 §4.1 obliga a TODO
                    // procesador conforme a rechazar una entidad que solo declara el
                    // subconjunto externo, así que honrarla sería el mismo
                    // diferencial de parser que el acople cierra. Verificado contra
                    // expat (concilio 2026-11-12).
                    self.dtd_define_entidades = !self.standalone_yes;
                    Ok(())
                }
                _ => Err(ErrorLectura::Peligro(Peligro::Doctype)),
            }
        }

        /// Lee un literal `"…"` o `'…'` y devuelve su contenido (sin comillas). Un
        /// valor sin comillas o sin cerrar no es un id externo bien formado → marcado.
        fn leer_literal(&mut self) -> Result<String, ErrorLectura> {
            let q = match self.b.get(self.i) {
                Some(&c) if c == b'"' || c == b'\'' => c,
                _ => return Err(ErrorLectura::Peligro(Peligro::Doctype)),
            };
            self.i += 1;
            let inicio = self.i;
            while self.i < self.b.len() && self.b[self.i] != q {
                self.i += 1;
            }
            if self.i >= self.b.len() {
                return Err(ErrorLectura::MalFormado("literal de «<!DOCTYPE» sin cerrar".into()));
            }
            let v = String::from_utf8_lossy(&self.b[inicio..self.i]).into_owned();
            self.i += 1; // la comilla de cierre
            Ok(v)
        }

        fn saltar_hasta(&mut self, fin: &[u8]) -> Result<(), ErrorLectura> {
            while self.i < self.b.len() {
                if self.b[self.i..].starts_with(fin) {
                    self.i += fin.len();
                    return Ok(());
                }
                self.i += 1;
            }
            Err(ErrorLectura::MalFormado(format!(
                "no se cerró «{}»",
                String::from_utf8_lossy(fin)
            )))
        }

        /// Etiqueta de cierre: `</nombre>`. Devuelve el nombre cualificado.
        fn cerrar(&mut self) -> Result<String, ErrorLectura> {
            self.i += 2; // «</»
            let nombre = self.leer_nombre();
            self.saltar_espacios();
            if self.i >= self.b.len() || self.b[self.i] != b'>' {
                return Err(ErrorLectura::MalFormado(format!("cierre </{nombre}…> sin «>»")));
            }
            self.i += 1;
            Ok(nombre)
        }

        /// Etiqueta de apertura: lee nombre y atributos, alimenta `doc`.
        fn abrir(&mut self, doc: &mut Documento, es_raiz: bool) -> Result<Apertura, ErrorLectura> {
            self.i += 1; // «<»
            let nombre = self.leer_nombre();
            if nombre.is_empty() {
                return Err(ErrorLectura::MalFormado("etiqueta sin nombre".into()));
            }

            let mut id: Option<String> = None;
            let mut ref_ctx: Option<String> = None;
            let mut ref_unidad: Option<String> = None;
            // ¿Esta etiqueta declara el NS de instancia en ALGÚN xmlns —por
            // defecto o ligado a un prefijo—? Solo importa en la raíz.
            let mut declara_instancia = false;
            // ¿Declara el NS de Inline XBRL (1.0 o 1.1)? Marca un XHTML con hechos
            // embebidos. También solo importa en la raíz.
            let mut declara_ixbrl = false;
            let mut autocierre = false;

            loop {
                self.saltar_espacios();
                match self.b.get(self.i) {
                    None => return Err(ErrorLectura::MalFormado("etiqueta sin «>»".into())),
                    Some(b'>') => {
                        self.i += 1;
                        break;
                    }
                    Some(b'/') => {
                        if self.b.get(self.i + 1) == Some(&b'>') {
                            autocierre = true;
                            self.i += 2;
                            break;
                        }
                        return Err(ErrorLectura::MalFormado("«/» suelto en etiqueta".into()));
                    }
                    Some(_) => {
                        let (clave, valor) = self.leer_atributo()?;
                        // Solo nos interesan tres atributos y los xmlns; el resto
                        // se descarta.
                        // «xmlns» exacto o un prefijo `xmlns:foo` — NO un atributo
                        // cualquiera que EMPIECE por «xmlns» (p. ej. `xmlnsfoo`),
                        // que no es una declaración de espacio de nombres (D23).
                        if clave == "xmlns" || clave.starts_with("xmlns:") {
                            if valor == NS_INSTANCIA {
                                declara_instancia = true;
                            } else if valor == NS_INLINE_XBRL_11 || valor == NS_INLINE_XBRL_10 {
                                declara_ixbrl = true;
                            }
                        }
                        match clave.as_str() {
                            "id" => id = Some(valor),
                            "contextRef" => ref_ctx = Some(valor),
                            "unitRef" => ref_unidad = Some(valor),
                            _ => {}
                        }
                    }
                }
            }

            if es_raiz {
                // Una instancia XBRL tiene raíz de nombre local «xbrl» y declara
                // en esa misma raíz el espacio de nombres de instancia —como
                // xmlns por defecto o ligado al prefijo de la raíz (xbrli:)—.
                doc.es_instancia_xbrl = local_de(&nombre) == "xbrl" && declara_instancia;
                // Un iXBRL es un XHTML (raíz local «html») que declara el NS inline.
                // Sus hechos (ix:nonFraction/nonNumeric con contextRef) y sus
                // contextos/unidades (xbrli:context/unit, local «context»/«unit»)
                // ya los recoge el lector abajo igual que en una instancia suelta.
                doc.es_ixbrl = local_de(&nombre) == "html" && declara_ixbrl;
            }

            // Un «hecho» es un elemento con contextRef (XBRL 2.1, §4.6).
            if let Some(r) = ref_ctx {
                doc.hechos += 1;
                doc.ref_contexto.push(r);
            }
            if let Some(r) = ref_unidad {
                doc.ref_unidad.push(r);
            }
            // Declaraciones: <…:context id="…"> y <…:unit id="…">.
            if let Some(idv) = id {
                match local_de(&nombre) {
                    "context" => {
                        doc.contextos.insert(idv);
                    }
                    "unit" => {
                        doc.unidades.insert(idv);
                    }
                    _ => {}
                }
            }

            Ok(Apertura { nombre, autocierre })
        }
    }

    struct Apertura {
        nombre: String,
        autocierre: bool,
    }

    impl Lector<'_> {
        fn leer_nombre(&mut self) -> String {
            let inicio = self.i;
            while self.i < self.b.len() && es_nombre(self.b[self.i]) {
                self.i += 1;
            }
            String::from_utf8_lossy(&self.b[inicio..self.i]).into_owned()
        }

        /// `clave = "valor"` | `clave = 'valor'`. Valida entidades del valor.
        fn leer_atributo(&mut self) -> Result<(String, String), ErrorLectura> {
            let clave = self.leer_nombre();
            if clave.is_empty() {
                return Err(ErrorLectura::MalFormado("atributo sin nombre".into()));
            }
            self.saltar_espacios();
            if self.b.get(self.i) != Some(&b'=') {
                return Err(ErrorLectura::MalFormado(format!("atributo «{clave}» sin «=»")));
            }
            self.i += 1;
            self.saltar_espacios();
            let comilla = match self.b.get(self.i) {
                Some(&c) if c == b'"' || c == b'\'' => c,
                _ => return Err(ErrorLectura::MalFormado(format!("valor de «{clave}» sin comillas"))),
            };
            self.i += 1;
            let inicio = self.i;
            while self.i < self.b.len() && self.b[self.i] != comilla {
                if self.b[self.i] == b'&' {
                    self.validar_entidad()?;
                } else if self.b[self.i] == b'<' {
                    return Err(ErrorLectura::MalFormado("«<» dentro de un atributo".into()));
                } else {
                    self.i += 1;
                }
            }
            if self.i >= self.b.len() {
                return Err(ErrorLectura::MalFormado(format!("valor de «{clave}» sin cerrar")));
            }
            let valor = String::from_utf8_lossy(&self.b[inicio..self.i]).into_owned();
            self.i += 1; // la comilla de cierre
            Ok((clave, valor))
        }

        /// Una referencia de entidad empieza en `&`. Solo se admiten las cinco
        /// predefinidas y las numéricas; cualquier otra es la vía de la
        /// expansión y se RECHAZA. Deja `self.i` tras el `;`.
        fn validar_entidad(&mut self) -> Result<(), ErrorLectura> {
            // self.b[self.i] == b'&'
            let inicio = self.i + 1;
            let mut j = inicio;
            // El `;` está cerca, pero una referencia numérica legítima puede
            // llevar ceros a la izquierda (`&#0000000000065;`), así que el tope
            // es holgado y acotado a la vez: 32 bytes bastan para cualquier
            // numérica real y cortan la búsqueda de un `&` suelto.
            let tope = (inicio + 32).min(self.b.len());
            while j < tope && self.b[j] != b';' {
                j += 1;
            }
            if j >= self.b.len() || self.b[j] != b';' {
                return Err(ErrorLectura::MalFormado("«&» sin «;»".into()));
            }
            let nombre = &self.b[inicio..j];
            let ok = matches!(nombre, b"lt" | b"gt" | b"amp" | b"apos" | b"quot")
                || es_referencia_numerica(nombre)
                // Una entidad XHTML con nombre (`&nbsp;`, `&copy;`) es una
                // referencia a un codepoint FIJO, no una expansión: se admite para
                // no marcar en falso un iXBRL real (D18) — pero SOLO cuando el
                // documento declaró el DTD XHTML que la define (`dtd_define_entidades`),
                // porque sin ese DTD un parser XML estricto la rechaza como entidad
                // no declarada, y darle veredicto limpio sería un diferencial de
                // parser (obs conf-7 del concilio). Las DECLARACIONES de entidad
                // siguen rechazadas: viven en el `[` del DOCTYPE, cerrado.
                || self.dtd_define_entidades && ENTIDADES_XHTML.binary_search(&nombre).is_ok();
            if !ok {
                return Err(ErrorLectura::Peligro(Peligro::Entidad(
                    String::from_utf8_lossy(nombre).into_owned(),
                )));
            }
            self.i = j + 1;
            Ok(())
        }
    }

    /// Las entidades con nombre del conjunto XHTML (lat1 + símbolos + especiales),
    /// SIN las que ya casa el `matches!` (lt/gt/amp/quot). Son REFERENCIAS a
    /// codepoints FIJOS: admitirlas NO abre expansión —se sigue rechazando toda
    /// DECLARACIÓN de entidad (el `[` del subconjunto interno del DOCTYPE)—, y
    /// evitan marcar en falso un iXBRL REAL de EDGAR/ESEF que use `&nbsp;`/`&copy;`
    /// (D18/D19). Pero solo se honran cuando el documento DECLARÓ el DTD XHTML que
    /// las define (ver `Lector::dtd_define_entidades`): sin ese DTD un parser XML
    /// estricto las rechaza, así que aquí también. Un nombre fuera de esta lista,
    /// o uno de ella sin el DTD, es `Peligro::Entidad`. Ordenada por bytes, que es
    /// como compara `binary_search` sobre `&[u8]`.
    #[rustfmt::skip]
    const ENTIDADES_XHTML: &[&[u8]] = &[
    b"AElig", b"Aacute", b"Acirc", b"Agrave", b"Alpha", b"Aring", b"Atilde", b"Auml",
    b"Beta", b"Ccedil", b"Chi", b"Dagger", b"Delta", b"ETH", b"Eacute", b"Ecirc",
    b"Egrave", b"Epsilon", b"Eta", b"Euml", b"Gamma", b"Iacute", b"Icirc", b"Igrave",
    b"Iota", b"Iuml", b"Kappa", b"Lambda", b"Mu", b"Ntilde", b"Nu", b"OElig",
    b"Oacute", b"Ocirc", b"Ograve", b"Omega", b"Omicron", b"Oslash", b"Otilde", b"Ouml",
    b"Phi", b"Pi", b"Prime", b"Psi", b"Rho", b"Scaron", b"Sigma", b"THORN",
    b"Tau", b"Theta", b"Uacute", b"Ucirc", b"Ugrave", b"Upsilon", b"Uuml", b"Xi",
    b"Yacute", b"Yuml", b"Zeta", b"aacute", b"acirc", b"acute", b"aelig", b"agrave",
    b"alefsym", b"alpha", b"and", b"ang", b"aring", b"asymp", b"atilde", b"auml",
    b"bdquo", b"beta", b"brvbar", b"bull", b"cap", b"ccedil", b"cedil", b"cent",
    b"chi", b"circ", b"clubs", b"cong", b"copy", b"crarr", b"cup", b"curren",
    b"dArr", b"dagger", b"darr", b"deg", b"delta", b"diams", b"divide", b"eacute",
    b"ecirc", b"egrave", b"empty", b"emsp", b"ensp", b"epsilon", b"equiv", b"eta",
    b"eth", b"euml", b"euro", b"exist", b"fnof", b"forall", b"frac12", b"frac14",
    b"frac34", b"frasl", b"gamma", b"ge", b"hArr", b"harr", b"hearts", b"hellip",
    b"iacute", b"icirc", b"iexcl", b"igrave", b"image", b"infin", b"int", b"iota",
    b"iquest", b"isin", b"iuml", b"kappa", b"lArr", b"lambda", b"lang", b"laquo",
    b"larr", b"lceil", b"ldquo", b"le", b"lfloor", b"lowast", b"loz", b"lrm",
    b"lsaquo", b"lsquo", b"macr", b"mdash", b"micro", b"middot", b"minus", b"mu",
    b"nabla", b"nbsp", b"ndash", b"ne", b"ni", b"not", b"notin", b"nsub",
    b"ntilde", b"nu", b"oacute", b"ocirc", b"oelig", b"ograve", b"oline", b"omega",
    b"omicron", b"oplus", b"or", b"ordf", b"ordm", b"oslash", b"otilde", b"otimes",
    b"ouml", b"para", b"part", b"permil", b"perp", b"phi", b"pi", b"piv",
    b"plusmn", b"pound", b"prime", b"prod", b"prop", b"psi", b"rArr", b"radic",
    b"rang", b"raquo", b"rarr", b"rceil", b"rdquo", b"real", b"reg", b"rfloor",
    b"rho", b"rlm", b"rsaquo", b"rsquo", b"sbquo", b"scaron", b"sdot", b"sect",
    b"shy", b"sigma", b"sigmaf", b"sim", b"spades", b"sub", b"sube", b"sum",
    b"sup", b"sup1", b"sup2", b"sup3", b"supe", b"szlig", b"tau", b"there4",
    b"theta", b"thetasym", b"thinsp", b"thorn", b"tilde", b"times", b"trade", b"uArr",
    b"uacute", b"uarr", b"ucirc", b"ugrave", b"uml", b"upsih", b"upsilon", b"uuml",
    b"weierp", b"xi", b"yacute", b"yen", b"yuml", b"zeta", b"zwj", b"zwnj",
    ];

    fn es_referencia_numerica(n: &[u8]) -> bool {
        // &#1234; o &#xFF;
        match n {
            [b'#', b'x', resto @ ..] if !resto.is_empty() => {
                resto.iter().all(u8::is_ascii_hexdigit)
            }
            [b'#', resto @ ..] if !resto.is_empty() => resto.iter().all(u8::is_ascii_digit),
            _ => false,
        }
    }

    fn es_espacio(c: u8) -> bool {
        matches!(c, b' ' | b'\t' | b'\r' | b'\n')
    }

    /// ¿La declaración XML (`decl` = lo que va entre `<?xml` y `?>`) trae
    /// `standalone="yes"`? Parseo DIRIGIDO (D17, no regex): busca el pseudo-atributo
    /// `standalone` como TOKEN —precedido por espacio o el inicio, para no casar
    /// dentro de otro nombre o de un valor de `encoding`—, seguido de `=` y un
    /// literal entrecomillado igual a `yes`. Cualquier otra forma (`no`, ausente,
    /// comilla sin cerrar) da `false`: la dirección segura es NO marcar `standalone`
    /// cuando no se está seguro (el peor caso es honrar `&nbsp;` como hoy, no abrir
    /// nada). El XML 1.0 solo admite `yes`/`no` aquí, así que un valor exacto basta.
    fn declara_standalone_yes(decl: &[u8]) -> bool {
        const CLAVE: &[u8] = b"standalone";
        if decl.len() < CLAVE.len() {
            return false;
        }
        for p in 0..=decl.len() - CLAVE.len() {
            if &decl[p..p + CLAVE.len()] != CLAVE {
                continue;
            }
            // Frontera de token a la izquierda: inicio o espacio.
            if p != 0 && !es_espacio(decl[p - 1]) {
                continue;
            }
            let mut j = p + CLAVE.len();
            while j < decl.len() && es_espacio(decl[j]) {
                j += 1;
            }
            if decl.get(j) != Some(&b'=') {
                continue;
            }
            j += 1;
            while j < decl.len() && es_espacio(decl[j]) {
                j += 1;
            }
            let comilla = match decl.get(j) {
                Some(&c) if c == b'"' || c == b'\'' => c,
                _ => continue,
            };
            let val_ini = j + 1;
            if let Some(rel) = decl[val_ini..].iter().position(|&c| c == comilla) {
                return &decl[val_ini..val_ini + rel] == b"yes";
            }
        }
        false
    }

    /// Caracteres admitidos en un nombre cualificado (NCName + «:»). Permisivo a
    /// propósito: un nombre con un byte raro no es un fallo de seguridad, solo
    /// corta el nombre, y el resto del parseo lo caza como mal formado.
    fn es_nombre(c: u8) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b':') || c >= 0x80
    }

    fn local_de(nombre: &str) -> &str {
        nombre.split_once(':').map_or(nombre, |(_, l)| l)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const INSTANCIA_OK: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xbrl xmlns="http://www.xbrl.org/2003/instance">
  <context id="c1"><entity><identifier scheme="s">E</identifier></entity></context>
  <unit id="u1"><measure>iso4217:COP</measure></unit>
  <Ingresos contextRef="c1" unitRef="u1">1000</Ingresos>
</xbrl>"#;

    #[test]
    fn una_instancia_xbrl_bien_formada_es_valida() {
        match evaluar(INSTANCIA_OK.as_bytes()) {
            Veredicto::XbrlValido { hechos, contextos, unidades } => {
                assert_eq!((hechos, contextos, unidades), (1, 1, 1));
            }
            otro => panic!("esperaba XbrlValido, fue {otro:?}"),
        }
    }

    #[test]
    fn con_prefijo_en_la_raiz_tambien_se_reconoce() {
        let doc = r#"<xbrli:xbrl xmlns:xbrli="http://www.xbrl.org/2003/instance">
          <xbrli:context id="c1"/>
          <Caja contextRef="c1">5</Caja>
        </xbrli:xbrl>"#;
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::XbrlValido { .. }));
    }

    // --- La pareja de la integridad referencial (D33) ---

    #[test]
    fn un_hecho_con_contextref_colgante_es_invalido() {
        // El MISMO documento bueno, pero el hecho apunta a un contexto que no existe.
        let malo = INSTANCIA_OK.replace(r#"contextRef="c1""#, r#"contextRef="NO_EXISTE""#);
        match evaluar(malo.as_bytes()) {
            Veredicto::XbrlInvalido(m) => {
                assert!(m.contains(&MotivoXbrl::ContextoHuerfano("NO_EXISTE".into())), "{m:?}");
            }
            otro => panic!("esperaba XbrlInvalido, fue {otro:?}"),
        }
    }

    #[test]
    fn un_unitref_colgante_es_invalido() {
        let malo = INSTANCIA_OK.replace(r#"unitRef="u1""#, r#"unitRef="FANTASMA""#);
        assert!(matches!(
            evaluar(malo.as_bytes()),
            Veredicto::XbrlInvalido(m) if m.contains(&MotivoXbrl::UnidadHuerfana("FANTASMA".into()))
        ));
    }

    // --- El corazón anti-XXE: el caso rojo falla por la vía real (D33) ---

    #[test]
    fn un_doctype_con_entidad_externa_se_rechaza() {
        // XXE clásico: entidad externa que leería /etc/passwd.
        let xxe = r#"<?xml version="1.0"?>
<!DOCTYPE xbrl [ <!ENTITY xxe SYSTEM "file:///etc/passwd"> ]>
<xbrl xmlns="http://www.xbrl.org/2003/instance"><x>&xxe;</x></xbrl>"#;
        assert_eq!(evaluar(xxe.as_bytes()), Veredicto::Rechazado(Peligro::Doctype));
    }

    #[test]
    fn una_entidad_no_predefinida_se_rechaza_aunque_no_haya_doctype() {
        // Sin DOCTYPE, pero refiriendo una entidad personalizada: segunda línea
        // de defensa, por si el DOCTYPE entrara por otra vía.
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance"><x>&bomba;</x></xbrl>"#;
        assert_eq!(
            evaluar(doc.as_bytes()),
            Veredicto::Rechazado(Peligro::Entidad("bomba".into()))
        );
    }

    #[test]
    fn las_entidades_predefinidas_y_numericas_pasan() {
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance">
          <context id="c1"/>
          <N contextRef="c1">a &lt; b &amp; c &#38; d &#x26; e</N>
        </xbrl>"#;
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::XbrlValido { .. }));
    }

    #[test]
    fn una_instruccion_de_proceso_incrustada_se_rechaza() {
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance"><?php system('x'); ?></xbrl>"#;
        assert_eq!(evaluar(doc.as_bytes()), Veredicto::Rechazado(Peligro::InstruccionProceso));
    }

    #[test]
    fn el_anidamiento_profundo_se_corta() {
        let lim = Limites { max_profundidad: 8, ..Limites::default() };
        let mut s = String::from(r#"<xbrl xmlns="http://www.xbrl.org/2003/instance">"#);
        for _ in 0..50 {
            s.push_str("<a>");
        }
        assert_eq!(evaluar_con(s.as_bytes(), &lim), Veredicto::Rechazado(Peligro::DemasiadoProfundo));
    }

    #[test]
    fn el_tamano_desmedido_se_rechaza() {
        let lim = Limites { max_bytes: 16, ..Limites::default() };
        assert_eq!(
            evaluar_con(INSTANCIA_OK.as_bytes(), &lim),
            Veredicto::Rechazado(Peligro::DemasiadoGrande)
        );
    }

    // --- No marcar lo legítimo (D18): el XML que no es XBRL ---

    #[test]
    fn un_xml_bien_formado_que_no_es_xbrl_se_distingue() {
        let doc = r#"<?xml version="1.0"?><factura><total>100</total></factura>"#;
        assert_eq!(evaluar(doc.as_bytes()), Veredicto::XmlNoXbrl);
    }

    // --- iXBRL: hechos embebidos en un XHTML (SEC/EDGAR, ESEF). La pareja D33 ---

    const IXBRL_OK: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:ix="http://www.xbrl.org/2013/inlineXBRL" xmlns:xbrli="http://www.xbrl.org/2003/instance">
<head><title>Reporte</title></head>
<body>
  <div style="display:none">
    <ix:header><ix:resources>
      <xbrli:context id="c1"><xbrli:entity><xbrli:identifier scheme="s">E</xbrli:identifier></xbrli:entity></xbrli:context>
      <xbrli:unit id="u1"><xbrli:measure>iso4217:COP</xbrli:measure></xbrli:unit>
    </ix:resources></ix:header>
  </div>
  <p>Ingresos: <ix:nonFraction name="Ingresos" contextRef="c1" unitRef="u1" decimals="0">1000</ix:nonFraction></p>
</body>
</html>"#;

    #[test]
    fn un_ixbrl_valido_se_reconoce_y_cuenta_sus_hechos() {
        // El hecho embebido (ix:nonFraction) y su contexto/unidad (xbrli:) se miden
        // con el MISMO evaluador que una instancia suelta. Mutante D33: si la
        // detección de iXBRL en la raíz no marcara, esto caería a XmlNoXbrl.
        match evaluar(IXBRL_OK.as_bytes()) {
            Veredicto::XbrlValido { hechos, contextos, unidades } => {
                assert_eq!((hechos, contextos, unidades), (1, 1, 1));
            }
            otro => panic!("esperaba XbrlValido, fue {otro:?}"),
        }
    }

    #[test]
    fn un_ixbrl_con_la_version_vieja_2008_tambien_se_reconoce() {
        let viejo = IXBRL_OK.replace("2013/inlineXBRL", "2008/inlineXBRL");
        assert!(matches!(evaluar(viejo.as_bytes()), Veredicto::XbrlValido { .. }));
    }

    #[test]
    fn un_hecho_ixbrl_con_contexto_colgante_es_invalido() {
        // La pareja roja por la vía real: el hecho embebido apunta a un contexto ausente.
        let malo = IXBRL_OK.replace(r#"contextRef="c1""#, r#"contextRef="NO_EXISTE""#);
        match evaluar(malo.as_bytes()) {
            Veredicto::XbrlInvalido(m) => {
                assert!(m.contains(&MotivoXbrl::ContextoHuerfano("NO_EXISTE".into())), "{m:?}");
            }
            otro => panic!("esperaba XbrlInvalido, fue {otro:?}"),
        }
    }

    #[test]
    fn un_xhtml_normal_sin_el_ns_inline_no_es_ixbrl() {
        // El control que DEBE no ver: un XHTML corriente (sin el NS inline) sigue
        // siendo XmlNoXbrl — es lo que impide que la detección dispare sobre toda
        // página web (D33: el caso que tiene que NO marcar, pareja del de arriba).
        let html = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body><p>hola</p></body></html>"#;
        assert_eq!(evaluar(html.as_bytes()), Veredicto::XmlNoXbrl);
    }

    // --- DOCTYPE desnudo + entidades con nombre: el iXBRL REAL de EDGAR/ESEF.
    //     Condiciones 1 y 2 del concilio: lo que un filing legítimo trae y el
    //     lector rechazaba. La frontera XXE se mantiene (parejas rojas abajo). ---

    #[test]
    fn un_ixbrl_desnudo_con_entidades_numericas_es_valido() {
        // Un iXBRL conforme con `<!DOCTYPE html>` desnudo usa entidades NUMÉRICAS
        // (`&#160;`/`&#169;`) —las con nombre exigen el DTD que las define, así que
        // un desnudo + `&nbsp;` sería XML mal formado (obs del concilio)—. Mutante
        // D33: sin `consumir_doctype` cae a Rechazado(Doctype).
        let doc = format!(
            "<!DOCTYPE html>\n{}",
            IXBRL_OK.replacen("<p>Ingresos:", "<p>Derechos&#160;reservados&#169;2026. Ingresos:", 1)
        );
        match evaluar(doc.as_bytes()) {
            Veredicto::XbrlValido { hechos, contextos, unidades } => {
                assert_eq!((hechos, contextos, unidades), (1, 1, 1));
            }
            otro => panic!("esperaba XbrlValido, fue {otro:?}"),
        }
    }

    #[test]
    fn un_ixbrl_public_con_entidades_con_nombre_es_valido() {
        // Un iXBRL conforme que usa `&nbsp;`/`&copy;` declara el DTD XHTML (PUBLIC de
        // la allowlist) que las define. Ese es el camino legítimo de las entidades
        // con nombre; se mide como XBRL.
        let doc = format!(
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Transitional//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd\">\n{}",
            IXBRL_OK.replacen("<p>Ingresos:", "<p>Derechos&nbsp;reservados&copy;2026. Ingresos:", 1)
        );
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::XbrlValido { .. }));
    }

    #[test]
    fn un_desnudo_con_entidad_con_nombre_se_marca() {
        // El ACOPLE (obs conf-7 del concilio): `&nbsp;` bajo un `<!DOCTYPE html>`
        // DESNUDO —que no declara DTD— es, para un parser XML estricto, una entidad
        // no declarada, así que no se le da veredicto limpio: se marca. Es la pareja
        // ROJA de `un_ixbrl_public_con_entidades_con_nombre_es_valido` (mismo `&nbsp;`,
        // la única diferencia es el DTD declarado), y el mutante D33 es la versión
        // anterior de la regla: sin `dtd_define_entidades` esto caía a XbrlValido
        // (`git show HEAD~1:…/contenido.rs`).
        let doc = format!(
            "<!DOCTYPE html>\n{}",
            IXBRL_OK.replacen("<p>Ingresos:", "<p>Derechos&nbsp;reservados. Ingresos:", 1)
        );
        assert_eq!(
            evaluar(doc.as_bytes()),
            Veredicto::Rechazado(Peligro::Entidad("nbsp".into()))
        );
    }

    #[test]
    fn sin_doctype_una_entidad_con_nombre_se_marca() {
        // Sin DOCTYPE alguno, un `&nbsp;` tampoco está declarado: se marca igual que
        // `&bomba;`. El conjunto XHTML no es un privilegio del nombre, sino del DTD
        // que lo define.
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance"><x>&nbsp;</x></xbrl>"#;
        assert_eq!(
            evaluar(doc.as_bytes()),
            Veredicto::Rechazado(Peligro::Entidad("nbsp".into()))
        );
    }

    #[test]
    fn standalone_yes_no_habilita_las_entidades_con_nombre_ni_con_dtd() {
        // XML 1.0 §4.1 (WFC «Entity Declared»): con `standalone="yes"`, una entidad
        // que solo declara el subconjunto EXTERNO (el DTD XHTML del PUBLIC) es error
        // FATAL para TODO procesador conforme —verificado contra expat: rechaza con
        // `standalone="yes"`, acepta con `="no"`—. Así que el acople la marca AUNQUE
        // el PUBLIC esté declarado. Mutante D33: `= !self.standalone_yes` → `= true`
        // la deja pasar (XbrlValido). Hallazgo del concilio (Fable), 2026-11-12.
        let con_nombre = format!(
            "<?xml version=\"1.0\" standalone=\"yes\"?>\n<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Transitional//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd\">\n{}",
            IXBRL_OK.replacen("<p>Ingresos:", "<p>Derechos&nbsp;reservados. Ingresos:", 1)
        );
        assert_eq!(
            evaluar(con_nombre.as_bytes()),
            Veredicto::Rechazado(Peligro::Entidad("nbsp".into()))
        );
        // Control: el MISMO documento `standalone="yes"` con entidad NUMÉRICA valida
        // (las numéricas no dependen de ningún DTD). Discrimina: lo que marca es la
        // entidad CON NOMBRE bajo standalone, no el standalone en sí.
        let con_numero = format!(
            "<?xml version=\"1.0\" standalone=\"yes\"?>\n<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Transitional//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd\">\n{}",
            IXBRL_OK.replacen("<p>Ingresos:", "<p>Derechos&#160;reservados. Ingresos:", 1)
        );
        assert!(matches!(evaluar(con_numero.as_bytes()), Veredicto::XbrlValido { .. }));
    }

    #[test]
    fn standalone_no_si_honra_las_entidades_con_nombre_con_dtd() {
        // La cara B (D18): `standalone="no"` NO dispara el carve-out, así que un DTD
        // PUBLIC de la allowlist SÍ habilita `&nbsp;`. Confirma que el fix es
        // específico de `yes` y no rompe el camino legítimo de EDGAR/ESEF.
        let doc = format!(
            "<?xml version=\"1.0\" standalone=\"no\"?>\n<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Transitional//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd\">\n{}",
            IXBRL_OK.replacen("<p>Ingresos:", "<p>Derechos&nbsp;reservados. Ingresos:", 1)
        );
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::XbrlValido { .. }));
    }

    #[test]
    fn la_allowlist_de_dtd_casa_exacto_no_por_prefijo() {
        // Un system-id que EMPIEZA con una URL permitida pero sigue (sufijo de
        // atacante) NO pasa: la comparación es igualdad EXACTA, no prefijo.
        let sufijo = r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Strict//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd.evil/x"><html xmlns="http://www.w3.org/1999/xhtml"/>"#;
        assert_eq!(evaluar(sufijo.as_bytes()), Veredicto::Rechazado(Peligro::Doctype));
        // El `https` del mismo DTD SÍ pasa (obs del concilio: el W3C sirve ambos).
        let https = format!(
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Transitional//EN\" \"https://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd\">\n{IXBRL_OK}"
        );
        assert!(matches!(evaluar(https.as_bytes()), Veredicto::XbrlValido { .. }));
    }

    #[test]
    fn un_doctype_html_desnudo_tras_la_declaracion_xml_pasa() {
        // Orden real: `<?xml …?>` PRIMERO, luego `<!DOCTYPE html>`, luego la raíz.
        let doc = INSTANCIA_OK.replacen("?>\n", "?>\n<!DOCTYPE html>\n", 1);
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::XbrlValido { .. }));
    }

    // --- Parejas ROJAS por la vía real: el DOCTYPE peligroso SIGUE cerrado ---

    #[test]
    fn un_doctype_con_subconjunto_interno_se_rechaza() {
        // La declaración de entidad (billion laughs / XXE) vive en el `[…]`: basta el `[`.
        let doc = r#"<!DOCTYPE html [ <!ENTITY lol "lololol"> ]><html xmlns="http://www.w3.org/1999/xhtml"/>"#;
        assert_eq!(evaluar(doc.as_bytes()), Veredicto::Rechazado(Peligro::Doctype));
    }

    #[test]
    fn un_ixbrl_con_doctype_xhtml_1_0_externo_se_tolera() {
        // La forma REAL de EDGAR/ESEF: `<!DOCTYPE html PUBLIC "…" "http://…dtd">`.
        // El id externo es inerte (no se trae), así que un iXBRL con esa cabecera se
        // mide como XBRL en vez de marcarse en falso (condición 1 del concilio).
        let doc = format!(
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Transitional//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd\">\n{IXBRL_OK}"
        );
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::XbrlValido { .. }));
    }

    #[test]
    fn un_doctype_con_system_de_atacante_se_marca() {
        // Diferencial de parser (hallazgo del review 2026-11-10): un `SYSTEM` a un
        // host arbitrario NO recibe veredicto limpio —un consumidor aguas abajo que
        // resolviera el id externo haría XXE/SSRF—, así que se marca.
        let doc = r#"<!DOCTYPE html SYSTEM "http://attacker.invalid/leak"><html xmlns="http://www.w3.org/1999/xhtml"/>"#;
        assert_eq!(evaluar(doc.as_bytes()), Veredicto::Rechazado(Peligro::Doctype));
    }

    #[test]
    fn un_doctype_public_con_url_desconocida_se_marca() {
        // PUBLIC cuyo system-id NO está en la allowlist XHTML del W3C → marcado.
        let doc = r#"<!DOCTYPE html PUBLIC "-//X//DTD//EN" "http://attacker.invalid/x.dtd"><html xmlns="http://www.w3.org/1999/xhtml"/>"#;
        assert_eq!(evaluar(doc.as_bytes()), Veredicto::Rechazado(Peligro::Doctype));
    }

    #[test]
    fn un_dtd_externo_permitido_no_habilita_sus_entidades() {
        // Incluso con un DOCTYPE PUBLIC de la allowlist (tolerado), una entidad que
        // ese DTD definiría (`&xxe;`) NO se honra: no está en el conjunto XHTML →
        // Rechazado(Entidad). El DTD nunca se trae.
        let doc = r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd"><html xmlns="http://www.w3.org/1999/xhtml"><body><p>&xxe;</p></body></html>"#;
        assert_eq!(
            evaluar(doc.as_bytes()),
            Veredicto::Rechazado(Peligro::Entidad("xxe".into()))
        );
    }

    #[test]
    fn un_doctype_repetido_o_tras_la_raiz_es_mal_formado() {
        // Solo UNA vez y ANTES de la raíz (hallazgo del review): un segundo DOCTYPE,
        // o uno tras la raíz, es marcado fuera de lugar.
        let dos = "<!DOCTYPE html>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"/>";
        assert!(matches!(evaluar(dos.as_bytes()), Veredicto::MalFormado(_)));
        let tras = r#"<html xmlns="http://www.w3.org/1999/xhtml"/><!DOCTYPE html>"#;
        assert!(matches!(evaluar(tras.as_bytes()), Veredicto::MalFormado(_)));
    }

    #[test]
    fn una_entidad_con_nombre_fuera_del_conjunto_xhtml_se_rechaza() {
        // La pareja de `&nbsp;`: un nombre que NO está en la lista (lo que un DTD
        // definiría) sigue siendo Peligro::Entidad — la expansión no entra.
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance"><context id="c"/><N contextRef="c">&bombita;</N></xbrl>"#;
        assert_eq!(
            evaluar(doc.as_bytes()),
            Veredicto::Rechazado(Peligro::Entidad("bombita".into()))
        );
    }

    #[test]
    fn un_cdata_con_pinta_de_etiqueta_no_confunde() {
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance">
          <context id="c1"/>
          <Nota contextRef="c1"><![CDATA[ esto <no> es </una> etiqueta & tampoco ]]></Nota>
        </xbrl>"#;
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::XbrlValido { .. }));
    }

    #[test]
    fn lo_que_no_empieza_como_xml_no_es_xml() {
        // Texto plano en UTF-8: se decodificó y no es XML.
        assert_eq!(evaluar(b"esto es texto plano"), Veredicto::NoEsXml);
    }

    // --- B: no-UTF-8 es «no legible», NO «inválido» (D18/D20) ---

    #[test]
    fn un_xml_que_no_es_utf8_es_no_legible_no_invalido() {
        // ISO-8859-1: «<x>ñ</x>» con la ñ como byte 0xF1 (UTF-8 inválido suelto).
        let latin1 = [b'<', b'x', b'>', 0xF1, b'<', b'/', b'x', b'>'];
        assert_eq!(evaluar(&latin1), Veredicto::NoLegible);
        // UTF-16LE con BOM.
        let utf16 = [0xFF, 0xFE, b'<', 0x00, b'x', 0x00];
        assert_eq!(evaluar(&utf16), Veredicto::NoLegible);
    }

    #[test]
    fn una_etiqueta_sin_cerrar_es_mal_formado() {
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance"><abierta></xbrl>"#;
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::MalFormado(_)));
    }

    // --- Parejas rojas que faltaban (D33), halladas por el concilio ---

    #[test]
    fn un_cierre_que_no_casa_la_apertura_es_mal_formado() {
        // Distinto de «sin cerrar»: aquí el cierre existe pero no corresponde.
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance"><a></b></xbrl>"#;
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::MalFormado(_)));
    }

    #[test]
    fn mas_de_un_elemento_raiz_es_mal_formado() {
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance"/><otro/>"#;
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::MalFormado(_)));
    }

    #[test]
    fn un_menor_que_dentro_de_un_atributo_es_mal_formado() {
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance" x="a<b"/>"#;
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::MalFormado(_)));
    }

    #[test]
    fn el_tope_de_nodos_se_aplica() {
        // Es el ÚNICO tope de memoria; antes no tenía prueba (mutante M1).
        let lim = Limites { max_nodos: 3, ..Limites::default() };
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance"><a/><b/><c/><d/></xbrl>"#;
        assert_eq!(evaluar_con(doc.as_bytes(), &lim), Veredicto::Rechazado(Peligro::DemasiadosNodos));
    }

    #[test]
    fn un_bom_al_inicio_se_salta() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(
            br#"<xbrl xmlns="http://www.xbrl.org/2003/instance"><context id="c1"/><N contextRef="c1">1</N></xbrl>"#,
        );
        assert!(matches!(evaluar(&bytes), Veredicto::XbrlValido { .. }));
    }

    #[test]
    fn una_referencia_de_caracter_vacia_no_pasa() {
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance"><context id="c"/><N contextRef="c">&#x;</N></xbrl>"#;
        // No es una numérica válida → no se trata como entidad conocida.
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::Rechazado(Peligro::Entidad(_))));
    }

    #[test]
    fn una_referencia_numerica_con_ceros_a_la_izquierda_pasa() {
        // &#0000065; = 'A'. Falso positivo que el concilio marcó (tope de 12).
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance"><context id="c"/><N contextRef="c">&#0000065;</N></xbrl>"#;
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::XbrlValido { .. }));
    }

    #[test]
    fn una_raiz_xbrl_sin_el_ns_de_instancia_no_es_xbrl() {
        // Nombre correcto, pero sin declarar el espacio de nombres (mutante M4).
        let doc = r#"<xbrl><a/></xbrl>"#;
        assert_eq!(evaluar(doc.as_bytes()), Veredicto::XmlNoXbrl);
    }

    #[test]
    fn un_xmlns_impostor_no_cuenta_como_declaracion() {
        // `xmlnsfoo` NO es una declaración de espacio de nombres (C).
        let doc = r#"<xbrl xmlnsfoo="http://www.xbrl.org/2003/instance"><a/></xbrl>"#;
        assert_eq!(evaluar(doc.as_bytes()), Veredicto::XmlNoXbrl);
    }

    #[test]
    fn una_pi_disfrazada_de_declaracion_se_rechaza() {
        // `<?xml-stylesheet…?>` NO es la declaración → cae como instrucción de proceso.
        let doc = r#"<?xml-stylesheet href="x.xsl"?><xbrl xmlns="http://www.xbrl.org/2003/instance"/>"#;
        assert_eq!(evaluar(doc.as_bytes()), Veredicto::Rechazado(Peligro::InstruccionProceso));
    }

    #[test]
    fn un_doctype_camuflado_en_la_declaracion_se_rechaza() {
        // `<?xml … <!DOCTYPE …> … ?>`: un `<` dentro de la declaración no se traga.
        let doc = "<?xml version=\"1.0\" <!DOCTYPE x [<!ENTITY e SYSTEM \"file:///etc/passwd\">]> ?><xbrl xmlns=\"http://www.xbrl.org/2003/instance\"/>";
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::MalFormado(_)));
    }

    #[test]
    fn texto_suelto_antes_de_la_raiz_no_se_acepta() {
        // Texto antes de la raíz → no empieza como XML (y jamás XbrlValido).
        let doc = r#"hola <xbrl xmlns="http://www.xbrl.org/2003/instance"/>"#;
        assert_eq!(evaluar(doc.as_bytes()), Veredicto::NoEsXml);
    }

    #[test]
    fn texto_suelto_tras_la_raiz_es_mal_formado() {
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance"/>basura"#;
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::MalFormado(_)));
    }

    #[test]
    fn un_comentario_antes_de_la_raiz_si_se_tolera() {
        // D18: el comentario previo es XML legítimo; no se debe marcar.
        let doc = r#"<!-- informe trimestral --><xbrl xmlns="http://www.xbrl.org/2003/instance"><context id="c1"/><N contextRef="c1">1</N></xbrl>"#;
        assert!(matches!(evaluar(doc.as_bytes()), Veredicto::XbrlValido { .. }));
    }

    #[test]
    fn una_instancia_sin_contextos_pero_con_hecho_es_invalida() {
        let doc = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance"><X contextRef="c1">1</X></xbrl>"#;
        match evaluar(doc.as_bytes()) {
            Veredicto::XbrlInvalido(m) => {
                assert!(m.contains(&MotivoXbrl::SinContextos));
                assert!(m.contains(&MotivoXbrl::ContextoHuerfano("c1".into())));
            }
            otro => panic!("esperaba XbrlInvalido, fue {otro:?}"),
        }
    }
}
