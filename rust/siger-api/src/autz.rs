//! Quién puede ver qué. **Un solo sitio, y no hay otro.**
//!
//! # El defecto que este módulo existe para hacer imposible
//!
//! En el sistema anterior la autorización eran dos guardas de página
//! (`authenticated_user`, `admin_only`) y, dentro de cada pantalla, la confianza
//! en que la consulta ya traía solo lo que correspondía. Con pantallas
//! renderizadas en el servidor eso casi funcionaba. Casi:
//!
//!   - el portal de terceros descargaba `fila['url']`, una ruta que **mandaba el
//!     navegador**, así que un tercero externo podía pedir cualquier archivo
//!     legible por el proceso —incluidas las claves del `.env`—;
//!   - la subida escribía en `os.path.join(carpeta, nombre_del_cliente)`, con lo
//!     que un `../..` salía del almacén.
//!
//! Los dos son el mismo error: **un identificador del cliente usado como
//! autoridad**. Al pasar a una API el error deja de ser posible por descuido y
//! pasa a ser posible por omisión — hay que escribir la comprobación en cada
//! extremo, y basta olvidarla una vez.
//!
//! Por eso aquí no se comprueba «en cada extremo»: los extremos no reciben un
//! identificador suelto, reciben un [`AlcanceEfectivo`] que ya sabe qué puede ver
//! este actor, y la consulta se construye **a partir de él**. Un extremo nuevo que
//! se olvide de filtrar no compila, porque no tiene de dónde sacar la lista.
//!
//! # Desde el 2026-08-30: la LÓGICA la pone el motor de Bachué
//!
//! El alcance ya no se deriva a mano aquí: el rol se traduce a CONCESIONES y
//! [`bachue_autz::alcance_de`] las colapsa en el [`AlcanceEfectivo`]. Es la misma
//! decisión que existía —Admin/Supervisor ven todo, Operador lo suyo, Tercero lo de
//! su entidad, y «nada» es un valor— pero ahora respaldada por el motor probado
//! (su banco discriminante, su propiedad anti-divergencia `alcance_de`↔`autorizar`)
//! y compartida con los demás productos, en vez de reescrita. La traducción de un
//! ámbito a su `WHERE` (igualdad de usuario, `EXISTS` de participación) vive en
//! [`predicado`], una sola vez, y se construye con [`AlcanceEfectivo::materializar`]
//! —que imposibilita confundir «todo» (sin filtro) con «nada» (`WHERE 0`)—.
//!
//! Lo que la fusión REGALA: el alcance es ahora multi-ámbito (un `Conjunto`), así
//! que un actor podría alcanzar varias entidades a la vez —hoy cada rol da una, pero
//! el `WHERE` ya las uniría con `OR`— y el **deny-override** del motor está
//! disponible el día que haga falta retraer un permiso (una suspensión).

use serde::Serialize;

pub use bachue_autz::AlcanceEfectivo;

/// Los papeles del sistema. Se leen de la base en cada petición, nunca de la
/// cookie: un rol dentro de la cookie sobrevive a que un administrador lo
/// revoque, y entonces revocarlo no revoca nada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Rol {
    Admin,
    Operador,
    Supervisor,
    /// Parte externa. Es el actor cuyo radio de daño es mayor si algo se escapa,
    /// porque por diseño no pertenece a la organización.
    Tercero,
}

impl Rol {
    pub fn desde_bd(texto: &str) -> Option<Self> {
        match texto.trim().to_uppercase().as_str() {
            "ADMIN" => Some(Rol::Admin),
            "OPERADOR" => Some(Rol::Operador),
            "SUPERVISOR" => Some(Rol::Supervisor),
            "TERCERO" => Some(Rol::Tercero),
            // Un rol que no se reconoce NO cae a «el menos peligroso»: cae a
            // ninguno. Un valor inesperado en esa columna es un dato corrupto o
            // una migración a medias, y adivinar convierte eso en un permiso.
            _ => None,
        }
    }

    /// El texto con el que se guarda. Vive junto a `desde_bd` para que las dos
    /// direcciones no puedan discrepar: el `CHECK` del esquema solo admite estos
    /// cuatro, y un quinto valor escrito aquí dejaría al usuario sin rol
    /// reconocible —o sea, fuera— en la siguiente petición.
    pub fn a_bd(self) -> &'static str {
        match self {
            Rol::Admin => "ADMIN",
            Rol::Operador => "OPERADOR",
            Rol::Supervisor => "SUPERVISOR",
            Rol::Tercero => "TERCERO",
        }
    }
}

/// El ámbito de una entrega para el motor: pertenece a un USUARIO (quien la produce)
/// o se ve por participación de un STAKEHOLDER (una relación N:M). El consumidor
/// —este módulo, en [`predicado`]— traduce cada uno a su `WHERE`: el usuario por
/// igualdad, el stakeholder por un `EXISTS` sobre `participacion`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ambito {
    Usuario(i64),
    Stakeholder(i64),
}

/// Las acciones del dominio de entregas cuyo alcance depende del actor. **`Ver` y
/// `Subir` son distintas a propósito** (directiva 32: ver ≠ hacer): un Tercero VE las
/// entregas de su stakeholder pero NO puede subirlas —recibe, no entrega—. El motor lo
/// modela dando a cada rol distintas concesiones por acción, en vez de un `match` de rol
/// enredado en el manejador de subida. El resto de permisos (administrar) son guardas de
/// rol aparte, no de alcance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accion {
    /// Ver/descargar una entrega. La tienen todos los roles sobre su alcance.
    VerEntregas,
    /// Registrar (subir) una entrega. NO la tiene el Tercero: ve pero no entrega.
    SubirEntrega,
}

/// Quién pide. Se construye en el extractor, a partir de la cookie verificada y
/// de una lectura fresca de la base.
#[derive(Debug, Clone, Serialize)]
pub struct Actor {
    pub id: i64,
    pub nombre: String,
    pub email: String,
    pub rol: Rol,
    /// Solo los terceros lo tienen: la entidad externa a la que representan.
    pub stakeholder_id: Option<i64>,
}

impl Actor {
    /// Las CONCESIONES de este actor para el motor, derivadas del rol. Es la misma
    /// política de siempre, expresada como lo que el motor entiende: Admin y
    /// Supervisor tienen la acción global; Operador, sobre su propio usuario;
    /// Tercero, sobre su stakeholder (o ninguna concesión si no tiene entidad —lo
    /// que el motor colapsa a «no ve nada»—).
    fn concesiones(&self) -> Vec<bachue_autz::Concesion<Ambito, Accion>> {
        use bachue_autz::{Alcance, Concesion};
        let con = |accion, alcance| Concesion { accion, alcance };
        match self.rol {
            // Admin y Supervisor ven Y suben cualquier entrega.
            Rol::Admin | Rol::Supervisor => vec![
                con(Accion::VerEntregas, Alcance::Todo),
                con(Accion::SubirEntrega, Alcance::Todo),
            ],
            // Operador ve y sube lo suyo.
            Rol::Operador => vec![
                con(Accion::VerEntregas, Alcance::De(Ambito::Usuario(self.id))),
                con(Accion::SubirEntrega, Alcance::De(Ambito::Usuario(self.id))),
            ],
            // Tercero VE lo de su stakeholder pero NO tiene `SubirEntrega`: recibe, no
            // entrega. Sin entidad vinculada no ve nada (sin concesiones, el motor lo
            // colapsa a `Ninguno` —lo que antes pintaba un aviso y seguía—).
            Rol::Tercero => match self.stakeholder_id {
                Some(id) => vec![con(Accion::VerEntregas, Alcance::De(Ambito::Stakeholder(id)))],
                None => Vec::new(),
            },
        }
    }

    /// El alcance de este actor para una `accion`, **según el motor de Bachué**. `Todo`
    /// (admin/supervisión), `Conjunto` de ámbitos, o `Ninguno`. Es el único origen del
    /// `WHERE`. Para `SubirEntrega`, un Tercero da `Ninguno` (no entrega) aunque para
    /// `VerEntregas` alcance su stakeholder.
    pub fn alcance(&self, accion: Accion) -> AlcanceEfectivo<Ambito> {
        let actor = bachue_autz::Actor::nuevo(self.id, self.concesiones());
        bachue_autz::alcance_de(&actor, &accion)
    }
}

/// El predicado de propiedad para una consulta de entregas, y sus bindings. Es la
/// traducción ámbito→`WHERE` que los cinco sitios repetían a mano; ahora vive una
/// sola vez y se construye con [`AlcanceEfectivo::materializar`] (que garantiza no
/// confundir «todo» con «nada»).
pub enum Predicado {
    /// El actor lo ve todo: no se añade `WHERE` de propiedad.
    SinFiltro,
    /// Filtra por estos ámbitos, con estos bindings en orden.
    Filtro { sql: String, binds: Vec<i64> },
    /// El actor no ve nada: la consulta no debe devolver filas (o el sitio corta
    /// antes, según su caso —auditar y 404, `WHERE 0`, `return false`—).
    Nada,
}

impl Actor {
    /// Materializa el alcance de este actor en un [`Predicado`]. `alias` es el
    /// prefijo de la tabla entregas en la consulta (`"e."` o `""`), e `informe_ref`
    /// cómo se nombra el `informe_id` para el `EXISTS` de stakeholder (`"i.id"`
    /// cuando hay `JOIN informes i`, o `"e.informe_id"`/`"informe_id"` si no).
    pub fn predicado(&self, accion: Accion, alias: &str, informe_ref: &str) -> Predicado {
        let de_ambito = |a: &Ambito| -> (String, i64) {
            match a {
                Ambito::Usuario(id) => (format!("{alias}usuario_id = ?"), *id),
                Ambito::Stakeholder(id) => (
                    format!(
                        "EXISTS (SELECT 1 FROM participacion p \
                         WHERE p.informe_id = {informe_ref} AND p.stakeholder_id = ?)"
                    ),
                    *id,
                ),
            }
        };
        let unir = |ambitos: &[Ambito], negado: bool| -> Predicado {
            let mut sqls = Vec::new();
            let mut binds = Vec::new();
            for a in ambitos {
                let (s, b) = de_ambito(a);
                sqls.push(s);
                binds.push(b);
            }
            let junto = sqls.join(" OR ");
            let sql = if negado { format!("NOT ({junto})") } else { junto };
            Predicado::Filtro { sql, binds }
        };
        self.alcance(accion).materializar(
            || Predicado::SinFiltro,
            |ambitos| unir(ambitos, false),
            |ambitos| unir(ambitos, true),
            || Predicado::Nada,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(rol: Rol, stakeholder: Option<i64>) -> Actor {
        Actor {
            id: 7,
            nombre: "Prueba".into(),
            email: "p@co.co".into(),
            rol,
            stakeholder_id: stakeholder,
        }
    }

    /// El alcance sale del motor y reproduce la política de siempre: el operador ve
    /// solo lo suyo (un `Conjunto` con su usuario).
    #[test]
    fn el_operador_solo_alcanza_lo_suyo() {
        assert_eq!(
            actor(Rol::Operador, None).alcance(Accion::VerEntregas),
            AlcanceEfectivo::Conjunto(vec![Ambito::Usuario(7)])
        );
    }

    #[test]
    fn admin_y_supervisor_ven_todo() {
        assert_eq!(actor(Rol::Admin, None).alcance(Accion::VerEntregas), AlcanceEfectivo::Todo);
        assert_eq!(actor(Rol::Supervisor, None).alcance(Accion::VerEntregas), AlcanceEfectivo::Todo);
    }

    #[test]
    fn un_tercero_sin_entidad_no_alcanza_nada() {
        // Es el caso que en el sistema anterior pintaba un aviso y continuaba.
        assert_eq!(actor(Rol::Tercero, None).alcance(Accion::VerEntregas), AlcanceEfectivo::Ninguno);
        assert_eq!(
            actor(Rol::Tercero, Some(3)).alcance(Accion::VerEntregas),
            AlcanceEfectivo::Conjunto(vec![Ambito::Stakeholder(3)])
        );
    }

    /// **VER ≠ SUBIR (directiva 32): un Tercero VE lo de su stakeholder pero NO lo sube.**
    /// Para `VerEntregas` alcanza su stakeholder; para `SubirEntrega`, `Ninguno`. Su
    /// pareja: el Operador sí sube lo suyo. Sin esta distinción, «también lo ve» se
    /// confundiría con «puede subirlo».
    #[test]
    fn el_tercero_ve_pero_no_sube() {
        let t = actor(Rol::Tercero, Some(3));
        assert_eq!(t.alcance(Accion::VerEntregas), AlcanceEfectivo::Conjunto(vec![Ambito::Stakeholder(3)]));
        assert_eq!(t.alcance(Accion::SubirEntrega), AlcanceEfectivo::Ninguno, "un tercero no entrega");
        // El operador SÍ sube lo suyo.
        assert_eq!(
            actor(Rol::Operador, None).alcance(Accion::SubirEntrega),
            AlcanceEfectivo::Conjunto(vec![Ambito::Usuario(7)])
        );
    }

    #[test]
    fn un_rol_desconocido_no_es_un_rol() {
        assert!(Rol::desde_bd("SUPERUSUARIO").is_none());
        assert!(Rol::desde_bd("").is_none());
    }

    /// **El predicado reproduce el `WHERE` de los cinco sitios, ahora desde una sola
    /// función.** Admin → sin filtro; Operador → igualdad de usuario; Tercero → el
    /// `EXISTS` de participación; Tercero-sin-entidad → nada. Con el `alias` e
    /// `informe_ref` que cada sitio pasa.
    #[test]
    fn el_predicado_reproduce_el_where_de_cada_rol() {
        match actor(Rol::Admin, None).predicado(Accion::VerEntregas, "e.", "i.id") {
            Predicado::SinFiltro => {}
            _ => panic!("admin no filtra"),
        }
        match actor(Rol::Operador, None).predicado(Accion::VerEntregas, "e.", "i.id") {
            Predicado::Filtro { sql, binds } => {
                assert_eq!(sql, "e.usuario_id = ?");
                assert_eq!(binds, vec![7]);
            }
            _ => panic!("operador filtra por su usuario"),
        }
        match actor(Rol::Tercero, Some(3)).predicado(Accion::VerEntregas, "e.", "e.informe_id") {
            Predicado::Filtro { sql, binds } => {
                assert!(sql.contains("EXISTS"));
                assert!(sql.contains("p.informe_id = e.informe_id"));
                assert_eq!(binds, vec![3]);
            }
            _ => panic!("tercero filtra por participación"),
        }
        match actor(Rol::Tercero, None).predicado(Accion::VerEntregas, "e.", "i.id") {
            Predicado::Nada => {}
            _ => panic!("tercero sin entidad no ve nada"),
        }
        // Y SUBIR: el tercero (que sí ve) da Nada al intentar subir.
        match actor(Rol::Tercero, Some(3)).predicado(Accion::SubirEntrega, "", "informe_id") {
            Predicado::Nada => {}
            _ => panic!("el tercero no sube, aunque vea"),
        }
    }
}
