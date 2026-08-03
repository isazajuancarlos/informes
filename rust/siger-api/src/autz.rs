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
//! identificador suelto, reciben un [`Alcance`] que ya sabe qué puede ver este
//! actor, y la consulta se construye **a partir de él**. Un extremo nuevo que se
//! olvide de filtrar no compila, porque no tiene de dónde sacar la lista.

use serde::Serialize;

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

/// Qué entregas puede ver este actor. **No es un filtro sugerido: es el único
/// origen del `WHERE`.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Alcance {
    /// Todas. Solo administración y supervisión.
    Todas,
    /// Las asignadas a esta persona.
    DeUsuario(i64),
    /// Las de los informes en los que esta entidad externa es parte.
    DeStakeholder(i64),
    /// Ninguna. Existe para que «no le corresponde nada» sea un valor legítimo
    /// y no un `Option` que alguien desenvuelva con un `unwrap_or(todas)`.
    Ninguna,
}

impl Actor {
    pub fn alcance(&self) -> Alcance {
        match self.rol {
            Rol::Admin | Rol::Supervisor => Alcance::Todas,
            Rol::Operador => Alcance::DeUsuario(self.id),
            Rol::Tercero => match self.stakeholder_id {
                Some(id) => Alcance::DeStakeholder(id),
                // Un tercero sin entidad vinculada no ve nada. Antes esto
                // pintaba un mensaje y seguía; aquí es un alcance vacío, que es
                // lo mismo que dice el mensaje pero sin depender de que alguien
                // lo lea.
                None => Alcance::Ninguna,
            },
        }
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

    #[test]
    fn el_operador_solo_alcanza_lo_suyo() {
        assert_eq!(actor(Rol::Operador, None).alcance(), Alcance::DeUsuario(7));
    }

    #[test]
    fn un_tercero_sin_entidad_no_alcanza_nada() {
        // Es el caso que en el sistema anterior pintaba un aviso y continuaba.
        assert_eq!(actor(Rol::Tercero, None).alcance(), Alcance::Ninguna);
        assert_eq!(actor(Rol::Tercero, Some(3)).alcance(), Alcance::DeStakeholder(3));
    }

    #[test]
    fn un_rol_desconocido_no_es_un_rol() {
        assert!(Rol::desde_bd("SUPERUSUARIO").is_none());
        assert!(Rol::desde_bd("").is_none());
    }
}
