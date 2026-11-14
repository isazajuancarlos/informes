//! Conexión y esquema.
//!
//! Las migraciones van **dentro del binario** (`sqlx::migrate!`) y se aplican al
//! arrancar. El sistema anterior necesitaba `python -m app.init_db` y después
//! `python migrar_esquema.py`, en ese orden y de memoria; el día que se
//! invirtieron, el instalador terminó con código 0 anunciando credenciales que
//! no existían. Aquí no hay orden que recordar porque no hay dos pasos.

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

#[derive(Debug, thiserror::Error)]
pub enum ErrorBd {
    #[error("no se pudo abrir la base «{0}»: {1}")]
    NoAbre(String, sqlx::Error),
    #[error("la migración del esquema falló: {0}")]
    Migracion(#[from] sqlx::migrate::MigrateError),
}

/// Abre la base y deja el esquema al día.
///
/// SQLite es el motor por defecto de los tres perfiles: un cliente por
/// instalación, un solo escritor y una base que hoy pesa 1,8 MB. PostgreSQL
/// deja de ser una dependencia que administrar para pasar a ser una opción, y
/// entra por el mismo `sqlx` cuando un cliente lo exija.
pub async fn abrir(url: &str) -> Result<SqlitePool, ErrorBd> {
    let opciones = SqliteConnectOptions::from_str(url)
        .map_err(|e| ErrorBd::NoAbre(url.to_string(), e))?
        // Que la base no exista es normal en la primera ejecución; que no se
        // pueda crear, no.
        .create_if_missing(true)
        // WAL: lecturas concurrentes con la escritura. `foreign_keys` no está
        // activo por defecto en SQLite, y sin él las claves foráneas del esquema
        // son documentación, no una restricción.
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opciones)
        .await
        .map_err(|e| ErrorBd::NoAbre(url.to_string(), e))?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod pruebas_migracion {
    //! ACTUALIZAR DE UNA VERSIÓN A LA SIGUIENTE NO PIERDE DATOS (A3.11).
    //!
    //! Es la única de las cinco condiciones de «plena operación» del plan que no
    //! tenía prueba en informes (medido en #609): un producto que se instala en
    //! casa del cliente y no sabe migrar su propia base solo se vende una vez.
    //!
    //! # Qué mide, y qué NO
    //!
    //! Hoy las siete migraciones son ADITIVAS (`ADD COLUMN`, `CREATE TABLE IF NOT
    //! EXISTS`), así que la pérdida de datos no puede ocurrir por reconstrucción
    //! de tabla. Esta prueba es por tanto una **guardia de regresión**: fija que
    //! sembrar en el esquema viejo y aplicar el resto de migraciones conserva las
    //! filas —padre y **sus hijas `ON DELETE CASCADE`**— y aplica los `DEFAULT`
    //! sobre las filas que ya existían. Se pondrá ROJA el día que una migración
    //! reconstruya `informes`/`usuarios`/`entregas` con las foráneas activas y
    //! dispare el `CASCADE`, que es exactamente el defecto que #609 encontró vivo
    //! en medico (su migración 0045, con su par rojo/verde).
    //!
    //! DISCRIMINA, comprobado el 2026-08-14: se añadió una migración `0008`
    //! destructiva (copiar `informes` a una tabla nueva, `DROP TABLE informes`
    //! con las foráneas activas, renombrar) y las aserciones de `participacion` y
    //! `entregas` se pusieron rojas —el `DROP` disparó el `CASCADE` y vació las
    //! hijas— mientras la de `informes` seguía verde. Retirada la migración, todo
    //! vuelve a verde. Sin esa demostración, un cero aquí no distinguiría «migra
    //! sin perder» de «no hay nada que perder».
    //!
    //! La siembra va con SQL crudo y no por la API a propósito: hay que escribir
    //! en el estado del esquema 0001, que solo se alcanza aplicando esa migración
    //! a mano y dejando que `abrir` aplique el resto.

    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::ConnectOptions;
    use std::str::FromStr;

    #[tokio::test]
    async fn actualizar_de_un_esquema_viejo_no_pierde_los_datos() {
        let dir = std::env::temp_dir().join(format!("siger-mig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("s.db");
        let url = format!("sqlite://{}", ruta.display());

        // 1. Solo la migración inicial (0001), con las foráneas ACTIVAS —que es
        //    como estaba una base antes de las alertas, el cifrado y el 2FA—, y
        //    registrada en `_sqlx_migrations` para que `abrir` continúe desde ahí.
        {
            let mut c = SqliteConnectOptions::from_str(&url)
                .unwrap()
                .create_if_missing(true)
                .foreign_keys(true)
                .connect()
                .await
                .unwrap();
            let m = sqlx::migrate!("./migrations");
            let inicial: Vec<_> = m.iter().filter(|mi| mi.version <= 1).cloned().collect();
            assert_eq!(inicial.len(), 1, "se esperaba exactamente la migración 0001");
            for mi in &inicial {
                sqlx::query(&mi.sql).execute(&mut c).await.unwrap();
            }
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS _sqlx_migrations (
                   version BIGINT PRIMARY KEY, description TEXT NOT NULL,
                   installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                   success BOOLEAN NOT NULL, checksum BLOB NOT NULL,
                   execution_time BIGINT NOT NULL)",
            )
            .execute(&mut c)
            .await
            .unwrap();
            for mi in &inicial {
                sqlx::query(
                    "INSERT INTO _sqlx_migrations
                     (version, description, success, checksum, execution_time)
                     VALUES (?,?,1,?,0)",
                )
                .bind(mi.version)
                .bind(mi.description.as_ref())
                .bind(mi.checksum.as_ref())
                .execute(&mut c)
                .await
                .unwrap();
            }

            // 2. Datos: un informe (padre) con su participación y su entrega, las
            //    dos hijas con `ON DELETE CASCADE`; más el usuario y el stakeholder
            //    a los que apuntan.
            sqlx::query("INSERT INTO stakeholders (id,nombre,externo) VALUES (1,'EPS Uno',1)")
                .execute(&mut c)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO usuarios (id,nombre,email,clave_hash,rol)
                 VALUES (1,'Ana','ana@x.co','$argon2id$v=19$m=19456,t=2,p=1$abc$def','OPERADOR')",
            )
            .execute(&mut c)
            .await
            .unwrap();
            sqlx::query("INSERT INTO informes (id,nombre) VALUES (1,'Informe Mensual')")
                .execute(&mut c)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO participacion (informe_id,usuario_id,rol) VALUES (1,1,'EJECUTA')",
            )
            .execute(&mut c)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO entregas (informe_id,usuario_id,fecha_nominal,fecha_limite)
                 VALUES (1,1,'2026-01-31','2026-02-05')",
            )
            .execute(&mut c)
            .await
            .unwrap();
        }

        // 3. `abrir` aplica 0002..0007 SOBRE esos datos.
        let pool = super::abrir(&url).await.expect("la base tiene que abrir y migrar");

        // Las filas siguen ahí: el padre y sus dos hijas.
        let informes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM informes")
            .fetch_one(&pool)
            .await
            .unwrap();
        let participaciones: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM participacion")
            .fetch_one(&pool)
            .await
            .unwrap();
        let entregas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entregas")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(informes, 1, "se perdió el informe al migrar");
        assert_eq!(
            participaciones, 1,
            "SE PERDIÓ LA PARTICIPACIÓN: una migración que reconstruyera `informes` \
             con las foráneas activas dispararía el ON DELETE CASCADE (es el defecto \
             de la 0045 de medico, #609)"
        );
        assert_eq!(entregas, 1, "se perdió la entrega, por lo mismo");

        // Las columnas que añadieron 0005 y 0006 tomaron su DEFAULT sobre la fila
        // que ya existía —que es lo que un `ADD COLUMN` mal hecho no haría—.
        let cifrado: i64 = sqlx::query_scalar("SELECT archivo_cifrado FROM entregas WHERE id=1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cifrado, 0, "archivo_cifrado no tomó su DEFAULT 0 (0005) en la fila preexistente");
        let token_v: i64 = sqlx::query_scalar("SELECT token_version FROM usuarios WHERE id=1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(token_v, 1, "token_version no tomó su DEFAULT 1 (0006) en la fila preexistente");

        // Y un dato de una columna VIEJA sobrevive textual, no solo el conteo.
        let email: String = sqlx::query_scalar("SELECT email FROM usuarios WHERE id=1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(email, "ana@x.co", "el dato de una columna vieja cambió al migrar");

        pool.close().await;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
