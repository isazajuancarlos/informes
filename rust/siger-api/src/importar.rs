//! Volcado de la base del sistema anterior (SQLAlchemy) al esquema nuevo.
//!
//! Es un subcomando del MISMO binario, no un script aparte, por la razón de
//! siempre: la base destino se crea y se migra con el `sqlx::migrate!` que después
//! sirve el servidor, así que el volcado no puede escribir sobre un esquema
//! distinto del que la aplicación espera. Un script en otro lenguaje tendría su
//! propia copia del esquema y el día que divergieran nadie lo notaría.
//!
//! # Principio
//!
//! Falla RUIDOSAMENTE. Un dato que no encaja en el esquema nuevo —un rol fuera
//! del conjunto, una recurrencia ilegible, una fecha que no parsea— detiene el
//! volcado entero nombrando la fila. No hay valor de relleno: un dato inventado
//! produce un sistema que *parece* migrado, y eso es peor que uno que se cae,
//! porque la caída se nota hoy y el dato falso no se nota nunca.
//!
//! El volcado exige una base destino VACÍA. Nunca mezcla con datos existentes:
//! el alcance de lo que escribe tiene que ser exactamente lo que lee.

use std::path::Path;
use std::str::FromStr;

use chrono::NaiveDate;
use siger_core::{Regla, ReglaJson};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::Row;

#[derive(Debug, thiserror::Error)]
pub enum ErrorImportar {
    #[error("no se pudo abrir la base de origen «{0}»: {1}")]
    Origen(String, sqlx::Error),
    #[error("no se pudo preparar la base de destino: {0}")]
    Destino(#[from] crate::db::ErrorBd),
    #[error(transparent)]
    Bd(#[from] sqlx::Error),
    #[error(
        "la base de destino ya tiene datos ({0}); el volcado exige una base nueva \
         para no mezclar dos orígenes. Borre el archivo destino y reintente."
    )]
    DestinoNoVacio(String),
    #[error("usuario id={0} ({1}): rol_sistema «{2}» no es de este sistema (ADMIN|OPERADOR|SUPERVISOR|TERCERO)")]
    RolUsuario(i64, String, String),
    #[error("participación id={0}: rol «{1}» no es del flujo (ORIGEN|DESTINO|USUARIO|SUPERVISA|EJECUTA)")]
    RolParticipacion(i64, String),
    #[error("entrega id={0}: estado «{1}» no es de este sistema (PENDIENTE|CUMPLIDO|EXTEMPORANEO|INCUMPLIDO)")]
    Estado(i64, String),
    #[error("entrega id={0}: tipo_ajuste «{1}» no es de este sistema (NINGUNO|ANTICIPADO|GRACIA|OMITIR)")]
    Ajuste(i64, String),
    #[error("entrega id={0}: {1} «{2}» no es una fecha válida")]
    Fecha(i64, &'static str, String),
    #[error("entrega id={0}: informe_id es NULL; una entrega sin informe no tiene sentido en el esquema nuevo")]
    EntregaSinInforme(i64),
    #[error("informe id={0} ({1}): la recurrencia guardada no se puede leer: {2}")]
    Recurrencia(i64, String, String),
    #[error(
        "usuario id={0} ({1}): la contraseña guardada no la reconoce el sistema nuevo ({2}). \
         El sistema anterior rehasheaba a Argon2 en cada acceso correcto, así que un hash de \
         otro esquema es de alguien que no ha entrado desde entonces. Copiarlo lo dejaría fuera \
         PARA SIEMPRE y sin señal: el login responde a un hash ilegible exactamente lo mismo que \
         a una contraseña mala. Reponga esa contraseña en el origen y reintente el volcado."
    )]
    ClaveIlegible(i64, String, String),
}

/// Lo copiado, por tabla. Se imprime al terminar y se compara contra el origen.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Resumen {
    pub usuarios: u64,
    pub stakeholders: u64,
    pub informes: u64,
    pub participacion: u64,
    pub entregas: u64,
}

const ROLES_USUARIO: &[&str] = &["ADMIN", "OPERADOR", "SUPERVISOR", "TERCERO"];
const ROLES_PARTICIPACION: &[&str] = &["ORIGEN", "DESTINO", "USUARIO", "SUPERVISA", "EJECUTA"];
const ESTADOS: &[&str] = &["PENDIENTE", "CUMPLIDO", "EXTEMPORANEO", "INCUMPLIDO"];
const AJUSTES: &[&str] = &["NINGUNO", "ANTICIPADO", "GRACIA", "OMITIR"];

/// El rol del sistema nuevo a partir de los dos campos del viejo. `es_admin`
/// manda: un administrador es ADMIN aunque su `rol_sistema` diga otra cosa. Fuera
/// de eso, el `rol_sistema` tiene que ser uno de los conocidos —no hay «en
/// cualquier otro caso» que lo mande a un rol por defecto—.
fn rol_usuario(id: i64, email: &str, es_admin: i64, rol_sistema: Option<&str>) -> Result<String, ErrorImportar> {
    if es_admin != 0 {
        return Ok("ADMIN".to_string());
    }
    let r = rol_sistema.unwrap_or("").trim().to_uppercase();
    if ROLES_USUARIO.contains(&r.as_str()) {
        Ok(r)
    } else {
        Err(ErrorImportar::RolUsuario(id, email.to_string(), r))
    }
}

/// El hash de la contraseña, comprobado contra el mismo reconocedor que usará el
/// login.
///
/// Es el único campo que el volcado copiaba verbatim sobre una suposición —«los
/// hashes ya son Argon2 en el origen»— que nadie comprobaba. Si es falsa para una
/// fila, esa cuenta no vuelve a entrar JAMÁS y nadie se entera: el login no
/// reconoce el formato, así que responde «credenciales incorrectas», que es
/// palabra por palabra lo que responde a una contraseña mala. El usuario cambia
/// una contraseña que ya era correcta y sigue sin entrar.
///
/// Y el log tampoco lo salva del todo. `rutas.rs` deja el aviso «el hash del
/// usuario N no lo reconoce ningún esquema» solo cuando falla `PasswordHash::new`;
/// un hash de otro esquema que SÍ es PHC válido —`$scrypt$…`— atraviesa ese
/// parseo, muere después en `verify_password` y no escribe nada en ninguna parte.
///
/// No se re-hashea (no tenemos la contraseña) ni se verifica de verdad (haría
/// falta la contraseña, y correr el KDF por usuario cuesta 45 ms de los suyos).
/// Se comprueba EXACTAMENTE lo que el login comprueba antes de comparar: que el
/// PHC parsea, que el algoritmo es de la familia Argon2, y que sus parámetros se
/// pueden leer. Lo que pasa por aquí, el login lo entiende.
///
/// La familia entera —`argon2i`, `argon2d`, `argon2id`—, no solo el `argon2id`
/// que emite hoy `hashear`: el origen es passlib y no siempre eligió la misma
/// variante. Rechazar una que el verificador acepta dejaría fuera a un usuario
/// legítimo, que es el error contrario y peor.
fn clave_argon2(id: i64, email: &str, hash: &str) -> Result<String, ErrorImportar> {
    let no = |motivo: String| ErrorImportar::ClaveIlegible(id, email.to_string(), motivo);
    let phc = argon2::PasswordHash::new(hash)
        .map_err(|e| no(format!("no es un hash PHC legible: {e}")))?;
    argon2::Algorithm::try_from(phc.algorithm)
        .map_err(|_| no(format!("el algoritmo «{}» no es de la familia Argon2", phc.algorithm)))?;
    argon2::Params::try_from(&phc)
        .map_err(|e| no(format!("los parámetros del Argon2 no se pueden leer: {e}")))?;
    Ok(hash.to_string())
}

/// La fecha SOLA, en `%Y-%m-%d`. El sistema viejo guardaba un `datetime`
/// (`2026-01-01 23:59:59.000000`); el nuevo identifica la entrega por el DÍA
/// nominal. Copiar el datetime verbatim rompería el planificador —que parsea con
/// `%Y-%m-%d` y compara por igualdad de cadena—, y regeneraría entregas duplicadas
/// que el `UNIQUE` no atraparía porque «…23:59:59» no es igual a «…».
fn solo_fecha(id: i64, campo: &'static str, s: &str) -> Result<String, ErrorImportar> {
    let fecha = s.trim().split(['T', ' ']).next().unwrap_or("").trim();
    NaiveDate::parse_from_str(fecha, "%Y-%m-%d")
        .map(|d| d.to_string()) // NaiveDate::to_string ⇒ %Y-%m-%d
        .map_err(|_| ErrorImportar::Fecha(id, campo, s.to_string()))
}

/// El NOMBRE del archivo, no su ruta: el esquema nuevo guarda solo el nombre y
/// construye la ruta desde el id. Si el origen trae una ruta, nos quedamos con su
/// último segmento.
fn nombre_archivo(ruta: Option<&str>) -> Option<String> {
    ruta.and_then(|r| Path::new(r).file_name().map(|s| s.to_string_lossy().into_owned()))
}

async fn abrir_origen(ruta: &str) -> Result<SqlitePool, ErrorImportar> {
    let opciones = SqliteConnectOptions::from_str(&format!("sqlite://{ruta}"))
        .map_err(|e| ErrorImportar::Origen(ruta.to_string(), e))?
        .read_only(true);
    SqlitePoolOptions::new()
        .connect_with(opciones)
        .await
        .map_err(|e| ErrorImportar::Origen(ruta.to_string(), e))
}

async fn destino_vacio(bd: &SqlitePool) -> Result<(), ErrorImportar> {
    for tabla in ["usuarios", "informes", "participacion", "entregas", "stakeholders"] {
        let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {tabla}"))
            .fetch_one(bd)
            .await?;
        if n > 0 {
            return Err(ErrorImportar::DestinoNoVacio(format!("{n} filas en {tabla}")));
        }
    }
    Ok(())
}

/// Ejecuta el volcado completo dentro de una sola transacción: o entra todo, o no
/// entra nada. Una migración a medias es la peor de las salidas.
pub async fn importar(origen: &str, destino: &str) -> Result<Resumen, ErrorImportar> {
    let src = abrir_origen(origen).await?;
    let dst = crate::db::abrir(&format!("sqlite://{destino}?mode=rwc")).await?;
    destino_vacio(&dst).await?;

    let mut r = Resumen::default();
    let mut tx = dst.begin().await?;

    // 1) usuarios. El hash se copia tal cual —no tenemos la contraseña para
    //    re-hashear— pero antes se COMPRUEBA que el sistema nuevo lo reconozca.
    //    Copiar uno que no reconoce deja a esa persona fuera para siempre y sin
    //    señal; ver `clave_argon2`. Es el mismo motivo por el que la recurrencia
    //    se valida tres bloques más abajo: el fallo aparecería lejos del volcado.
    for fila in sqlx::query(
        "SELECT id, nombre, email, password_hash, es_admin, rol_sistema, activo FROM usuarios ORDER BY id",
    )
    .fetch_all(&src)
    .await?
    {
        let id: i64 = fila.get("id");
        let email: String = fila.get("email");
        let rol = rol_usuario(id, &email, fila.get("es_admin"), fila.get::<Option<String>, _>("rol_sistema").as_deref())?;
        let clave = clave_argon2(id, &email, &fila.get::<String, _>("password_hash"))?;
        sqlx::query(
            "INSERT INTO usuarios (id, nombre, email, clave_hash, rol, activo, stakeholder_id) \
             VALUES (?, ?, ?, ?, ?, ?, NULL)",
        )
        .bind(id)
        .bind(fila.get::<String, _>("nombre"))
        .bind(email)
        .bind(clave)
        .bind(rol)
        .bind(fila.get::<i64, _>("activo"))
        .execute(&mut *tx)
        .await?;
        r.usuarios += 1;
    }

    // 2) stakeholders. El origen no marca interno/externo; que tenga un usuario
    //    asociado es lo más cercano a «interno». (Hoy son cero filas; el mapeo
    //    existe para no fallar si mañana no lo son.)
    for fila in sqlx::query("SELECT id, nombre, email, usuario_id FROM stakeholders ORDER BY id")
        .fetch_all(&src)
        .await?
    {
        let externo = if fila.get::<Option<i64>, _>("usuario_id").is_some() { 0 } else { 1 };
        sqlx::query("INSERT INTO stakeholders (id, nombre, externo, email) VALUES (?, ?, ?, ?)")
            .bind(fila.get::<i64, _>("id"))
            .bind(fila.get::<String, _>("nombre"))
            .bind(externo)
            .bind(fila.get::<Option<String>, _>("email"))
            .execute(&mut *tx)
            .await?;
        r.stakeholders += 1;
    }

    // 3) informes. La recurrencia se copia verbatim —el esquema la guarda en el
    //    mismo formato—, pero antes se VALIDA que se pueda leer: un informe cuya
    //    regla no parsea no generaría entregas nunca, y el fallo aparecería lejos.
    for fila in sqlx::query(
        "SELECT id, nombre, descripcion, recurrencia_json, activo FROM cat_informes ORDER BY id",
    )
    .fetch_all(&src)
    .await?
    {
        let id: i64 = fila.get("id");
        let nombre: String = fila.get("nombre");
        let recurrencia: String = fila.get::<Option<String>, _>("recurrencia_json").unwrap_or_else(|| "{}".to_string());
        // Validación: parsea como ReglaJson Y se traduce a una Regla del motor.
        let rj: ReglaJson = serde_json::from_str(&recurrencia)
            .map_err(|e| ErrorImportar::Recurrencia(id, nombre.clone(), e.to_string()))?;
        Regla::try_from(&rj).map_err(|e| ErrorImportar::Recurrencia(id, nombre.clone(), e.to_string()))?;
        sqlx::query(
            "INSERT INTO informes (id, nombre, descripcion, recurrencia, activo) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(nombre)
        .bind(fila.get::<Option<String>, _>("descripcion"))
        .bind(recurrencia)
        .bind(fila.get::<i64, _>("activo"))
        .execute(&mut *tx)
        .await?;
        r.informes += 1;
    }

    // 4) participación.
    for fila in sqlx::query(
        "SELECT id, informe_id, usuario_id, stakeholder_id, rol, es_principal, orden \
         FROM participacion_informe ORDER BY id",
    )
    .fetch_all(&src)
    .await?
    {
        let id: i64 = fila.get("id");
        let rol: String = fila.get::<String, _>("rol").trim().to_uppercase();
        if !ROLES_PARTICIPACION.contains(&rol.as_str()) {
            return Err(ErrorImportar::RolParticipacion(id, rol));
        }
        sqlx::query(
            "INSERT INTO participacion (id, informe_id, usuario_id, stakeholder_id, rol, es_principal, orden) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(fila.get::<i64, _>("informe_id"))
        .bind(fila.get::<Option<i64>, _>("usuario_id"))
        .bind(fila.get::<Option<i64>, _>("stakeholder_id"))
        .bind(rol)
        .bind(fila.get::<Option<i64>, _>("es_principal").unwrap_or(1))
        .bind(fila.get::<Option<i64>, _>("orden").unwrap_or(0))
        .execute(&mut *tx)
        .await?;
        r.participacion += 1;
    }

    // 5) entregas. Se preserva la historia calculada por el sistema anterior
    //    (fechas normalizadas al día), para que el planificador CONTINÚE desde
    //    donde quedó y no recompute el pasado.
    for fila in sqlx::query(
        "SELECT id, informe_id, usuario_id, fecha_vencimiento_nominal, fecha_limite_teorica, \
                tipo_ajuste, fecha_entrega_real, estado, ruta_archivo, firma \
         FROM entregas_esperadas ORDER BY id",
    )
    .fetch_all(&src)
    .await?
    {
        let id: i64 = fila.get("id");
        let informe_id = fila.get::<Option<i64>, _>("informe_id").ok_or(ErrorImportar::EntregaSinInforme(id))?;

        let estado: String = fila.get::<Option<String>, _>("estado").unwrap_or_default().trim().to_uppercase();
        if !ESTADOS.contains(&estado.as_str()) {
            return Err(ErrorImportar::Estado(id, estado));
        }
        let ajuste: String = fila.get::<Option<String>, _>("tipo_ajuste").unwrap_or_default().trim().to_uppercase();
        if !AJUSTES.contains(&ajuste.as_str()) {
            return Err(ErrorImportar::Ajuste(id, ajuste));
        }

        let nominal = solo_fecha(id, "fecha_vencimiento_nominal", &fila.get::<String, _>("fecha_vencimiento_nominal"))?;
        let limite = solo_fecha(id, "fecha_limite_teorica", &fila.get::<String, _>("fecha_limite_teorica"))?;
        let archivo = nombre_archivo(fila.get::<Option<String>, _>("ruta_archivo").as_deref());

        sqlx::query(
            "INSERT INTO entregas \
             (id, informe_id, usuario_id, fecha_nominal, fecha_limite, tipo_ajuste, estado, \
              fecha_entrega_real, archivo_nombre, firma) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(informe_id)
        .bind(fila.get::<Option<i64>, _>("usuario_id"))
        .bind(nominal)
        .bind(limite)
        .bind(ajuste)
        .bind(estado)
        .bind(fila.get::<Option<String>, _>("fecha_entrega_real"))
        .bind(archivo)
        .bind(fila.get::<Option<String>, _>("firma"))
        .execute(&mut *tx)
        .await?;
        r.entregas += 1;
    }

    // Deja constancia del volcado en la auditoría de la base nueva. No se migra la
    // auditoría del sistema viejo —es historia operativa de otro esquema—, pero sí
    // se registra que este volcado ocurrió y con cuántas filas.
    sqlx::query("INSERT INTO auditoria (momento, evento, email, detalle) VALUES (?, 'IMPORTACION', NULL, ?)")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(format!(
            "volcado desde «{origen}»: {} usuarios, {} stakeholders, {} informes, {} participaciones, {} entregas",
            r.usuarios, r.stakeholders, r.informes, r.participacion, r.entregas
        ))
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El motivo del rechazo, para poder afirmar QUÉ capa lo paró.
    ///
    /// Las tres comprobaciones de `clave_argon2` están en cascada, así que una
    /// entrada mal elegida las atraviesa todas por la primera y deja las otras
    /// dos sin probar — verde indistinguible del verde real. Cada prueba de aquí
    /// abajo nombra su capa; si un cambio mueve el rechazo de sitio, se cae.
    fn motivo(h: &str) -> String {
        clave_argon2(1, "a@b.co", h).unwrap_err().to_string()
    }

    /// Un hash Argon2 del origen pasa tal cual: es el caso normal, y la
    /// comprobación no puede estorbarlo (directiva 21: la regla se prueba contra
    /// el cliente legítimo, no contra el atacante).
    #[test]
    fn el_hash_argon2_del_origen_pasa_verbatim() {
        // El que emite HOY el propio sistema (`hashear`), generado aquí y no
        // copiado a mano: si algún día cambian sus parámetros, esta prueba se
        // entera. Los otros dos son literales porque el origen es passlib, que
        // no siempre eligió argon2id — rechazar argon2i dejaría fuera cuentas
        // legítimas, que es el error contrario y peor.
        use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
        let propio = argon2::Argon2::default()
            .hash_password(b"cualquiera", &SaltString::generate(&mut OsRng))
            .expect("el hasher del sistema produce un PHC válido")
            .to_string();

        for h in [
            propio.as_str(),
            "$argon2i$v=19$m=4096,t=3,p=1$c2Fsc2Fsc2Fsc2Fsc2Fs$3wLKm4Cy0S4KEC0N/EOKuqhKz9WlFtHiJvQMHNKqvT4",
            "$argon2d$v=19$m=4096,t=3,p=1$c2Fsc2Fsc2Fsc2Fsc2Fs$3wLKm4Cy0S4KEC0N/EOKuqhKz9WlFtHiJvQMHNKqvT4",
        ] {
            assert_eq!(clave_argon2(1, "a@b.co", h).unwrap(), h, "rechazado: {h}");
        }
    }

    /// El caso de #231. Un bcrypt superviviente no lo reconoce ningún esquema
    /// del sistema nuevo: copiarlo deja esa cuenta fuera PARA SIEMPRE, y el
    /// login responde exactamente lo mismo que ante una clave mala, así que
    /// quien lo sufre no tiene ninguna señal. El volcado se detiene.
    #[test]
    fn un_bcrypt_superviviente_detiene_el_volcado() {
        let h = "$2b$12$KIXQd7yPQhLpqZ0e0O9K0eXhO0LNnRZQ1B3D8dQPmZfJPjMcCsGvC";
        let e = clave_argon2(7, "quedo@fuera.co", h).unwrap_err();
        assert!(matches!(e, ErrorImportar::ClaveIlegible(7, _, _)), "{e}");
        // El mensaje nombra la fila: sin eso, quien migra no sabe a QUIÉN
        // reponerle la contraseña.
        assert!(e.to_string().contains("quedo@fuera.co"), "{e}");
        // Y lo para la PRIMERA capa: bcrypt no llega ni a ser PHC (su «salt» de
        // dos dígitos es más corto que el mínimo).
        assert!(e.to_string().contains("no es un hash PHC legible"), "{e}");
    }

    /// El agujero que la prueba anterior no cubre: un hash de otro esquema que
    /// SÍ es un PHC válido —scrypt— atraviesa el `PasswordHash::new` del login
    /// sin activar su rama de error, muere después en `verify_password` y NO
    /// deja rastro en ningún log. Por eso parsear no basta: hay que exigir la
    /// familia del algoritmo, que es la SEGUNDA capa.
    #[test]
    fn un_phc_valido_que_no_es_argon2_tampoco_pasa() {
        let h = "$scrypt$ln=16,r=8,p=1$c2FsdHNhbHRzYWx0c2FsdA$AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
        let e = clave_argon2(9, "otro@esquema.co", h).unwrap_err();
        assert!(matches!(e, ErrorImportar::ClaveIlegible(9, _, _)), "{e}");
        assert!(e.to_string().contains("«scrypt» no es de la familia Argon2"), "{e}");
    }

    /// TERCERA capa: es Argon2 y parsea, pero sus parámetros no construyen un
    /// KDF. El login lo tomaría por bueno hasta el momento de verificar.
    #[test]
    fn un_argon2_con_parametros_imposibles_tampoco_pasa() {
        let h = "$argon2id$v=19$m=0,t=1,p=1$c2FsdHNhbHRzYWx0c2FsdA$AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
        assert!(motivo(h).contains("los parámetros del Argon2 no se pueden leer"), "{}", motivo(h));
    }

    /// Ni el vacío ni la basura caen a un defecto: son la misma parada.
    #[test]
    fn una_clave_ausente_o_ilegible_no_cae_a_un_defecto() {
        for h in ["", "         ", "contraseña-en-claro"] {
            assert!(motivo(h).contains("no es un hash PHC legible"), "{h:?}: {}", motivo(h));
        }
    }

    #[test]
    fn el_rol_admin_manda_sobre_rol_sistema() {
        // es_admin=1 gana aunque el rol_sistema diga otra cosa.
        assert_eq!(rol_usuario(1, "a@b.co", 1, Some("OPERADOR")).unwrap(), "ADMIN");
        assert_eq!(rol_usuario(2, "a@b.co", 0, Some("operador")).unwrap(), "OPERADOR");
    }

    #[test]
    fn un_rol_desconocido_no_cae_a_un_defecto() {
        // La regla no tiene «en cualquier otro caso»: un rol ajeno es un error,
        // no un OPERADOR por descarte.
        let e = rol_usuario(3, "x@y.co", 0, Some("GERENTE")).unwrap_err();
        assert!(matches!(e, ErrorImportar::RolUsuario(3, _, _)));
        // Sin rol_sistema tampoco se inventa uno.
        assert!(rol_usuario(4, "x@y.co", 0, None).is_err());
    }

    #[test]
    fn la_fecha_se_queda_en_el_dia() {
        assert_eq!(solo_fecha(1, "f", "2026-01-01 23:59:59.000000").unwrap(), "2026-01-01");
        assert_eq!(solo_fecha(1, "f", "2026-03-15").unwrap(), "2026-03-15");
        assert_eq!(solo_fecha(1, "f", "2026-03-15T08:00:00+00:00").unwrap(), "2026-03-15");
        // Una fecha ilegible detiene el volcado; no se sustituye por hoy.
        assert!(solo_fecha(1, "f", "ayer").is_err());
        assert!(solo_fecha(1, "f", "").is_err());
    }

    #[test]
    fn del_path_solo_sobrevive_el_nombre() {
        assert_eq!(nombre_archivo(Some("/var/datos/e/informe.pdf")).as_deref(), Some("informe.pdf"));
        assert_eq!(nombre_archivo(Some("suelto.xlsx")).as_deref(), Some("suelto.xlsx"));
        assert_eq!(nombre_archivo(None), None);
    }
}
