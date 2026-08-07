// SPDX-License-Identifier: LicenseRef-Xiliux-Comercial
//! **La pantalla, DIBUJADA en un navegador de verdad.**
//!
//! El resto del banco de este repositorio le habla al binario por HTTP y afirma
//! sobre el texto que devuelve axum. Eso mide el CÓDEC. Desde que el panel es
//! `maud` + htmx (directiva 26) la pantalla la termina de construir el
//! navegador: los `hx-swap` los ejecuta JavaScript, y un `<script type="module">`
//! bloqueado por la CSP, servido con el MIME equivocado o que revienta al
//! importar **devuelve el mismo 200 con el mismo cuerpo**.
//!
//! Hasta hoy este repositorio no tenía NINGUNA prueba con navegador —comprobado
//! con control— y ya lleva dos formularios que dependen de que `boi` corra: la
//! confirmación de doble pulsación de «Quitar 2FA» y la de «Desactivar». Si el
//! módulo no cargara, esos botones actuarían a la PRIMERA pulsación y ninguna
//! prueba lo vería.
//!
//! ## Lo que este banco NO cubre, y por qué se dice aquí
//!
//! Solo alcanza las pantallas **sin sesión**. `chromium --dump-dom` no admite
//! mandar una cookie —no existe `--header`, medido el 2026-08-06 en el panel de
//! inventario—, y la única forma de meterla sería abrir una ruta que autentique
//! por GET: un agujero real a cambio de una prueba.
//!
//! Lo que SÍ queda demostrado, y no es poco: que la cadena de módulos entera
//! funciona bajo la CSP real —`boi.js` se descarga, se acepta con
//! `script-src 'self'`, se parsea como módulo y se ejecuta—. Como **todas** las
//! pantallas salen de la MISMA `envoltura`, con los mismos tres `<script>` y la
//! misma CSP, que funcione en la de entrada es lo que extiende el resultado a
//! las demás. Lo que queda sin medir es si un `hx-swap` concreto de una pantalla
//! con sesión inserta donde debe.
//!
//! Se cierra el día que el arnés hable CDP. Mientras tanto, esto es lo que
//! discrimina y cuesta poco.

use std::path::Path;
use std::process::{Child, Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_siger-api");
const SECRETO: &str = "un-secreto-de-sesion-suficientemente-largo-para-la-prueba";

/// Localiza Chromium. Si no está, **falla ruidosamente**: un E2E que se salta
/// solo es indistinguible de uno que pasa (directivas 30 y 33), y éste existe
/// justamente para cruzar la frontera del navegador. `CHROMIUM` lo fuerza.
fn buscar_chromium() -> String {
    if let Ok(p) = std::env::var("CHROMIUM") {
        return p;
    }
    // Sin cadena de `let`: este crate es edición 2021 y MSRV 1.85, y las cadenas
    // de `let` piden 2024. Lo dice el CLAUDE.md del proyecto.
    for c in ["chromium", "chromium-browser", "google-chrome", "google-chrome-stable"] {
        if let Ok(out) = Command::new("sh").arg("-c").arg(format!("command -v {c}")).output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    return p;
                }
            }
        }
    }
    panic!(
        "no se encontró Chromium: este banco DIBUJA la pantalla y no puede saltarse sin \
         dejar de medir (directiva 28). Instale chromium o exporte CHROMIUM=/ruta."
    );
}

/// Pinta `url` y devuelve el DOM ya construido, después de que el JavaScript de
/// la página haya corrido.
///
/// Un perfil ÚNICO por llamada: con uno compartido, dos pruebas en paralelo
/// chocan y Chromium aborta con «Failed to create SingletonLock» —pasó el
/// 2026-08-06 en chasqui—.
fn dibujar(chromium: &str, url: &str) -> String {
    static SIGUIENTE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SIGUIENTE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let perfil = std::env::temp_dir().join(format!("siger-perfil-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&perfil);
    let salida = Command::new(chromium)
        .args([
            "--headless=new",
            "--no-sandbox",
            "--disable-gpu",
            "--disable-dev-shm-usage",
            &format!("--user-data-dir={}", perfil.display()),
            "--virtual-time-budget=4000",
            "--dump-dom",
            url,
        ])
        .output()
        .expect("Chromium no arrancó");
    let _ = std::fs::remove_dir_all(&perfil);
    assert!(
        salida.status.success(),
        "Chromium salió con error: {}",
        String::from_utf8_lossy(&salida.stderr)
    );
    String::from_utf8_lossy(&salida.stdout).into_owned()
}

struct Servidor {
    hijo: Child,
    puerto: u16,
    dir: tempfile::TempDir,
}

impl Drop for Servidor {
    fn drop(&mut self) {
        let _ = self.hijo.kill();
        let _ = self.hijo.wait();
    }
}

/// Arranca el binario REAL en un puerto efímero y espera a su aviso de que está
/// escuchando —no a que el archivo de la base exista, que es una señal anterior
/// a las migraciones—.
fn arrancar(extra: &[(&str, &str)]) -> Servidor {
    let dir = tempfile::tempdir().expect("carpeta temporal");
    std::fs::create_dir_all(dir.path().join("storage/entregas")).ok();
    let mut hijo = spawn(dir.path(), extra);

    let salida = hijo.stdout.take().expect("se pidió stdout por tubería");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for linea in std::io::BufReader::new(salida).lines().map_while(Result::ok) {
            if let Some(i) = linea.find("escuchando en http://") {
                let cola = &linea[i..];
                if let Some(p) = cola.rsplit(':').next().and_then(|s| s.trim().parse::<u16>().ok())
                {
                    let _ = tx.send(p);
                    return;
                }
            }
        }
    });
    let puerto = rx
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("el binario no dijo su puerto en 30 s");

    Servidor { hijo, puerto, dir }
}

fn spawn(dir: &Path, extra: &[(&str, &str)]) -> Child {
    let mut c = Command::new(BIN);
    c.current_dir(dir)
        .env("SIGER_SECRETO_SESION", SECRETO)
        .env("DATABASE_URL", "sqlite://siger.db")
        .env("SIGER_DIR_ENTREGAS", "storage/entregas")
        .env("PORT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (k, v) in extra {
        c.env(k, v);
    }
    c.spawn().expect("no arrancó el binario")
}

#[test]
fn la_pantalla_de_entrada_se_dibuja_y_boi_se_engancha_de_verdad() {
    let chromium = buscar_chromium();
    let s = arrancar(&[]);
    let dom = dibujar(&chromium, &format!("http://127.0.0.1:{}/entrar", s.puerto));
    let _ = &s.dir;

    // Lo que el navegador montó de verdad.
    assert!(dom.contains(r#"id="email""#), "no se dibujó el campo de correo: {dom:.400}");
    assert!(dom.contains(r#"id="clave""#), "no se dibujó el campo de contraseña");
    assert!(dom.contains("SIGER"), "no se dibujó la marca");

    // LA AFIRMACIÓN QUE NO SE PUEDE HACER SOBRE EL HTML DE axum. `boi` viaja como
    // `<script type="module">` y solo deja `data-boi="listo"` en el <html> si el
    // navegador lo descargó, lo aceptó bajo la CSP, lo parseó como módulo y
    // ejecutó `enganchar`. En el cuerpo que sirve axum ese atributo NO está.
    assert!(
        dom.contains(r#"data-boi="listo""#),
        "boi no se enganchó en el navegador: el módulo está en el HTML pero no llegó a \
         ejecutarse (CSP, MIME o un error al importar). Los dos botones de confirmación \
         del panel —«Quitar 2FA» y «Desactivar»— actuarían a la PRIMERA pulsación, y \
         ninguna otra prueba lo vería.\nDOM: {dom:.500}"
    );

    // Y no quedó rastro del SPA que se está retirando.
    assert!(!dom.contains("<app-root"), "quedó un resto del Angular anterior");

    // Negativo de control (directiva 33): sin `SIGER_DEMO_CUENTAS` la pantalla no
    // puede anunciar ninguna credencial. Su pareja es la prueba siguiente.
    assert!(
        !dom.to_lowercase().contains("demostración"),
        "la pantalla anunció credenciales sin haberlas configurado"
    );
}

#[test]
fn el_cartel_de_demostracion_se_dibuja_solo_con_su_variable() {
    // La pareja del control anterior, y lo que lo convierte en medición: el DOM
    // dibujado cambia CON EL ENTORNO del servidor. Sin esta mitad, el `!contains`
    // de arriba pasaría igual con una pantalla en blanco.
    //
    // Es también la comprobación de la regla que dice que NINGUNA instalación que
    // no sea la demostración puede anunciar cuentas: lo que se publica hay que
    // escribirlo entero en el entorno, no hay un `SIGER_DEMO=1` que lo encienda.
    let chromium = buscar_chromium();
    let s = arrancar(&[("SIGER_DEMO_CUENTAS", "demo@siger.co:clave-de-demo:Demostración")]);
    let dom = dibujar(&chromium, &format!("http://127.0.0.1:{}/entrar", s.puerto));
    let _ = &s.dir;

    assert!(
        dom.contains("demo@siger.co"),
        "la variable está puesta y el cartel no se dibujó: {dom:.400}"
    );
    assert!(dom.contains(r#"data-boi="listo""#), "boi no se enganchó con el cartel puesto");
}

#[test]
fn sin_sesion_el_navegador_acaba_en_la_entrada_y_no_ve_el_panel() {
    // La guardia, comprobada a través de un navegador real que SIGUE la
    // redirección — que es como llega quien teclea la dirección a mano.
    let chromium = buscar_chromium();
    let s = arrancar(&[]);
    let dom = dibujar(&chromium, &format!("http://127.0.0.1:{}/entregas", s.puerto));
    let _ = &s.dir;

    assert!(dom.contains(r#"id="clave""#), "no acabó en la pantalla de entrada: {dom:.400}");
    assert!(!dom.contains("class=\"barra\""), "se dibujó la barra del panel sin sesión");
    // Y no filtra los nombres de las secciones, que le dirían a quien no ha
    // entrado qué hay dentro.
    for seccion in ["Entregas", "Cronograma", "Cuentas"] {
        assert!(!dom.contains(seccion), "la entrada nombra la sección «{seccion}»");
    }
}
