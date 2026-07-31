//! SIGER — un solo binario: la interfaz, el esquema, el motor y el servidor.
//!
//! ```text
//! SIGER_SECRETO_SESION=$(siger-api generar-secreto) siger-api
//! ```
//!
//! Sin Python, sin `pip`, sin Node y sin base de datos que administrar. El
//! cliente copia un archivo y lo ejecuta; la base se crea y se migra sola.

mod acceso;
mod alertas;
mod autz;
mod config;
mod db;
mod estadisticas;
mod estaticos;
mod firma;
mod flujo;
mod importar;
mod informes;
mod licencia;
mod origen;
mod participacion;
mod planificador;
#[cfg(test)]
mod pruebas_cronograma;
mod respaldo;
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

    if matches!(
        std::env::args().nth(1).as_deref(),
        Some("respaldar") | Some("restaurar")
    ) {
        return respaldo_cmd().await;
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

    if std::env::args().nth(1).as_deref() == Some("generar-claves-licencia") {
        // El par del LICENCIANTE, distinto del de las entregas: uno acredita
        // quién emitió la licencia y el otro qué se entregó. Compartirlos ataría
        // la capacidad de emitir licencias a una clave que vive en el .env de
        // cada instalación de cliente.
        let (vk, sk) = crate::licencia::generar_claves();
        println!("INFORMES_LICENCIA_PUBKEY={vk}   # va al .env del CLIENTE");
        println!("INFORMES_LICENCIA_SK={sk}   # SECRETA: se queda en casa del licenciante");
        return Some(std::process::ExitCode::SUCCESS);
    }

    if std::env::args().nth(1).as_deref() == Some("emitir-licencia") {
        return Some(emitir_licencia_cmd());
    }

    None
}

/// `siger-api emitir-licencia --licenciatario … --nit … …`
///
/// **Lo corre EL LICENCIANTE, no el cliente.** Sale del MISMO binario que
/// verifica, por la razón de siempre (ver `hashear`): un emisor aparte tendría su
/// propia copia del formato canónico y el día que divergieran las licencias
/// dejarían de verificarse sin que nadie supiera por qué.
///
/// La clave de firma se lee de `INFORMES_LICENCIA_SK` y **no** se acepta por
/// argumento: en la línea de órdenes quedaría en el historial del intérprete y a
/// la vista de cualquier `ps`.
fn emitir_licencia_cmd() -> std::process::ExitCode {
    const USO: &str = "uso: siger-api emitir-licencia \\\n  \
        --licenciatario <razón social> --nit <nit> \\\n  \
        --edicion <esencial|profesional|corporativa> \\\n  \
        --perfil <standalone|onprem|cloud> \\\n  \
        --modalidad <asistente|asistida|air-gapped> \\\n  \
        --version <versión licenciada> --usuarios <n> --sedes <n> \\\n  \
        --mantenimiento-hasta <AAAA-MM-DD> [--salida <archivo>]\n\n\
        La clave de firma sale de INFORMES_LICENCIA_SK (hex), nunca de un argumento.\n\
        --mantenimiento-hasta es el fin del PERÍODO DE MANTENIMIENTO, no de la\n\
        licencia: el derecho de uso es perpetuo (Cláusulas 3.1, 3.2 y 7.6 del EULA).";

    // Los argumentos, en pares `--clave valor`. Uno impar o repetido es un error
    // y no se adivina: una licencia emitida con un campo que el operador creía
    // haber puesto es peor que un mensaje de uso.
    let args: Vec<String> = std::env::args().skip(2).collect();
    let mut campos: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let Some(clave) = args[i].strip_prefix("--") else {
            eprintln!("no entiendo «{}».\n\n{USO}", args[i]);
            return std::process::ExitCode::FAILURE;
        };
        let Some(valor) = args.get(i + 1) else {
            eprintln!("a --{clave} le falta el valor.\n\n{USO}");
            return std::process::ExitCode::FAILURE;
        };
        if campos.insert(clave.to_string(), valor.clone()).is_some() {
            eprintln!("--{clave} viene dos veces; ¿cuál vale?\n\n{USO}");
            return std::process::ExitCode::FAILURE;
        }
        i += 2;
    }

    let dame = |k: &str| campos.get(k).cloned().unwrap_or_default();
    let numero = |k: &str| campos.get(k).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);

    let l = licencia::Licencia {
        licenciatario: dame("licenciatario"),
        nit: dame("nit"),
        edicion: dame("edicion"),
        perfil: dame("perfil"),
        version: dame("version"),
        usuarios: numero("usuarios"),
        sedes: numero("sedes"),
        modalidad: dame("modalidad"),
        // La emisión es HOY. No se acepta por argumento: una fecha de emisión
        // que se teclea es una fecha de emisión que se puede antedatar, y este
        // campo es de los que un perito miraría.
        emitida: chrono::Local::now().date_naive().to_string(),
        mantenimiento_hasta: dame("mantenimiento-hasta"),
    };

    let Ok(sk) = std::env::var("INFORMES_LICENCIA_SK") else {
        eprintln!(
            "falta INFORMES_LICENCIA_SK (la clave de firma, en hex).\n\
             Genere el par con `siger-api generar-claves-licencia`.\n\n{USO}"
        );
        return std::process::ExitCode::FAILURE;
    };

    let firmado = match licencia::emitir(&l, &sk) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("no se emitió nada:\n  {e}\n\n{USO}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let salida = campos
        .get("salida")
        .cloned()
        .unwrap_or_else(|| licencia::ARCHIVO_POR_DEFECTO.to_string());
    // No se sobrescribe una licencia existente sin decirlo: el archivo de al
    // lado puede ser el de otro cliente.
    if std::path::Path::new(&salida).exists() {
        eprintln!("«{salida}» ya existe; no lo piso. Elija otro --salida o borre ese.");
        return std::process::ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::write(&salida, &firmado) {
        eprintln!("no se pudo escribir «{salida}»: {e}");
        return std::process::ExitCode::FAILURE;
    }
    println!(
        "licencia emitida en «{salida}» para {} ({}): {} · {} · {} usuarios · {} sedes\n\
         mantenimiento hasta {} — el derecho de uso es PERPETUO y no caduca con esa fecha.",
        l.licenciatario, l.nit, l.edicion, l.perfil, l.usuarios, l.sedes, l.mantenimiento_hasta
    );
    std::process::ExitCode::SUCCESS
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

/// `siger-api respaldar <carpeta>` y `siger-api restaurar <carpeta>`.
///
/// Los dos leen del entorno DÓNDE está la base y dónde los archivos, así que
/// respaldan lo que el servicio usa de verdad y no lo que alguien creía. Es la
/// misma razón por la que el hash lo produce el binario que después lo verifica.
async fn respaldo_cmd() -> std::process::ExitCode {
    let orden = std::env::args().nth(1).unwrap_or_default();
    let Some(carpeta) = std::env::args().nth(2) else {
        eprintln!(
            "uso: siger-api {orden} <carpeta>\n\n\
             respaldar  copia la base EN CALIENTE y los archivos de las entregas,\n\
             \x20          y verifica la copia antes de darla por buena.\n\
             restaurar  la devuelve a su sitio. Pare el servicio antes.\n\
             \x20          --forzar para pisar una base que ya tiene datos."
        );
        return std::process::ExitCode::FAILURE;
    };
    let carpeta = std::path::PathBuf::from(carpeta);

    // La configuración se lee entera, con su validación: sin secreto de sesión
    // esto no arranca. Es a propósito — respaldar la base de una instalación
    // cuya configuración no es válida daría un respaldo de algo que no corre.
    let cfg = match config::Config::desde_entorno() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("\nno se pudo leer la configuración:\n  {e}\n");
            return std::process::ExitCode::FAILURE;
        }
    };

    let resultado = if orden == "respaldar" {
        respaldo::respaldar(&cfg.url_bd, &cfg.dir_entregas, &carpeta).await
    } else {
        let forzar = std::env::args().any(|a| a == "--forzar");
        respaldo::restaurar(&carpeta, &cfg.url_bd, &cfg.dir_entregas, forzar).await
    };

    match resultado {
        Ok(r) => {
            // La preposición importa: «respaldado EN la carpeta» y «restaurado
            // DESDE la carpeta» son direcciones opuestas, y con un único verbo
            // el mensaje de restaurar parecía decir que había escrito en el
            // respaldo. Quien lee esto lo lee el peor día.
            let frase = if orden == "respaldar" {
                format!("Respaldado en «{}»", carpeta.display())
            } else {
                format!(
                    "Restaurado desde «{}» a «{}»",
                    carpeta.display(),
                    cfg.url_bd
                )
            };
            println!(
                "{frase}: {} filas en {} tablas y {} archivos de entrega.",
                r.total_filas(),
                r.filas.len(),
                r.archivos
            );
            for (tabla, n) in &r.filas {
                println!("  {tabla:<16} {n}");
            }
            if orden == "respaldar" {
                println!("\nLa copia se abrió y se comprobó: integridad, recuentos y archivos.");
            }
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("\n{e}\n");
            std::process::ExitCode::FAILURE
        }
    }
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
    // Dos motores desde EL MISMO calendario: uno para el planificador y otro para
    // la pantalla del cronograma. Si se construyeran de dos fuentes, lo que la
    // pantalla muestra podría diferir de lo que después se genera, y eso no se
    // notaría hasta que un plazo venciera solo.
    let motor_web = std::sync::Arc::new(siger_core::Motor::nuevo(calendario.clone()));
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
    let direccion = cfg.escucha;
    let estado = rutas::Estado { bd, cfg, motor: motor_web };

    let app = Router::new()
        .merge(rutas::router(estado))
        // Lo que no es API es la interfaz, que viaja dentro del binario.
        .fallback(get(estaticos::servir))
        // DESPUÉS del `fallback`, para que las cabeceras cubran también la
        // interfaz: la CSP no sirve de nada si va solo en las respuestas de la
        // API y no en el HTML que el navegador interpreta.
        .layer(axum::middleware::from_fn(rutas::cabeceras_de_seguridad))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    let escucha = tokio::net::TcpListener::bind((direccion, puerto)).await?;
    tracing::info!("SIGER escuchando en http://{direccion}:{puerto}");
    // `with_connect_info` y no `into_make_service` a secas: sin esto no hay
    // dirección del socket, y el freno del login se queda sin la mitad de su
    // clave (ver `origen`).
    axum::serve(escucha, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("cerrando");
        })
        .await?;
    Ok(())
}
