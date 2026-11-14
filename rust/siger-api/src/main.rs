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
mod cuentas;
mod db;
mod estadisticas;
mod firma;
mod hex;
mod flujo;
mod importar;
mod importar_secop;
mod informes;
mod licencia;
mod origen;
mod participacion;
mod planificador;
#[cfg(test)]
mod pruebas_cronograma;
mod respaldo;
mod rutas;
mod segundofactor;
mod sesion;
/// El panel, tejido en el servidor con maud (directiva 26).
mod vista;

use axum::Router;
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

    if std::env::args().nth(1).as_deref() == Some("importar-secop") {
        return importar_secop_cmd().await;
    }

    if std::env::args().nth(1).as_deref() == Some("alertas") {
        return alertas_cmd().await;
    }

    if std::env::args().nth(1).as_deref() == Some("revisar-hashes") {
        return revisar_hashes_cmd().await;
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
        // Lo necesita el instalador para crear el primer administrador, y tiene
        // que salir del MISMO código que después verifica.
        //
        // El porqué que este comentario daba hasta el 2026-08-04 estaba AL REVÉS,
        // y medido: decía que un hash con otros parámetros «deja a alguien fuera
        // del sistema». No deja a nadie fuera. Un PHC de Argon2 lleva sus propios
        // parámetros dentro y el verificador los lee de ahí, así que un hash
        // `m=8 t=1` entra igual de bien que el bueno — se comprobó mutando justo
        // esto: `verificar` siguió en verde. El daño es el contrario y no tiene
        // síntoma: la cuenta del administrador, que es la que más vale, queda
        // protegida por un hash 2400 veces más barato y nada lo delata. Ni
        // siquiera `renovar_hash_si_hace_falta` lo arregla solo: lo arregla, sí,
        // pero la primera vez que esa persona entre, y entretanto el archivo de
        // la base ya salió a un respaldo.
        let Some(clave) = std::env::args().nth(2) else {
            eprintln!("uso: siger-api hashear <clave>");
            return Some(std::process::ExitCode::FAILURE);
        };
        // «El mismo código» es literalmente el mismo: `guaca::claves::hashear`, el
        // que llama `cuentas::hashear` al dar de alta y cuya pareja verifica el
        // login. Antes esto eran tres líneas copiadas de `cuentas.rs`: coincidían,
        // pero nada lo obligaba. Lo sujeta, ahora sí por los parámetros,
        // `el_hash_del_cli_abre_la_puerta_y_cuesta_lo_que_debe` en `tests/cli.rs`.
        return Some(match guaca::claves::hashear(&clave) {
            Some(h) => {
                println!("{h}");
                std::process::ExitCode::SUCCESS
            }
            None => {
                eprintln!("no se pudo cifrar la clave");
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

    // Las claves que este subcomando entiende. Es una lista CERRADA, y por un
    // caso concreto: `--salidaa` se tragaba sin rechistar, el campo `salida`
    // quedaba sin poner, y la licencia del cliente aparecía en `licencia.quipu`
    // del directorio actual en vez de donde el operador creyó mandarla. El resto
    // de erratas (`--usuarioss`) acaban en «faltan campos obligatorios», que es
    // ruidoso pero no dice la causa. Lo señaló una revisión de seguridad.
    const CONOCIDAS: &[&str] = &[
        "licenciatario", "nit", "edicion", "perfil", "version",
        "usuarios", "sedes", "modalidad", "mantenimiento-hasta", "salida",
    ];

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
        if !CONOCIDAS.contains(&clave) {
            eprintln!(
                "«--{clave}» no es una opción de este subcomando. \
                 Las que hay: --{}.\n\n{USO}",
                CONOCIDAS.join(", --")
            );
            return std::process::ExitCode::FAILURE;
        }
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

    // Un valor que no es número se DICE. Antes caía a 0 y salía por «faltan
    // campos obligatorios: usuarios», que es ruidoso pero no dice la causa: el
    // operador escribió algo, y el mensaje da a entender que no escribió nada.
    // Es la misma queja muda que la lista cerrada de arriba vino a cerrar.
    let mut mal: Option<String> = None;
    let mut numero = |k: &str| match campos.get(k) {
        None => 0,
        Some(v) => v.parse::<u32>().unwrap_or_else(|_| {
            mal.get_or_insert(format!("--{k} tiene que ser un número entero, y es «{v}»"));
            0
        }),
    };

    let l = licencia::Licencia {
        licenciatario: dame("licenciatario"),
        nit: dame("nit"),
        edicion: dame("edicion"),
        perfil: dame("perfil"),
        version: dame("version"),
        usuarios: numero("usuarios"),
        sedes: numero("sedes"),
        modalidad: dame("modalidad"),
        // La emisión es HOY, con el MISMO reloj que usa el veredicto. No se
        // acepta por argumento: una fecha de emisión que se teclea es una fecha
        // que se puede antedatar, y este campo es de los que un perito miraría.
        emitida: licencia::hoy().to_string(),
        mantenimiento_hasta: dame("mantenimiento-hasta"),
    };

    if let Some(queja) = mal {
        eprintln!("no se emitió nada:\n  {queja}\n\n{USO}");
        return std::process::ExitCode::FAILURE;
    }

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

/// `siger-api importar-secop <csv> <db_destino> [--periodicidad mensual]`:
/// siembra el calendario de obligaciones desde una foto de contratos de SECOP II.
///
/// Es una IMPORTACIÓN, no una integración: lee el CSV una vez y siembra. La
/// periodicidad del informe no viene en el CSV y por eso es un parámetro
/// declarado —mensual por defecto—, nunca un valor supuesto (directiva 20).
/// Falla ruidosamente y en una sola transacción, como el volcado.
async fn importar_secop_cmd() -> std::process::ExitCode {
    const USO: &str = "uso: siger-api importar-secop <csv> <db_destino> \
        [--periodicidad mensual] [--solo <referencia|id|entidad>]\n\n  \
        --solo siembra SÓLO los contratos cuya referencia, id de contrato o \
        entidad contenga ese texto — para la demo por-prospecto «aquí está SU \
        contrato», sin volcar los 336.";
    let (Some(csv), Some(destino)) = (std::env::args().nth(2), std::env::args().nth(3)) else {
        eprintln!("{USO}");
        return std::process::ExitCode::FAILURE;
    };
    // `--periodicidad <valor>` (sin ella, mensual) y `--solo <texto>` (sin él,
    // los 336).
    let args: Vec<String> = std::env::args().skip(4).collect();
    let mut periodicidad = "mensual".to_string();
    let mut solo: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--periodicidad" => match args.get(i + 1) {
                Some(v) => {
                    periodicidad = v.clone();
                    i += 2;
                }
                None => {
                    eprintln!("a --periodicidad le falta el valor.\n{USO}");
                    return std::process::ExitCode::FAILURE;
                }
            },
            "--solo" => match args.get(i + 1) {
                Some(v) => {
                    solo = Some(v.clone());
                    i += 2;
                }
                None => {
                    eprintln!("a --solo le falta el valor.\n{USO}");
                    return std::process::ExitCode::FAILURE;
                }
            },
            otro => {
                eprintln!("no entiendo «{otro}».\n{USO}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }
    let periodicidad = match importar_secop::Periodicidad::desde(&periodicidad) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("\nno se sembró nada:\n  {e}\n");
            return std::process::ExitCode::FAILURE;
        }
    };

    let hoy = chrono::Utc::now().date_naive();
    match importar_secop::importar_secop(&csv, &destino, periodicidad, solo.as_deref(), hoy).await {
        Ok(r) => {
            if let Some(f) = &solo {
                println!("filtrado por «{f}»:");
            }
            println!(
                "sembrado en «{destino}» desde «{csv}»:\n  \
                 {} contratos: {} vivos (con calendario), {} históricos",
                r.contratos, r.vivos, r.historicos
            );
            if r.futuros_pero_cerrados > 0 {
                println!(
                    "  {} con fin futuro NO generan calendario por estar cerrados en SECOP",
                    r.futuros_pero_cerrados
                );
            }
            if r.sin_calendario > 0 {
                println!(
                    "  {} sin calendario (fecha de inicio/fin ausente, ilegible o invertida)",
                    r.sin_calendario
                );
            }
            println!(
                "  {} stakeholders, {} participaciones, {} entregas futuras",
                r.stakeholders, r.participaciones, r.entregas
            );
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("\nla siembra se detuvo sin escribir nada:\n  {e}\n");
            std::process::ExitCode::FAILURE
        }
    }
}

/// `revisar-hashes` — cuántas cuentas están por debajo de la política del KDF.
///
/// # Por qué es un subcomando y no un aviso en el log
///
/// La pregunta que alguien se hace no es «¿este hash está bien?» —eso lo arregla el
/// acceso solo, cuando puede— sino **«¿puedo subir `Params::DEFAULT` sin dejar a
/// nadie con un hash que no se pueda poner al día?»**. Eso es un recuento del
/// padrón, y se consulta cuando se decide, no una línea que se repite en cada
/// login: `renovar_hash_si_hace_falta` avisaba con `warn!` de las cuentas que no
/// puede arreglar, y como nada resuelve ese estado, la misma advertencia sonaba
/// para siempre hasta que nadie la mirara.
///
/// No lee ninguna contraseña ni la necesita: solo parsea la cabecera PHC de cada
/// fila. Es barato y no toca el KDF.
///
/// Sale con FAILURE si hay hashes ilegibles, porque ésos SÍ son un problema —
/// alguien no puede entrar y el login no distingue eso de una clave mala—; con
/// SUCCESS en cualquier otro caso, incluidos los dos cuadrantes flojos, que no son
/// una avería sino una decisión pendiente.
async fn revisar_hashes_cmd() -> std::process::ExitCode {
    let cfg = match config::Config::desde_entorno() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("\nno se pudo leer la configuración:\n  {e}\n");
            return std::process::ExitCode::FAILURE;
        }
    };
    let bd = match db::abrir(&cfg.url_bd).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("no se pudo abrir la base: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let filas: Vec<(i64, String, String)> =
        match sqlx::query_as("SELECT id, email, clave_hash FROM usuarios ORDER BY id")
            .fetch_all(&bd)
            .await
        {
            Ok(f) => f,
            Err(e) => {
                eprintln!("no se pudo leer el padrón: {e}");
                return std::process::ExitCode::FAILURE;
            }
        };

    use cuentas::SituacionHash as S;
    let mut al_dia = 0usize;
    let mut se_suben = 0usize;
    let mut flojos_memoria = Vec::new();
    let mut flojos_trabajo = Vec::new();
    let mut digest_corto = Vec::new();
    let mut ilegibles = Vec::new();
    for (id, email, hash) in &filas {
        match cuentas::situacion_de(hash) {
            S::AlDia => al_dia += 1,
            S::SeSubeAlEntrar => se_suben += 1,
            S::FlojoEnMemoria => flojos_memoria.push((id, email)),
            S::FlojoEnTrabajo => flojos_trabajo.push((id, email)),
            S::DigestCorto => digest_corto.push((id, email)),
            S::Ilegible => ilegibles.push((id, email)),
        }
    }

    let p = argon2::Params::DEFAULT;
    println!("Política de hoy: m={} KiB, t={}, p={}", p.m_cost(), p.t_cost(), p.p_cost());
    println!("{} cuentas en el padrón\n", filas.len());
    println!("  {al_dia:>4}  a la altura de la política, o por encima");
    println!("  {se_suben:>4}  se ponen al día solas en el próximo acceso de su dueño");
    println!("  {:>4}  con menos memoria que la política (no se pueden subir sin rebajarla)", flojos_memoria.len());
    println!("  {:>4}  con menos trabajo total, pero más memoria (déficit acotado bajo 2×)", flojos_trabajo.len());
    println!("  {:>4}  con parámetros suficientes pero DIGEST más corto de 32 bytes", digest_corto.len());
    println!("  {:>4}  ILEGIBLES: no los reconoce el verificador", ilegibles.len());

    for (titulo, lista) in [
        ("Se ponen al día subiendo Params::DEFAULT", &flojos_memoria),
        ("Sin acción: no son más débiles en conjunto", &flojos_trabajo),
        ("Salida corta, parámetros por encima: no se reescriben para no rebajarlos", &digest_corto),
    ] {
        if !lista.is_empty() {
            println!("\n{titulo}:");
            for (id, email) in lista {
                println!("  id={id} {email}");
            }
        }
    }
    if !ilegibles.is_empty() {
        println!("\nNO PUEDEN ENTRAR, y el login les responde lo mismo que a una clave mala:");
        for (id, email) in &ilegibles {
            println!("  id={id} {email}");
        }
        println!("\nRepóngales la contraseña con `siger-api hashear` o desde la administración.");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
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
    // Una pasada incompleta SALE CON FALLO. Esto es lo único que ve un
    // temporizador: hasta hoy devolvía éxito siempre, así que un SMTP mal
    // configurado dejaba `systemctl` en verde para siempre mientras nadie
    // recibía el aviso que es el producto. No contradice la invariante 2 (#71):
    // el código informa, no bloquea — las entregas siguen sin marcar y el
    // siguiente pase las reintenta.
    if r.incompleta() {
        eprintln!("la pasada quedó INCOMPLETA: {}", r.detalle());
        return std::process::ExitCode::FAILURE;
    }
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
        // Ya no hay `fallback` que sirva un SPA: el panel son rutas de `maud`
        // dentro de `rutas::router`, y lo que no case con ninguna es un 404 de
        // verdad. El comodín anterior devolvía `index.html` con 200 a cualquier
        // errata de URL.
        .layer(axum::middleware::from_fn(rutas::cabeceras_de_seguridad))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    let escucha = tokio::net::TcpListener::bind((direccion, puerto)).await?;
    // Se anuncia el puerto QUE SE LIGÓ, no el que se pidió. Con `PORT=0` —que es
    // como arrancan las pruebas para no chocar entre ellas— el sistema elige uno
    // efímero, y el mensaje anterior decía «:0», que no es una dirección a la que
    // nadie pueda conectarse. Un aviso de arranque que no sirve para conectarse
    // no es un aviso de arranque.
    let ligado = escucha.local_addr().map(|d| d.port()).unwrap_or(puerto);
    tracing::info!("SIGER escuchando en http://{direccion}:{ligado}");
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
