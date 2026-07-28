//! SIGER — un solo binario: la interfaz, el esquema, el motor y el servidor.
//!
//! ```text
//! SIGER_SECRETO_SESION=$(siger-api generar-secreto) siger-api
//! ```
//!
//! Sin Python, sin `pip`, sin Node y sin base de datos que administrar. El
//! cliente copia un archivo y lo ejecuta; la base se crea y se migra sola.

mod alertas;
mod autz;
mod config;
mod db;
mod estaticos;
mod firma;
mod importar;
mod informes;
mod planificador;
mod rutas;
mod sesion;

use axum::{routing::get, Router};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use tower_http::{compression::CompressionLayer, trace::TraceLayer};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // El error se imprime con Display, no con Debug.
    //
    // Devolviendo `Result` desde `main`, Rust imprime `Error: SecretoAusente(...)`
    // —la forma de depuración— y se pierde entero el mensaje que dice qué falta
    // y cómo generarlo. Quien instala esto no lee Rust: lee la única línea que
    // le sale por pantalla, y esa línea tiene que servirle.
    if std::env::args().nth(1).as_deref() == Some("importar") {
        return importar_cmd().await;
    }

    if std::env::args().nth(1).as_deref() == Some("alertas") {
        return alertas_cmd().await;
    }

    if let Some(codigo) = subcomando() {
        return codigo;
    }

    match arrancar().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("\nSIGER no puede arrancar:\n  {e}\n");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Las órdenes que no levantan el servidor. `None` significa «arranca».
fn subcomando() -> Option<std::process::ExitCode> {
    if std::env::args().nth(1).as_deref() == Some("hashear") {
        // Lo necesita el instalador para crear el primer administrador, y hace
        // falta que salga del MISMO código que después verifica: un hash
        // generado por otra herramienta con otros parámetros deja a alguien
        // fuera del sistema, y el fallo aparece lejos de su causa.
        let Some(clave) = std::env::args().nth(2) else {
            eprintln!("uso: siger-api hashear <clave>");
            return Some(std::process::ExitCode::FAILURE);
        };
        use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
        let sal = SaltString::generate(&mut OsRng);
        return Some(match argon2::Argon2::default().hash_password(clave.as_bytes(), &sal) {
            Ok(h) => {
                println!("{h}");
                std::process::ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("no se pudo cifrar la clave: {e}");
                std::process::ExitCode::FAILURE
            }
        });
    }

    if std::env::args().nth(1).as_deref() == Some("generar-secreto") {
        // Que el secreto lo genere el propio binario evita el único camino por
        // el que el sistema anterior acababa con la clave del repositorio en
        // producción: que generarlo fuera trabajo del cliente.
        let mut bytes = [0u8; 48];
        rand::thread_rng().fill_bytes(&mut bytes);
        println!("{}", URL_SAFE_NO_PAD.encode(bytes));
        return Some(std::process::ExitCode::SUCCESS);
    }

    if std::env::args().nth(1).as_deref() == Some("generar-claves-firma") {
        // Las claves de firma de entregas (perfil de custodia `entorno`). Salen
        // del MISMO código que firma, por la misma razón que el hash: una clave
        // hecha por otra herramienta podría no encajar con el verificador. La
        // firma es opcional; sin estas variables, la entrega se registra sin firma.
        let (vk, sk) = crate::firma::generar_claves();
        println!("QUIPU_VERIFYING_KEY={vk}");
        println!("QUIPU_SIGNING_KEY={sk}");
        return Some(std::process::ExitCode::SUCCESS);
    }

    None
}

/// `siger-api importar <db_origen> <db_destino>`: vuelca la base del sistema
/// anterior al esquema nuevo. La base destino tiene que ser nueva; el volcado se
/// niega a mezclar con datos existentes. Falla RUIDOSAMENTE ante cualquier dato
/// que no encaje, y no escribe nada si algo falla (una sola transacción).
async fn importar_cmd() -> std::process::ExitCode {
    let (Some(origen), Some(destino)) = (std::env::args().nth(2), std::env::args().nth(3)) else {
        eprintln!("uso: siger-api importar <db_origen> <db_destino>");
        return std::process::ExitCode::FAILURE;
    };
    match importar::importar(&origen, &destino).await {
        Ok(r) => {
            println!(
                "volcado a «{destino}»: {} usuarios, {} stakeholders, {} informes, \
                 {} participaciones, {} entregas",
                r.usuarios, r.stakeholders, r.informes, r.participacion, r.entregas
            );
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("\nel volcado se detuvo sin escribir nada:\n  {e}\n");
            std::process::ExitCode::FAILURE
        }
    }
}

/// `siger-api alertas`: corre UNA pasada de alertas ahora, con la configuración
/// del entorno. Útil para operar («manda los avisos pendientes ya») y para probar
/// contra un buzón local. Si las alertas no están activas, lo dice y no hace nada.
async fn alertas_cmd() -> std::process::ExitCode {
    let cfg = match config::Config::desde_entorno() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("\nno se pudo leer la configuración:\n  {e}\n");
            return std::process::ExitCode::FAILURE;
        }
    };
    let Some(correo) = cfg.correo.clone() else {
        println!("las alertas están dormidas (ALERTAS_ACTIVAS no está activo); no se envía nada.");
        return std::process::ExitCode::SUCCESS;
    };
    let bd = match db::abrir(&cfg.url_bd).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("no se pudo abrir la base: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let r = alertas::correr(&bd, &correo, chrono::Utc::now().date_naive()).await;
    println!(
        "alertas: {} de proximidad, {} de incumplimiento, {} fallidas, {} sin destinatario",
        r.proximidad, r.incumplimiento, r.fallidas, r.sin_destinatario
    );
    std::process::ExitCode::SUCCESS
}

async fn arrancar() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    // Si falta la configuración, aquí se acaba. No hay arranque a medias con un
    // aviso: la instalación insegura silenciosa no existe.
    let cfg = config::Config::desde_entorno()?;

    // El calendario se construye al arrancar para que un PAIS_ISO sin soporte
    // falle ahora —con el operador delante— y no dentro de seis meses, al
    // calcular la primera fecha, cuando el fallo parecerá otra cosa.
    let calendario = siger_core::Calendario::de_pais(&cfg.pais_iso, 2000, 2100)?;
    let motor = siger_core::Motor::nuevo(calendario);
    tracing::info!(
        "calendario de {} listo; hoy {} es hábil: {}",
        cfg.pais_iso,
        chrono::Local::now().date_naive(),
        motor.calendario().es_habil(chrono::Local::now().date_naive())
    );

    let bd = db::abrir(&cfg.url_bd).await?;

    // El planificador arranca con la aplicación y vuelve a correr cada día.
    // Generar al arrancar y no solo a medianoche es deliberado: una instalación
    // que se apaga por las noches —un portátil en una oficina, que es justo el
    // perfil `standalone`— no vería nunca la medianoche, y el sistema no crearía
    // una sola tarea sin que nadie entendiera por qué.
    planificador::arrancar_reloj(bd.clone(), motor, cfg.correo.clone());

    let puerto = cfg.puerto;
    let estado = rutas::Estado { bd, cfg };

    let app = Router::new()
        .merge(rutas::router(estado))
        // Lo que no es API es la interfaz, que viaja dentro del binario.
        .fallback(get(estaticos::servir))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    let escucha = tokio::net::TcpListener::bind(("0.0.0.0", puerto)).await?;
    tracing::info!("SIGER escuchando en http://0.0.0.0:{puerto}");
    axum::serve(escucha, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("cerrando");
        })
        .await?;
    Ok(())
}
