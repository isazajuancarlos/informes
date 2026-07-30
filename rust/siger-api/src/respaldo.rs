//! Respaldo y restauración. **Lo que se restaura es lo que se probó.**
//!
//! # Por qué esto no es «copiar el archivo»
//!
//! La base va en modo WAL: las escrituras confirmadas viven en `siger.db-wal`
//! hasta que alguien las integra. Copiar `siger.db` con el servidor vivo produce
//! un archivo que se abre, que pasa por bueno, y al que le faltan las últimas
//! transacciones — o que trae una a medias. El respaldo parece bueno hasta el
//! día en que hace falta, que es el único día en que se mira.
//!
//! Aquí se usa `VACUUM INTO`, que toma la instantánea bajo una transacción de
//! lectura: consistente, en caliente, y sin parar el servicio.
//!
//! # Y por qué la base sola no es un respaldo
//!
//! Los archivos de las entregas viven FUERA de la base. Un respaldo que solo
//! guarda `siger.db` restaura un sistema donde cada entrega cumplida dice tener
//! un archivo que ya no existe: la fila promete y el disco no cumple. El
//! respaldo lleva los dos, y se niega a declararse bueno si algún archivo que la
//! base nombra no llegó a la copia.
//!
//! # La verificación no es opcional
//!
//! «Hay un cron» no es tener respaldos. Cada respaldo se ABRE al terminar, se le
//! pasa `PRAGMA integrity_check`, se comparan los recuentos de las tablas con el
//! origen y se comprueba archivo por archivo. Si algo no cuadra, el comando sale
//! con error y lo dice; no deja una carpeta que parezca un respaldo.

use std::path::{Path, PathBuf};

use sqlx::{Connection, Row, SqliteConnection};

/// Las tablas cuyo recuento se compara entre origen y copia.
///
/// Es la lista de lo que constituye el estado del sistema. Si mañana nace una
/// tabla nueva y no entra aquí, el respaldo seguiría diciendo «verificado»
/// habiéndola ignorado, así que la prueba `las_tablas_del_esquema_estan_todas`
/// compara esta lista contra el esquema real y falla si alguien añade una tabla
/// sin pasar por aquí.
const TABLAS: &[&str] = &[
    "usuarios",
    "stakeholders",
    "informes",
    "participacion",
    "entregas",
    "auditoria",
    // Estas dos las cazó la prueba de abajo en su primera corrida: existen desde
    // la migración 0004, y yo había escrito la lista de memoria.
    "notificaciones",
    "comentarios",
];

#[derive(Debug, thiserror::Error)]
pub enum ErrorRespaldo {
    #[error("no se pudo abrir «{0}»: {1}")]
    NoAbre(String, sqlx::Error),
    #[error("la copia no se pudo escribir en «{0}»: {1}")]
    NoEscribe(String, sqlx::Error),
    #[error("problema con los archivos: {0}")]
    Archivos(#[from] std::io::Error),
    #[error("el destino «{0}» ya tiene contenido; elija una carpeta nueva")]
    DestinoOcupado(String),
    #[error("«{0}» no parece un respaldo de SIGER: falta {1}")]
    NoEsRespaldo(String, &'static str),
    #[error(
        "la copia NO es fiel al origen y por eso no se declara buena:\n{}",
        .0.join("\n")
    )]
    NoVerifica(Vec<String>),
    #[error(
        "la base de destino ya tiene datos ({0} usuarios, {1} entregas). \
         Restaurar los borraría. Pare el servicio y use --forzar si es lo que quiere."
    )]
    DestinoConDatos(i64, i64),
}

/// Qué se guardó, para poder decirlo y para poder compararlo.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Resumen {
    pub filas: Vec<(String, i64)>,
    pub archivos: usize,
    pub bytes_bd: u64,
}

impl Resumen {
    pub fn total_filas(&self) -> i64 {
        self.filas.iter().map(|(_, n)| n).sum()
    }
}

fn ruta_de_url(url: &str) -> PathBuf {
    // `sqlite://siger.db`, `sqlite:siger.db` o una ruta pelada. Los parámetros
    // (`?mode=rwc`) se descartan: no forman parte del nombre del archivo.
    let sin_esquema = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
        .unwrap_or(url);
    PathBuf::from(sin_esquema.split('?').next().unwrap_or(sin_esquema))
}

async fn conectar(ruta: &Path) -> Result<SqliteConnection, ErrorRespaldo> {
    let url = format!("sqlite://{}", ruta.display());
    SqliteConnection::connect(&url)
        .await
        .map_err(|e| ErrorRespaldo::NoAbre(ruta.display().to_string(), e))
}

async fn recuentos(cx: &mut SqliteConnection) -> Result<Vec<(String, i64)>, sqlx::Error> {
    let mut salida = Vec::new();
    for tabla in TABLAS {
        // El nombre de la tabla viene de una constante del código, nunca de
        // fuera: no hay parámetro que ligar porque no hay entrada del usuario.
        let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {tabla}"))
            .fetch_one(&mut *cx)
            .await?;
        salida.push(((*tabla).to_string(), n));
    }
    Ok(salida)
}

/// Los archivos que la base DICE tener, con la entrega a la que pertenecen.
async fn archivos_declarados(
    cx: &mut SqliteConnection,
) -> Result<Vec<(i64, String)>, sqlx::Error> {
    let filas = sqlx::query(
        "SELECT id, archivo_nombre FROM entregas \
         WHERE archivo_nombre IS NOT NULL AND archivo_nombre <> ''",
    )
    .fetch_all(&mut *cx)
    .await?;
    Ok(filas
        .into_iter()
        .map(|f| (f.get::<i64, _>("id"), f.get::<String, _>("archivo_nombre")))
        .collect())
}

fn copiar_arbol(origen: &Path, destino: &Path) -> std::io::Result<usize> {
    if !origen.exists() {
        // Que no haya ni un archivo subido es un estado legítimo de una
        // instalación recién puesta, no un error.
        return Ok(0);
    }
    let mut copiados = 0;
    std::fs::create_dir_all(destino)?;
    for entrada in std::fs::read_dir(origen)? {
        let entrada = entrada?;
        let destino_hijo = destino.join(entrada.file_name());
        if entrada.file_type()?.is_dir() {
            copiados += copiar_arbol(&entrada.path(), &destino_hijo)?;
        } else {
            std::fs::copy(entrada.path(), &destino_hijo)?;
            copiados += 1;
        }
    }
    Ok(copiados)
}

/// `siger-api respaldar <carpeta>`: la base en caliente y sus archivos, con la
/// copia verificada contra el origen antes de decir que existe.
pub async fn respaldar(url_bd: &str, dir_entregas: &str, destino: &Path) -> Result<Resumen, ErrorRespaldo> {
    if destino.exists() && std::fs::read_dir(destino)?.next().is_some() {
        // Escribir dentro de una carpeta que ya tiene cosas mezcla dos respaldos
        // y produce uno que no es ninguno de los dos.
        return Err(ErrorRespaldo::DestinoOcupado(destino.display().to_string()));
    }
    std::fs::create_dir_all(destino)?;

    let origen_bd = ruta_de_url(url_bd);
    let copia_bd = destino.join("siger.db");

    let mut cx = conectar(&origen_bd).await?;
    let filas_origen = recuentos(&mut cx)
        .await
        .map_err(|e| ErrorRespaldo::NoAbre(origen_bd.display().to_string(), e))?;
    let declarados = archivos_declarados(&mut cx)
        .await
        .map_err(|e| ErrorRespaldo::NoAbre(origen_bd.display().to_string(), e))?;

    // `VACUUM INTO` toma la instantánea bajo una transacción de lectura: no para
    // al servidor, no arrastra el WAL a medias y deja la copia ya compactada.
    // El destino lo escribe SQLite, así que la ruta va en la propia orden; se
    // rechaza cualquier comilla en ella antes de llegar aquí (ver abajo).
    let ruta_copia = copia_bd.display().to_string();
    if ruta_copia.contains('\'') {
        return Err(ErrorRespaldo::DestinoOcupado(format!(
            "{ruta_copia} (la ruta no puede llevar comillas simples)"
        )));
    }
    sqlx::query(&format!("VACUUM INTO '{ruta_copia}'"))
        .execute(&mut cx)
        .await
        .map_err(|e| ErrorRespaldo::NoEscribe(ruta_copia.clone(), e))?;
    cx.close().await.ok();

    let copiados = copiar_arbol(Path::new(dir_entregas), &destino.join("entregas"))?;

    // Y ahora se comprueba. Un respaldo que no se abre no es un respaldo.
    let resumen = verificar(destino, Some(&filas_origen), Some(&declarados)).await?;

    std::fs::write(
        destino.join("RESPALDO.txt"),
        format!(
            "Respaldo de SIGER\n\
             fecha: {}\n\
             origen: {}\n\
             archivos de entrega: {}\n\
             {}\n\
             Verificado al crearse: integrity_check, recuentos por tabla y \n\
             presencia de cada archivo que la base nombra.\n\n\
             Restaurar:  siger-api restaurar {}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            origen_bd.display(),
            copiados,
            resumen
                .filas
                .iter()
                .map(|(t, n)| format!("{t}: {n}"))
                .collect::<Vec<_>>()
                .join("\n"),
            destino.display(),
        ),
    )?;

    Ok(Resumen { archivos: copiados, ..resumen })
}

/// Abre un respaldo y comprueba que es fiel. Si se le pasan los recuentos y los
/// archivos del origen, además compara; si no, verifica lo que puede por sí solo
/// (integridad y que cada archivo nombrado esté presente).
///
/// Se usa en dos momentos: al crear el respaldo y antes de restaurarlo. Es a
/// propósito la MISMA función: comprobar la restauración con un criterio más
/// flojo que el de la copia dejaría pasar justo lo que se degradó en el medio.
pub async fn verificar(
    carpeta: &Path,
    filas_origen: Option<&[(String, i64)]>,
    archivos_origen: Option<&[(i64, String)]>,
) -> Result<Resumen, ErrorRespaldo> {
    let bd = carpeta.join("siger.db");
    if !bd.exists() {
        return Err(ErrorRespaldo::NoEsRespaldo(
            carpeta.display().to_string(),
            "siger.db",
        ));
    }

    let mut fallos = Vec::new();
    let mut cx = conectar(&bd).await?;

    let integridad: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut cx)
        .await
        .map_err(|e| ErrorRespaldo::NoAbre(bd.display().to_string(), e))?;
    if integridad != "ok" {
        fallos.push(format!("  integrity_check dice: {integridad}"));
    }

    let filas = recuentos(&mut cx)
        .await
        .map_err(|e| ErrorRespaldo::NoAbre(bd.display().to_string(), e))?;
    if let Some(esperadas) = filas_origen {
        for ((tabla, copia), (_, origen)) in filas.iter().zip(esperadas.iter()) {
            if copia != origen {
                fallos.push(format!(
                    "  {tabla}: el origen tiene {origen} filas y la copia {copia}"
                ));
            }
        }
    }

    // Cada archivo que la base NOMBRA tiene que estar. Se mira en la copia, y
    // se comprueba contra lo que declaraba el origen cuando se sabe: si una fila
    // perdió su nombre de archivo por el camino, comparar solo la copia consigo
    // misma no lo vería.
    let declarados_copia = archivos_declarados(&mut cx)
        .await
        .map_err(|e| ErrorRespaldo::NoAbre(bd.display().to_string(), e))?;
    if let Some(origen) = archivos_origen {
        if origen.len() != declarados_copia.len() {
            fallos.push(format!(
                "  el origen declaraba {} archivos de entrega y la copia declara {}",
                origen.len(),
                declarados_copia.len()
            ));
        }
    }
    let mut ausentes = Vec::new();
    for (id, nombre) in &declarados_copia {
        if !carpeta.join("entregas").join(id.to_string()).join(nombre).exists() {
            ausentes.push(format!("entrega {id}: {nombre}"));
        }
    }
    if !ausentes.is_empty() {
        fallos.push(format!(
            "  {} archivos que la base nombra NO están en el respaldo:\n    {}",
            ausentes.len(),
            ausentes.join("\n    ")
        ));
    }
    cx.close().await.ok();

    if !fallos.is_empty() {
        return Err(ErrorRespaldo::NoVerifica(fallos));
    }

    Ok(Resumen {
        filas,
        archivos: declarados_copia.len(),
        bytes_bd: std::fs::metadata(&bd)?.len(),
    })
}

/// `siger-api restaurar <carpeta>`: pone el respaldo en su sitio.
///
/// Verifica ANTES de tocar nada, y se niega si el destino ya tiene datos salvo
/// que se le insista. Restaurar es lo que se hace el peor día; que ese día haya
/// que descubrir que el respaldo estaba mal es exactamente el fallo que este
/// módulo existe para no tener.
pub async fn restaurar(
    carpeta: &Path,
    url_bd: &str,
    dir_entregas: &str,
    forzar: bool,
) -> Result<Resumen, ErrorRespaldo> {
    let resumen = verificar(carpeta, None, None).await?;

    let destino_bd = ruta_de_url(url_bd);
    if destino_bd.exists() && !forzar {
        let mut cx = conectar(&destino_bd).await?;

        // Dos situaciones que se parecen y no lo son:
        //
        //   (a) la base de destino no tiene ni el esquema — máquina nueva, no hay
        //       nada que perder, adelante;
        //   (b) la base existe pero NO SE PUEDE LEER — tomada por otro proceso,
        //       corrupta, sin permisos.
        //
        // Antes las dos daban lo mismo, porque el recuento se leía con
        // `.unwrap_or(0)`: en el caso (b) el fallo de lectura se contaba como
        // «cero filas» y la restauración pisaba una base que podía estar llena.
        // «No pude mirar» contabilizado como «no hay nada» es la forma más cara
        // de perder datos, porque parece que funcionó.
        //
        // Se distinguen preguntando primero si las tablas EXISTEN. Si no existen,
        // es (a). Si existen, el recuento tiene que salir bien: cualquier error se
        // propaga y no se restaura nada.
        let con_esquema: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type = 'table' AND name IN ('usuarios', 'entregas')",
        )
        .fetch_one(&mut cx)
        .await
        .map_err(|e| ErrorRespaldo::NoAbre(destino_bd.display().to_string(), e))?;

        if con_esquema > 0 {
            let usuarios: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM usuarios")
                .fetch_one(&mut cx)
                .await
                .map_err(|e| ErrorRespaldo::NoAbre(destino_bd.display().to_string(), e))?;
            let entregas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entregas")
                .fetch_one(&mut cx)
                .await
                .map_err(|e| ErrorRespaldo::NoAbre(destino_bd.display().to_string(), e))?;
            cx.close().await.ok();
            if usuarios > 0 || entregas > 0 {
                return Err(ErrorRespaldo::DestinoConDatos(usuarios, entregas));
            }
        } else {
            cx.close().await.ok();
        }
    }

    if let Some(padre) = destino_bd.parent() {
        if !padre.as_os_str().is_empty() {
            std::fs::create_dir_all(padre)?;
        }
    }
    std::fs::copy(carpeta.join("siger.db"), &destino_bd)?;
    // El WAL y el índice del origen NO se conservan: la copia de `VACUUM INTO`
    // ya los tiene integrados, y dejar un WAL viejo junto a una base nueva es
    // pedirle a SQLite que reproduzca transacciones de otra base.
    for sufijo in ["-wal", "-shm"] {
        let mut ruta = destino_bd.clone().into_os_string();
        ruta.push(sufijo);
        std::fs::remove_file(PathBuf::from(ruta)).ok();
    }

    let entregas_origen = carpeta.join("entregas");
    if entregas_origen.exists() {
        std::fs::create_dir_all(dir_entregas)?;
        copiar_arbol(&entregas_origen, Path::new(dir_entregas))?;
    }

    Ok(resumen)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tablas que el esquema tiene y que a propósito NO se comparan.
    ///
    /// Vive aquí y no junto a `TABLAS` porque solo la usa la prueba que audita
    /// esa lista; puesta arriba sería código muerto en el binario, y un
    /// `#[allow(dead_code)]` para conservar la simetría cambia una comprobación
    /// del compilador por una nota de estilo.
    const TABLAS_EXENTAS: &[&str] = &[
        // La lleva sqlx para saber qué migraciones aplicó. Viaja en la copia
        // como parte de la base, pero su recuento no dice nada del estado del
        // cliente.
        "_sqlx_migrations",
    ];

    /// Una base con esquema, dos usuarios, una entrega con archivo y su archivo.
    async fn base_de_prueba(dir: &Path) -> (String, String) {
        let url = format!("sqlite://{}?mode=rwc", dir.join("siger.db").display());
        let pool = crate::db::abrir(&url).await.unwrap();
        sqlx::query(
            "INSERT INTO usuarios (nombre, email, clave_hash, rol) \
             VALUES ('A','a@co.co','x','ADMIN'), ('B','b@co.co','x','OPERADOR')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO informes (id, nombre, recurrencia) VALUES (1,'I','{}')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO entregas (id, informe_id, fecha_nominal, fecha_limite, estado, archivo_nombre) \
             VALUES (7, 1, '2026-01-01', '2026-01-01', 'CUMPLIDO', 'acta.pdf')",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let entregas = dir.join("storage").join("entregas");
        std::fs::create_dir_all(entregas.join("7")).unwrap();
        std::fs::write(entregas.join("7").join("acta.pdf"), b"contenido").unwrap();
        (url, entregas.display().to_string())
    }

    #[tokio::test]
    async fn el_respaldo_se_verifica_y_trae_los_archivos() {
        let tmp = tempfile::tempdir().unwrap();
        let (url, entregas) = base_de_prueba(tmp.path()).await;
        let destino = tmp.path().join("copia");

        let r = respaldar(&url, &entregas, &destino).await.unwrap();

        assert_eq!(r.archivos, 1);
        assert_eq!(r.filas.iter().find(|(t, _)| t == "usuarios").unwrap().1, 2);
        assert!(destino.join("entregas").join("7").join("acta.pdf").exists());
        assert!(destino.join("RESPALDO.txt").exists());
    }

    #[tokio::test]
    async fn un_archivo_que_falta_impide_declarar_bueno_el_respaldo() {
        // La prueba que de verdad importa: el respaldo tiene que DISCRIMINAR.
        // Sin esto, «verificado» sería una palabra escrita en un `println!`.
        let tmp = tempfile::tempdir().unwrap();
        let (url, entregas) = base_de_prueba(tmp.path()).await;
        let destino = tmp.path().join("copia");
        respaldar(&url, &entregas, &destino).await.unwrap();

        std::fs::remove_file(destino.join("entregas").join("7").join("acta.pdf")).unwrap();

        let e = verificar(&destino, None, None).await.unwrap_err();
        let texto = e.to_string();
        assert!(texto.contains("acta.pdf"), "el fallo no nombra el archivo: {texto}");
        assert!(texto.contains("entrega 7"), "el fallo no nombra la entrega: {texto}");
    }

    #[tokio::test]
    async fn una_base_corrompida_no_pasa_la_verificacion() {
        let tmp = tempfile::tempdir().unwrap();
        let (url, entregas) = base_de_prueba(tmp.path()).await;
        let destino = tmp.path().join("copia");
        respaldar(&url, &entregas, &destino).await.unwrap();

        // Se pisan las páginas del medio: el encabezado sigue diciendo «soy
        // SQLite», así que la base ABRE. Es justo el respaldo que parece bueno.
        use std::io::{Seek, SeekFrom, Write};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(destino.join("siger.db"))
            .unwrap();
        f.seek(SeekFrom::Start(4096)).unwrap();
        f.write_all(&[0xFF; 4096]).unwrap();
        f.sync_all().unwrap();
        drop(f);

        assert!(
            verificar(&destino, None, None).await.is_err(),
            "una base con las páginas pisadas pasó por buena"
        );
    }

    #[tokio::test]
    async fn restaurar_no_pisa_una_base_con_datos() {
        let tmp = tempfile::tempdir().unwrap();
        let (url, entregas) = base_de_prueba(tmp.path()).await;
        let destino = tmp.path().join("copia");
        respaldar(&url, &entregas, &destino).await.unwrap();

        let e = restaurar(&destino, &url, &entregas, false).await.unwrap_err();
        assert!(matches!(e, ErrorRespaldo::DestinoConDatos(2, 1)), "{e}");
    }

    #[tokio::test]
    async fn una_base_de_destino_ilegible_no_cuenta_como_vacia() {
        // El hallazgo de la revisión de seguridad del 2026-07-30. El recuento del
        // destino se leía con `.unwrap_or(0)`, así que una base tomada, corrupta o
        // sin permisos daba «cero filas» y la restauración la pisaba. «No pude
        // mirar» contabilizado como «no hay nada» es la forma más cara de perder
        // datos: parece que funcionó.
        //
        // Aquí se corrompen las páginas del medio de la base de DESTINO —el
        // encabezado sigue diciendo «soy SQLite», así que abre— y se comprueba
        // que restaurar se NIEGA en vez de sobrescribirla.
        let tmp = tempfile::tempdir().unwrap();
        let (url, entregas) = base_de_prueba(tmp.path()).await;
        let destino = tmp.path().join("copia");
        respaldar(&url, &entregas, &destino).await.unwrap();

        use std::io::{Seek, SeekFrom, Write};
        let bd = ruta_de_url(&url);
        let mut f = std::fs::OpenOptions::new().write(true).open(&bd).unwrap();
        f.seek(SeekFrom::Start(4096)).unwrap();
        f.write_all(&[0xFF; 8192]).unwrap();
        f.sync_all().unwrap();
        drop(f);

        let r = restaurar(&destino, &url, &entregas, false).await;
        assert!(
            r.is_err(),
            "restauró sobre una base que no se pudo leer: eso la habría borrado"
        );
    }

    #[tokio::test]
    async fn restaurar_en_una_maquina_nueva_sigue_funcionando() {
        // El reverso de la anterior, para que la corrección no cierre de más: una
        // base de destino que existe pero AÚN NO TIENE ESQUEMA es el caso normal
        // de instalar en una máquina nueva, y ahí no hay nada que perder.
        let tmp = tempfile::tempdir().unwrap();
        let (url, entregas) = base_de_prueba(tmp.path()).await;
        let destino = tmp.path().join("copia");
        respaldar(&url, &entregas, &destino).await.unwrap();

        // Base vacía de verdad: el archivo EXISTE y no tiene una sola tabla. Un
        // archivo de cero bytes es una base SQLite válida y vacía, que es
        // exactamente el estado de una instalación recién puesta. (No sirve
        // `conectar`: abre sin crear, a propósito.)
        let bd = ruta_de_url(&url);
        std::fs::remove_file(&bd).unwrap();
        std::fs::File::create(&bd).unwrap();

        restaurar(&destino, &url, &entregas, false)
            .await
            .expect("una base sin esquema debe poder restaurarse sin --forzar");
    }

    #[tokio::test]
    async fn restaurar_deja_el_sistema_como_estaba() {
        // La vuelta completa, que es lo único que prueba que el respaldo SIRVE:
        // se respalda, se destruye el original, se restaura, y se comprueba que
        // están las filas Y el archivo.
        let tmp = tempfile::tempdir().unwrap();
        let (url, entregas) = base_de_prueba(tmp.path()).await;
        let destino = tmp.path().join("copia");
        respaldar(&url, &entregas, &destino).await.unwrap();

        let bd = ruta_de_url(&url);
        std::fs::remove_file(&bd).unwrap();
        std::fs::remove_dir_all(&entregas).unwrap();

        let r = restaurar(&destino, &url, &entregas, false).await.unwrap();
        assert_eq!(r.filas.iter().find(|(t, _)| t == "usuarios").unwrap().1, 2);

        let mut cx = conectar(&bd).await.unwrap();
        let usuarios: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM usuarios")
            .fetch_one(&mut cx)
            .await
            .unwrap();
        assert_eq!(usuarios, 2);
        assert_eq!(
            std::fs::read_to_string(Path::new(&entregas).join("7").join("acta.pdf")).unwrap(),
            "contenido"
        );
    }

    #[tokio::test]
    async fn no_se_respalda_sobre_una_carpeta_ocupada() {
        let tmp = tempfile::tempdir().unwrap();
        let (url, entregas) = base_de_prueba(tmp.path()).await;
        let destino = tmp.path().join("copia");
        std::fs::create_dir_all(&destino).unwrap();
        std::fs::write(destino.join("otra-cosa.txt"), b"x").unwrap();

        assert!(matches!(
            respaldar(&url, &entregas, &destino).await.unwrap_err(),
            ErrorRespaldo::DestinoOcupado(_)
        ));
    }

    #[tokio::test]
    async fn las_tablas_del_esquema_estan_todas() {
        // El vigilante también se audita: si alguien añade una tabla y no la
        // mete en TABLAS, el respaldo diría «verificado» sin haberla mirado.
        let tmp = tempfile::tempdir().unwrap();
        let (url, _) = base_de_prueba(tmp.path()).await;
        let mut cx = conectar(&ruta_de_url(&url)).await.unwrap();
        let filas = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_all(&mut cx)
        .await
        .unwrap();

        let del_esquema: Vec<String> = filas.iter().map(|f| f.get::<String, _>("name")).collect();
        let sin_comprobar: Vec<&String> = del_esquema
            .iter()
            .filter(|t| !TABLAS.contains(&t.as_str()) && !TABLAS_EXENTAS.contains(&t.as_str()))
            .collect();
        assert!(
            sin_comprobar.is_empty(),
            "tablas que el respaldo no compara: {sin_comprobar:?}. \
             Añádalas a TABLAS o a TABLAS_EXENTAS con su razón."
        );
    }

    #[test]
    fn la_url_se_convierte_en_ruta() {
        assert_eq!(ruta_de_url("sqlite://siger.db"), PathBuf::from("siger.db"));
        assert_eq!(ruta_de_url("sqlite:/tmp/a.db"), PathBuf::from("/tmp/a.db"));
        assert_eq!(ruta_de_url("siger.db?mode=rwc"), PathBuf::from("siger.db"));
    }
}
