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
//! ## Alcanza las pantallas CON sesión desde el 2026-08-08
//!
//! Hasta ese día solo llegaba a las de **sin sesión**, y aquí estaba escrito por
//! qué: `chromium --dump-dom` no admite mandar una cookie —no existe `--header`,
//! medido el 2026-08-06 en el panel de inventario— y la única forma de meterla
//! habría sido abrir una ruta que autentique por GET, o sea un agujero real a
//! cambio de una prueba.
//!
//! Lo cierra **`mirilla`**, la librería que habla CDP por tubería. Y la sesión
//! no se inyecta: se **ENTRA por el formulario**, como entra una persona, así
//! que el acceso queda medido de paso.
//!
//! ## Desde el 2026-08-10 el arnés es una LIBRERÍA, no una copia
//!
//! Nació en `medico` el 2026-08-08 y se copió aquí **sin generalizar** a
//! propósito: extraer antes del segundo consumidor es adivinar la costura. Con
//! el segundo delante se midió el CÓDIGO de las dos copias —no los
//! comentarios— y de ~309 líneas la única diferencia real era la ruta de la
//! entrada. Se extrajo a `github.com/isazajuancarlos/mirilla` (MIT, pública) con
//! eso como único parámetro; entra por `dev-dependencies` fijada por `rev`, así
//! que **no toca el binario de producción**, y con ella se fue `libc`, que solo
//! estaba aquí por las dos llamadas que ahora viven dentro.
//!
//! Lo que se queda en ESTE archivo es lo propio del producto y ya estaba fuera
//! del módulo: arrancar el binario, sembrar la cuenta, y `dibujar` con
//! `--dump-dom` para las pantallas que no necesitan sesión.
//!
//! Las pruebas de aquí siguen usando `--dump-dom` donde basta —es más simple, y
//! una pieza que no hace falta no se usa por tenerla—; las que necesitan sesión
//! usan el navegador dirigido.
//!
//! ## Lo que sigue sin cubrir
//!
//! **Dibuja y ejecuta; no mira píxeles.** Que una tabla salga descuadrada o que
//! un color no contraste no lo ve nadie todavía.

use mirilla::{buscar_chromium, Navegador};
use std::path::Path;
use std::process::{Child, Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_siger-api");
const SECRETO: &str = "un-secreto-de-sesion-suficientemente-largo-para-la-prueba";
/// **La única línea que este producto le aporta al arnés compartido**, y por eso
/// vive aquí y no dentro de `mirilla`: dónde está su formulario de acceso. En
/// medico es `/panel/entrada`. Los nombres de los campos —`email` y `clave`—
/// resultaron ser los mismos en los dos, así que ésos sí son de la librería.
const ENTRADA: &str = "/entrar";


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
    let hijo = Command::new(chromium)
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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Chromium no arrancó");
    let (salida, colgado) = esperar_con_plazo(hijo, PLAZO_CHROMIUM);
    let _ = std::fs::remove_dir_all(&perfil);
    assert!(
        !colgado,
        "Chromium NO terminó en {} s dibujando {url}: se le mató. Un headless que \
         no sale nunca cuelga la corrida entera —pasó en el CI el 2026-08-07, 49 \
         minutos— y un cuelgue no se distingue de una prueba lenta.",
        PLAZO_CHROMIUM.as_secs()
    );
    assert!(
        salida.status.success(),
        "Chromium salió con error: {}",
        String::from_utf8_lossy(&salida.stderr)
    );
    String::from_utf8_lossy(&salida.stdout).into_owned()
}

/// El plazo del PROCESO, que es distinto del `--virtual-time-budget`.
///
/// Ese presupuesto lo respeta un Chromium que funciona; el caso que aquí se
/// teme es el que **no** llega a respetarlo —no encuentra un socket, se queda en
/// un `SingletonLock`, el sandbox del runner le cierra una llamada—. Cuatro
/// segundos de tiempo virtual con 60 de techo real deja holgura de sobra para
/// el arranque frío del navegador y sigue siendo dos órdenes de magnitud menos
/// que los 49 minutos que costó no tenerlo.
const PLAZO_CHROMIUM: std::time::Duration = std::time::Duration::from_secs(60);

/// Espera a que el proceso termine, y **lo mata si se pasa del plazo**.
///
/// Sin dependencias nuevas: un hilo centinela con `recv_timeout` sobre un canal
/// que el camino feliz cierra al terminar. Devuelve además si hubo que matarlo,
/// porque «murió por plazo» y «salió con error» son cosas distintas y confundir
/// las dos es lo que hace que un cuelgue se lea como un fallo de la pantalla.
///
/// # `Timeout` y `Disconnected` NO son lo mismo, y confundirlos mata al sano
///
/// La primera versión preguntaba `recv_timeout(...).is_err()`, y eso es cierto
/// por DOS motivos distintos: que venza el plazo, y que el otro extremo se
/// cierre. El camino feliz cierra el canal justo al terminar bien — o sea que
/// la condición se cumplía **siempre**, y el centinela mataba a un Chromium que
/// ya había salido con éxito. Medido: las tres pruebas de `--dump-dom` en rojo a
/// los 3,6 s con el mensaje «no terminó en 60 s», que es el propio instrumento
/// contradiciéndose. Se distingue el brazo, y por eso este `match` no se puede
/// «simplificar» a un `is_err()`.
fn esperar_con_plazo(hijo: Child, plazo: std::time::Duration) -> (std::process::Output, bool) {
    let pid = hijo.id();
    let colgado = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (fin, aviso) = std::sync::mpsc::channel::<()>();
    let centinela = {
        let colgado = std::sync::Arc::clone(&colgado);
        std::thread::spawn(move || {
            if let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = aviso.recv_timeout(plazo) {
                colgado.store(true, std::sync::atomic::Ordering::SeqCst);
                let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
            }
        })
    };
    let salida = hijo.wait_with_output().expect("no se pudo esperar al proceso");
    drop(fin);
    let _ = centinela.join();
    let colgado = colgado.load(std::sync::atomic::Ordering::SeqCst);
    (salida, colgado)
}

/// **La pareja del centinela**, y hace falta: su fallo es silencioso por los dos
/// lados —si no mata, vuelve el cuelgue de 49 minutos; si mata de más, tumba
/// pruebas sanas, que es justo lo que hizo la primera versión—.
///
/// Dos casos por la MISMA costura que usa `dibujar`, no por un arnés aparte:
/// uno que TIENE que sobrevivir y uno que TIENE que morir.
///
/// **Sin `#[ignore]`, y es deliberado**: es la única prueba de este archivo que
/// no necesita navegador —le habla a `sh`—, cuesta 0,3 s y vigila la pieza que
/// existe para que el CI no se vuelva a colgar. Dejarla fuera del lane sería
/// apagar la alarma en el sitio donde suena.
#[test]
fn el_centinela_mata_al_que_se_cuelga_y_deja_vivir_al_que_termina() {
    // El señuelo se lanza DIRECTO (`echo`/`sleep`), no por `sh -c`, y es la
    // corrección de un defecto que el reinstalar destapó: `dibujar` lanza Chromium
    // directo (`Command::new(chromium)`), así que el señuelo del centinela tiene que
    // hacer lo mismo para ser fiel. Con `sh -c "sleep 30"` y `/bin/sh -> dash`, dash
    // FORKEA: el `sleep` es un NIETO, y el `kill -9 <pid>` del centinela mata al `sh`
    // padre mientras el nieto sigue vivo reteniendo el pipe, así que `wait_with_output`
    // esperaba los 30 s completos y la prueba fallaba en «lo mató el sistema, no el
    // centinela». Antes del reinstalar `/bin/sh` era bash, que ejecuta el comando
    // único de `-c` (pid = sleep) y el `kill` acertaba — por eso llevaba años en verde.
    let rapido = Command::new("echo")
        .arg("listo")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("no arrancó el proceso rápido");
    let (salida, colgado) = esperar_con_plazo(rapido, std::time::Duration::from_secs(30));
    assert!(!colgado, "mató a un proceso que terminó en milisegundos");
    assert!(salida.status.success());
    assert_eq!(String::from_utf8_lossy(&salida.stdout).trim(), "listo");

    let lento = Command::new("sleep")
        .arg("30")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("no arrancó el proceso lento");
    let inicio = std::time::Instant::now();
    let (_, colgado) = esperar_con_plazo(lento, std::time::Duration::from_millis(300));
    assert!(colgado, "NO mató a un proceso que se pasa del plazo: el centinela no sirve");
    assert!(
        inicio.elapsed() < std::time::Duration::from_secs(10),
        "tardó {:?}: lo mató el sistema, no el centinela",
        inicio.elapsed()
    );
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

/// **El comportamiento compartido se sirve y su import resuelve.**
///
/// Desde el 2026-08-07 `confirmar-doble` no es una copia local: es el archivo de
/// `/mnt/data/boi` vendorizado SIN TOCAR, y por eso importa `from "../boi.js"`.
/// Ese import solo resuelve porque se sirve bajo `/comportamientos/`.
///
/// Si alguien lo moviera a la raíz —lo "natural", ya que el anterior vivía
/// ahí— el módulo reventaría al importar **en silencio**: mismo 200, mismo DOM,
/// `data-boi="listo"` intacto porque el núcleo sí carga… y los botones de
/// «Quitar 2FA» y «Desactivar» volverían a actuar a la primera pulsación. La
/// prueba de arriba NO lo vería: mide el núcleo, no el comportamiento.
#[test]
#[ignore = "dibuja con navegador: fuera del lane de CI; lo corre construir.sh con --include-ignored"]
fn el_comportamiento_compartido_se_sirve_y_su_import_resuelve() {
    let s = arrancar(&[]);
    let pedir = |ruta: &str| {
        let salida = Command::new("curl")
            .args([
                "-s",
                "-w",
                "\n%{http_code}",
                "--max-time",
                "15",
                &format!("http://127.0.0.1:{}{ruta}", s.puerto),
            ])
            .output()
            .expect("curl");
        String::from_utf8_lossy(&salida.stdout).into_owned()
    };

    let js = pedir("/comportamientos/confirmar-doble.js");
    assert!(js.trim_end().ends_with("200"), "el comportamiento no se sirvió: {js:.300}");
    assert!(js.contains("data-confirmar"), "no es el comportamiento esperado");
    assert!(
        js.contains(r#"from "../boi.js""#),
        "no es el archivo del repositorio compartido: le editaron el import"
    );
    // El destino de ese import EXISTE. Es lo que hace válido el módulo.
    assert!(
        pedir("/boi.js").trim_end().ends_with("200"),
        "`../boi.js` no resuelve desde el comportamiento: reventaría al importar"
    );
    // La pareja: una ruta hermana inventada NO da 200, así que el 200 de arriba
    // significa algo. Sin esto, un servidor que respondiera a todo pasaría.
    assert!(
        !pedir("/comportamientos/no-existe.js").trim_end().ends_with("200"),
        "una ruta inventada devolvió 200: este banco no discrimina"
    );

    // La mitad ESTÉTICA: boi.css se sirve, es autocontenido (fuentes `data:`,
    // sin CDN) y la CSP admite esas fuentes `data:` —con `font-src 'self'` a
    // secas el navegador bloquea IBM Plex en silencio (medido en el concilio
    // 2026-09-29)—.
    let css = pedir("/boi.css");
    assert!(css.trim_end().ends_with("200"), "boi.css no se sirvió: {css:.200}");
    let css_min = css.to_lowercase();
    assert!(css_min.contains("--boi-acento"), "boi.css no trae sus tokens");
    assert!(!css_min.contains("@import"), "boi.css trae un @import: no es autocontenido");
    assert!(!css_min.contains("url(http"), "boi.css referencia un recurso remoto");

    let cab = Command::new("curl")
        .args(["-s", "-D", "-", "-o", "/dev/null", "--max-time", "15",
               &format!("http://127.0.0.1:{}/entrar", s.puerto)])
        .output()
        .expect("curl");
    let cab = String::from_utf8_lossy(&cab.stdout).to_lowercase();
    assert!(
        cab.contains("font-src 'self' data:"),
        "la CSP no permite fuentes data:: boi.css no cargaría IBM Plex. Cabeceras:\n{cab}"
    );
}

#[test]
#[ignore = "dibuja con navegador: fuera del lane de CI; lo corre construir.sh con --include-ignored"]
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
#[ignore = "dibuja con navegador: fuera del lane de CI; lo corre construir.sh con --include-ignored"]
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
#[ignore = "dibuja con navegador: fuera del lane de CI; lo corre construir.sh con --include-ignored"]
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

// ─────────────────────────────────────────────────────────────────────────────
// Las pantallas CON SESIÓN, desde el 2026-08-08
//
// La cabecera de este archivo decía —y era cierto— que este banco solo alcanzaba
// las pantallas sin sesión, porque `--dump-dom` no admite mandar una cookie. Lo
// cierra el navegador dirigido de `comun`, que habla CDP por tubería y **entra
// por el formulario**, como entra una persona.
// ─────────────────────────────────────────────────────────────────────────────

const CORREO: &str = "operadora@siger.local";
const CLAVE: &str = "clave-de-prueba-larga-y-suficiente";

/// Crea la cuenta como la crea el instalador: con `siger-api hashear`, que es el
/// MISMO código que después verifica. Un hash fabricado en la prueba mediría el
/// arnés y no el producto.
fn con_una_cuenta(s: &Servidor) {
    let salida = Command::new(BIN)
        .args(["hashear", CLAVE])
        .current_dir(s.dir.path())
        .env("SIGER_SECRETO_SESION", SECRETO)
        .output()
        .expect("no se pudo hashear");
    assert!(salida.status.success(), "`hashear` falló");
    let hash = String::from_utf8_lossy(&salida.stdout).trim().to_string();
    assert!(hash.starts_with("$argon2"), "eso no es un PHC de Argon2: {hash}");

    let bd = s.dir.path().join("siger.db");
    let sql = format!(
        "INSERT INTO usuarios (nombre, email, clave_hash, rol, activo) \
         VALUES ('Operadora', '{CORREO}', '{hash}', 'ADMIN', 1);"
    );
    let salida = Command::new("sqlite3")
        .arg(&bd)
        .arg(&sql)
        .output()
        .expect("no está sqlite3");
    assert!(
        salida.status.success(),
        "no se pudo crear la cuenta: {}",
        String::from_utf8_lossy(&salida.stderr)
    );
}

/// **Una pantalla CON sesión, dibujada de verdad.**
///
/// Es lo que este repositorio no había hecho nunca. Lo que se afirma no se puede
/// afirmar sobre el HTML que devuelve axum: `data-boi="listo"` solo aparece si el
/// navegador descargó el módulo, lo aceptó bajo la CSP, lo parseó y lo ejecutó —y
/// en una pantalla del panel, que es donde viven los dos botones de confirmación
/// doble que dependen de él.
#[test]
#[ignore = "dibuja con navegador: fuera del lane de CI; lo corre construir.sh con --include-ignored"]
fn una_pantalla_con_sesion_se_dibuja_y_boi_corre_dentro_del_panel() {
    let chromium = buscar_chromium();
    let s = arrancar(&[]);
    con_una_cuenta(&s);
    let base = format!("http://127.0.0.1:{}", s.puerto);
    let mut nav = Navegador::abrir(&chromium);

    nav.entrar(&base, ENTRADA, CORREO, CLAVE);
    nav.ir(&format!("{base}/entregas"));

    assert_eq!(nav.texto("location.pathname"), "/entregas", "la sesión no llegó al panel");
    assert!(
        nav.evaluar("document.documentElement.dataset.boi === 'listo'")
            == serde_json::json!(true),
        "boi no se enganchó DENTRO del panel: los botones de confirmación doble actuarían \
         a la primera pulsación y ninguna otra prueba lo vería"
    );
    // Y es la pantalla del panel, no la entrada disfrazada.
    assert!(
        nav.evaluar("document.querySelector('.barra') !== null") == serde_json::json!(true),
        "no se dibujó la barra del panel"
    );
}

/// **El caso rojo del arnés** (directiva 33): sin él, la de arriba estaría verde
/// con un instrumento que dice que sí a todo.
///
/// La MISMA ruta, el MISMO navegador dirigido, sin entrar: tiene que acabar en la
/// entrada y no enseñar nada del panel.
#[test]
#[ignore = "dibuja con navegador: fuera del lane de CI; lo corre construir.sh con --include-ignored"]
fn el_navegador_dirigido_distingue_tener_sesion_de_no_tenerla() {
    let chromium = buscar_chromium();
    let s = arrancar(&[]);
    con_una_cuenta(&s);
    let base = format!("http://127.0.0.1:{}", s.puerto);
    let mut nav = Navegador::abrir(&chromium);

    nav.ir(&format!("{base}/entregas"));
    assert_eq!(
        nav.texto("location.pathname"),
        "/entrar",
        "sin sesión, /entregas no llevó a la entrada"
    );
    assert!(
        nav.evaluar("document.querySelector('.barra') === null") == serde_json::json!(true),
        "se dibujó la barra del panel sin sesión"
    );

    // Y la pareja de la pareja: con la MISMA cuenta y la MISMA URL, entrando, sí
    // se ve. Es lo que hace creíble el negativo de arriba.
    nav.entrar(&base, ENTRADA, CORREO, CLAVE);
    nav.ir(&format!("{base}/entregas"));
    assert_eq!(nav.texto("location.pathname"), "/entregas");
}

/// **Una clave equivocada no abre sesión.** `entrar` da por bueno cualquier
/// cambio de URL, así que sin esto un servidor que dejara pasar a todo el mundo
/// pasaría las dos de arriba.
#[test]
#[ignore = "dibuja con navegador: fuera del lane de CI; lo corre construir.sh con --include-ignored"]
fn una_clave_equivocada_no_entra() {
    let chromium = buscar_chromium();
    let s = arrancar(&[]);
    con_una_cuenta(&s);
    let base = format!("http://127.0.0.1:{}", s.puerto);
    let mut nav = Navegador::abrir(&chromium);

    nav.ir(&format!("{base}/entrar"));
    nav.evaluar(
        "document.querySelector('input[name=email]').value = 'operadora@siger.local';\
         document.querySelector('input[name=clave]').value = 'la-que-no-es';\
         document.querySelector('form').submit();",
    );
    nav.esperar(
        "document.querySelector('input[name=clave]') !== null",
        "la pantalla no contestó nada a un acceso fallido",
    );
    assert_eq!(nav.texto("location.pathname"), "/entrar", "una clave equivocada abrió sesión");
    // Y la puerta sigue cerrada de verdad, no solo de aspecto.
    nav.ir(&format!("{base}/entregas"));
    assert_eq!(nav.texto("location.pathname"), "/entrar");
}

// ─────────────────────────────────────────────────────────────────────────────
// ACCESIBILIDAD — la medición que pedía la deuda de la retirada de Angular
//
// `@angular/material` traía la accesibilidad puesta; `maud` + CSS propio no trae
// nada, y el CLAUDE.md ya reconocía eso como «el coste real del cambio». Lo que
// faltaba era MEDIRLO en vez de suponerlo, en las dos direcciones: nadie había
// comprobado que estuviera mal, y nadie había comprobado que estuviera bien.
//
// Se mide contra el ÁRBOL DE ACCESIBILIDAD DEL NAVEGADOR, no contra el HTML.
// Un `label` en la fuente no prueba que el lector de pantalla encuentre el
// nombre: puede estar asociado a un `id` que no existe, tapado por un
// `aria-label` vacío, o pertenecer a un control que la CSP dejó sin dibujar.
// Chrome calcula el nombre accesible para su propio uso y lo entrega entero.
// ─────────────────────────────────────────────────────────────────────────────

/// Los papeles que una persona OPERA. Si uno de éstos no tiene nombre, con un
/// lector de pantalla se anuncia como «botón» a secas y no hay forma de saber
/// qué hace: no es una molestia, es una pantalla inutilizable.
const OPERABLES: &[&str] =
    &["button", "textbox", "combobox", "checkbox", "radio", "link", "searchbox", "spinbutton"];

/// Recorre el árbol de accesibilidad y devuelve los nodos operables SIN nombre.
fn sin_nombre(nodos: &[serde_json::Value]) -> Vec<String> {
    let mut pelados = Vec::new();
    for n in nodos {
        if n.get("ignored").and_then(serde_json::Value::as_bool) == Some(true) {
            continue;
        }
        let papel = n["role"]["value"].as_str().unwrap_or("");
        if !OPERABLES.contains(&papel) {
            continue;
        }
        let nombre = n
            .get("name")
            .and_then(|v| v.get("value"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if nombre.is_empty() {
            let pista = n
                .get("backendDOMNodeId")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "?".into());
            pelados.push(format!("{papel} (nodo {pista})"));
        }
    }
    pelados
}

#[test]
#[ignore = "dibuja con navegador: fuera del lane de CI; lo corre construir.sh con --include-ignored"]
fn ningun_control_del_panel_se_anuncia_sin_nombre() {
    let chromium = buscar_chromium();
    let s = arrancar(&[]);
    con_una_cuenta(&s);
    let base = format!("http://127.0.0.1:{}", s.puerto);
    let mut nav = Navegador::abrir(&chromium);
    nav.entrar(&base, ENTRADA, CORREO, CLAVE);

    let pantallas = [
        "/entregas", "/informes", "/cuentas", "/cronograma",
        "/estadisticas", "/notificaciones", "/partes", "/mi-clave", "/dos-pasos",
    ];

    let mut informe = Vec::new();
    let mut mudas = Vec::new();
    let mut operables = 0usize;
    for p in pantallas {
        nav.ir(&format!("{base}{p}"));
        let nodos = nav.arbol_accesible();

        // POSITIVO DE CONTROL, POR PANTALLA, y sin él esta prueba no vale nada
        // (directiva 30). Una lista vacía de infracciones sale igual de una
        // pantalla impecable que de un árbol de accesibilidad vacío —una sesión
        // que no llegó, una redirección silenciosa a `/entrar`, un `role` que
        // Chrome pasó a llamar de otra forma—: «sana» y «no se miró» se ven
        // idénticas. Contando el TOTAL no bastaría: una pantalla que redirigiera
        // aportaría los dos campos del formulario de acceso y se escondería
        // dentro de la suma de las otras ocho.
        //
        // El umbral es 4 y no exacto: exacto se actualiza a ciegas con cada
        // control nuevo. Toda pantalla del panel trae al menos la barra de
        // navegación, así que 4 es holgado por abajo. Medido el 2026-08-10: 94
        // controles operables en las nueve.
        let n = cuantos_operables(&nodos);
        operables += n;
        if n < 4 {
            mudas.push(format!("{p} ({n})"));
        }

        let pelados = sin_nombre(&nodos);
        if !pelados.is_empty() {
            informe.push(format!("  {p}: {}", pelados.join(", ")));
        }
    }

    assert!(
        mudas.is_empty(),
        "estas pantallas casi no tienen controles operables, así que un veredicto \
         limpio sobre ellas no significa nada: {}. ¿Entró la sesión? ¿Redirigieron?",
        mudas.join(", ")
    );

    assert!(
        informe.is_empty(),
        "controles que un lector de pantalla anuncia SIN NOMBRE ({operables} examinados):\n{}",
        informe.join("\n")
    );
}

/// La pareja del `sin_nombre`: cuántos nodos operables se llegaron a mirar.
fn cuantos_operables(nodos: &[serde_json::Value]) -> usize {
    nodos
        .iter()
        .filter(|n| n.get("ignored").and_then(serde_json::Value::as_bool) != Some(true))
        .filter(|n| OPERABLES.contains(&n["role"]["value"].as_str().unwrap_or("")))
        .count()
}

/// **El calendario de selección múltiple, DIBUJADO y funcionando.**
///
/// La aritmética la prueba `boi` con `node --test`; lo que no se puede probar
/// allí es que el módulo llegue al navegador, pase la CSP, se enganche al campo
/// y que pulsar un día reescriba el campo. Un `data-calendario-multiple` que no
/// enganche deja la pantalla **exactamente como antes** —escribiendo las
/// fechas—, que es un degradado correcto y por eso completamente invisible: el
/// mismo 200 con el mismo cuerpo.
#[test]
#[ignore = "dibuja con navegador: fuera del lane de CI; lo corre construir.sh con --include-ignored"]
fn el_calendario_se_dibuja_y_marcar_un_dia_escribe_en_el_campo() {
    let chromium = buscar_chromium();
    let s = arrancar(&[]);
    con_una_cuenta(&s);
    let base = format!("http://127.0.0.1:{}", s.puerto);
    let mut nav = Navegador::abrir(&chromium);
    nav.entrar(&base, ENTRADA, CORREO, CLAVE);
    nav.ir(&format!("{base}/cronograma"));

    // Se dibujó, y con días dentro.
    nav.esperar(
        "document.querySelectorAll('.boi-calendario td button').length >= 28",
        "el calendario no se dibujó junto al campo de fechas",
    );

    // El campo empieza vacío y pulsar un día lo escribe. Es el sentido que da
    // valor al control: sin él la rejilla sería un adorno.
    assert_eq!(nav.texto("document.querySelector('#fechas').value"), "");
    nav.evaluar("document.querySelector('.boi-calendario td button').click()");
    let escrito = nav.texto("document.querySelector('#fechas').value");
    assert!(
        parece_iso(&escrito),
        "pulsar un día no escribió una fecha ISO en el campo: {escrito:?}"
    );

    // Al marcar, ese día queda anunciado como pulsado. Se comprueba por
    // `aria-pressed`, que es lo que un lector de pantalla dice — no por la clase
    // CSS, que solo cuenta cómo se ve.
    nav.esperar(
        "document.querySelectorAll('.boi-calendario td button[aria-pressed=\"true\"]').length === 1",
        "el día marcado no quedó anunciado como pulsado",
    );

    // Y el otro sentido: lo TECLEADO repinta la rejilla. Se borra el día desde
    // el campo —no desde el botón— y la marca tiene que desaparecer.
    //
    // La fecha se toma de lo que el propio calendario acaba de escribir, y no de
    // un literal: la vista abre en el mes de HOY, así que un `2026-03-01`
    // quemado en la prueba caería fuera de la rejilla visible y fallaría por el
    // calendario, no por el código. Ya pasó al escribirla.
    nav.evaluar(
        "(() => { const c = document.querySelector('#fechas'); \
          c.value = ''; \
          c.dispatchEvent(new Event('input', {bubbles: true})); })()",
    );
    nav.esperar(
        "document.querySelectorAll('.boi-calendario td button[aria-pressed=\"true\"]').length === 0",
        "borrar el campo no repintó el calendario: hay dos verdades en pantalla",
    );

    // La pareja del `type="button"`: si faltara, pulsar un día enviaría el
    // formulario y acabaríamos en otra pantalla.
    assert_eq!(
        nav.texto("location.pathname"),
        "/cronograma",
        "pulsar un día envió el formulario: a los botones les falta type=\"button\""
    );
}

/// `AAAA-MM-DD`, sin arrastrar una dependencia de regex a un banco de pruebas.
fn parece_iso(s: &str) -> bool {
    let b = s.trim().as_bytes();
    b.len() == 10
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..].iter().all(u8::is_ascii_digit)
}

/// Siembra un informe con una entrega PENDIENTE a nombre de la cuenta de
/// `con_una_cuenta`, y devuelve el id de la entrega.
/// Una entrega por cada estado que admite el esquema, para poder mirar los
/// cuatro colores en la misma pantalla.
fn con_una_entrega_de_cada_estado(s: &Servidor) {
    let bd = s.dir.path().join("siger.db");
    let sql = "INSERT INTO informes (id,nombre,recurrencia) VALUES (1,'Informe de gestión','{\"modo\":\"INMEDIATO\"}'); \
               INSERT INTO entregas (id,informe_id,usuario_id,fecha_nominal,fecha_limite,estado) VALUES \
                 (1,1,1,'2026-06-01','2030-12-31','PENDIENTE'), \
                 (2,1,1,'2026-06-02','2030-12-31','CUMPLIDO'), \
                 (3,1,1,'2026-06-03','2030-12-31','EXTEMPORANEO'), \
                 (4,1,1,'2026-06-04','2030-12-31','INCUMPLIDO');";
    let salida = Command::new("sqlite3").arg(&bd).arg(sql).output().expect("no está sqlite3");
    assert!(salida.status.success(), "no se pudo sembrar: {}", String::from_utf8_lossy(&salida.stderr));
}

/// **El color del estado llega al DOM, y lo dice el navegador.**
///
/// No se puede afirmar sobre el HTML que devuelve axum, y ése es el punto: la
/// pantalla emitía `<span class="estado" class="mal">` y **el analizador de
/// HTML se queda con el primero y tira el segundo**, así que el atributo estaba
/// en el cuerpo y la clase no llegaba nunca al elemento. Un `assert` sobre el
/// texto habría pasado en verde con el defecto puesto. Aquí se lee el
/// `classList` ya construido, que es lo único que decide de qué color se pinta.
///
/// Encima había un segundo defecto que el primero tapaba: `clase_estado`
/// comparaba contra «vencida» y «entregada», que no son de este sistema —la
/// columna guarda PENDIENTE, CUMPLIDO, EXTEMPORANEO e INCUMPLIDO—, así que no
/// casaba ninguno. Los dos juntos dejaban las píldoras sin color desde siempre,
/// con la página respondiendo 200 y el texto correcto.
///
/// La pareja va dentro: los cuatro estados tienen que dar cuatro clases
/// DISTINTAS. Sin eso, un mapa que devolviera «espera» a todo pasaría igual.
#[test]
#[ignore = "dibuja con navegador: fuera del lane de CI; lo corre construir.sh con --include-ignored"]
fn el_color_del_estado_llega_al_dom_y_distingue_los_cuatro() {
    let chromium = buscar_chromium();
    let s = arrancar(&[]);
    con_una_cuenta(&s);
    con_una_entrega_de_cada_estado(&s);

    let base = format!("http://127.0.0.1:{}", s.puerto);
    let mut nav = Navegador::abrir(&chromium);
    nav.entrar(&base, ENTRADA, CORREO, CLAVE);
    nav.ir(&format!("{base}/entregas"));

    // Se lee el `classList` de cada píldora, emparejado con su texto. Es el DOM
    // ya construido: si el atributo viniera repetido, la segunda clase no
    // estaría aquí por mucho que aparezca en el cuerpo de la respuesta.
    let leido = nav.evaluar(
        "JSON.stringify([...document.querySelectorAll('.estado')]           .map(e => [e.textContent.trim(), [...e.classList].join(' ')]))",
    );
    let filas: Vec<(String, String)> =
        serde_json::from_str(leido.as_str().unwrap_or("[]")).expect("json del DOM");

    assert_eq!(filas.len(), 4, "no se dibujaron las cuatro entregas: {filas:?}");

    let clase_de = |estado: &str| -> String {
        filas
            .iter()
            .find(|(t, _)| t == estado)
            .unwrap_or_else(|| panic!("no está la píldora de {estado}: {filas:?}"))
            .1
            .clone()
    };

    for estado in ["PENDIENTE", "CUMPLIDO", "EXTEMPORANEO", "INCUMPLIDO"] {
        let c = clase_de(estado);
        assert!(c.contains("estado"), "«{estado}» perdió la clase base: {c}");
        assert!(
            c.split_whitespace().count() >= 2,
            concat!(
                "«{}» llegó al DOM SIN color —el atributo `class` se emitió dos ",
                "veces o el estado no casó—: classList = {:?}"
            ),
            estado,
            c
        );
    }

    // PAREJA: cuatro estados, cuatro colores. Que no se colapsen entregar tarde
    // y no entregar (directiva 32).
    let colores: std::collections::HashSet<String> = ["PENDIENTE", "CUMPLIDO", "EXTEMPORANEO", "INCUMPLIDO"]
        .iter()
        .map(|e| clase_de(e))
        .collect();
    assert_eq!(colores.len(), 4, "dos estados comparten color: {colores:?}");

    // Y el resumen de arriba CUENTA de verdad: decía «0 pendientes, 0 vencidas»
    // encima de una tabla llena, porque filtraba con el mismo vocabulario ajeno.
    let dom = nav.dom();
    assert!(dom.contains("1</span> pendientes"), "el contador de pendientes no cuenta: {dom}");
    assert!(dom.contains("1</span> extemporáneas"), "falta el contador de extemporáneas");
    assert!(dom.contains("1</span> incumplidas"), "falta el contador de incumplidas");
}

fn con_una_entrega_pendiente(s: &Servidor) -> i64 {
    let bd = s.dir.path().join("siger.db");
    let sql = "INSERT INTO informes (id,nombre,recurrencia) VALUES (1,'Informe de gestión','{\"modo\":\"INMEDIATO\"}'); \
               INSERT INTO entregas (id,informe_id,usuario_id,fecha_nominal,fecha_limite,estado) \
               VALUES (7,1,1,'2026-06-01','2030-12-31','PENDIENTE');";
    let salida = Command::new("sqlite3").arg(&bd).arg(sql).output().expect("no está sqlite3");
    assert!(salida.status.success(), "no se pudo sembrar: {}", String::from_utf8_lossy(&salida.stderr));
    7
}

/// **El camino crítico del producto, en la pantalla** — el punto 3 de la #443.
///
/// Hasta hoy el banco de navegador llegaba a ENTRAR y ahí se paraba. Entrar lo
/// hace cualquier aplicación; lo que distingue a SIGER de una hoja de cálculo es
/// que la entrega se registre y quede constando. Eso viaja por `multipart`,
/// pasa por `subir_de` y vuelve como una pantalla entera redibujada, y **nada de
/// eso se ve desde un `assert` sobre el HTML que devuelve axum**.
///
/// La pareja va dentro y es la que le da valor: se mira la MISMA pantalla de
/// lista, con la MISMA sesión, antes y después. Lo único que cambia entre las
/// dos miradas es que se subió el archivo. Sin la mirada de antes, un panel que
/// dijera «CUMPLIDA» desde el principio —o una lista que ignorara el estado—
/// pasaría en verde.
#[test]
#[ignore = "dibuja con navegador: fuera del lane de CI; lo corre construir.sh con --include-ignored"]
fn una_entrega_subida_desde_el_navegador_queda_registrada_y_se_ve_en_la_lista() {
    let chromium = buscar_chromium();
    let s = arrancar(&[]);
    con_una_cuenta(&s);
    let id = con_una_entrega_pendiente(&s);

    let base = format!("http://127.0.0.1:{}", s.puerto);
    let mut nav = Navegador::abrir(&chromium);
    nav.entrar(&base, ENTRADA, CORREO, CLAVE);

    // ANTES: la lista la da por pendiente y no hay archivo que descargar.
    nav.ir(&format!("{base}/entregas"));
    let antes = nav.dom();
    assert!(antes.contains("Informe de gestión"), "la entrega sembrada no sale en la lista: {antes}");
    assert!(
        !antes.contains(&format!("/api/entregas/{id}/archivo")),
        "todavía no debería haber archivo que descargar"
    );
    assert!(
        !antes.contains("CUMPLIDO"),
        "antes de entregar nada puede constar como cumplido: {antes}"
    );

    // Se sube por el FORMULARIO, con un archivo de verdad metido en el `input`.
    // `DataTransfer` es la única forma de poblar un `input[type=file]` desde el
    // documento; el envío es un `submit` normal, sin htmx, así que después hay
    // una navegación de página entera que esperar.
    nav.ir(&format!("{base}/entregas/{id}"));
    nav.evaluar(
        "(() => { \
           const i = document.querySelector('input[type=file][name=archivo]'); \
           const dt = new DataTransfer(); \
           dt.items.add(new File(['concepto,valor\\nejecucion,100\\n'], 'informe.csv', {type:'text/csv'})); \
           i.files = dt.files; \
           i.dispatchEvent(new Event('change', {bubbles:true})); \
           i.form.submit(); \
           return true; })()",
    );
    // Esto ESPERA a que la navegación termine; no fija el cartel. Medido: la
    // frase también la produce el historial, así que quitarle el aviso a
    // `entrega_subir` no pone roja esta línea —se comprobó mutándolo—. Lo que la
    // prueba sí fija es lo de abajo, sobre la lista.
    //
    // El `?.` NO es adorno: `submit()` navega la página entera, y mientras el
    // documento nuevo se construye hay una ventana en la que `document.body` es
    // `null`. La sonda cae ahí, la expresión revienta y `esperar` entra en
    // pánico. No se veía en debug y **sí contra el binario de release**, que
    // responde antes: el mismo caso al que apunta la directiva 30 —el fallo
    // estaba, y el camino no llegaba a recorrerlo—.
    nav.esperar(
        "(document.body?.textContent ?? '').includes('Entrega registrada')",
        "la subida nunca llegó a devolver la pantalla de la entrega",
    );

    // DESPUÉS: la misma lista, la misma sesión — y ahora sí consta.
    nav.ir(&format!("{base}/entregas"));
    let despues = nav.dom();
    assert!(
        despues.contains(&format!("/api/entregas/{id}/archivo")),
        "la entrega no aparece con su archivo en la lista: {despues}"
    );
    // **El nombre que se guarda NO es el que mandó el cliente**, y eso hay que
    // fijarlo aquí: el archivo queda como `entrega-<id>-<marca>.csv`. Dar por
    // bueno el nombre de quien sube fue exactamente el agujero del portal
    // anterior; se conserva la extensión y nada más. Medido, no supuesto: la
    // primera versión de esta prueba buscaba «informe.csv» y salió roja.
    assert!(
        !despues.contains("informe.csv"),
        "se está guardando el nombre que mandó el cliente: {despues}"
    );
    let guardado = despues
        .split("/api/entregas/7/archivo\">")
        .nth(1)
        .and_then(|t| t.split('<').next())
        .unwrap_or("")
        .to_string();
    assert!(
        guardado.starts_with("entrega-7-") && guardado.ends_with(".csv"),
        "el archivo no se guardó con el nombre canónico: {guardado:?}"
    );

    // Y el estado cambió de verdad, que es lo que el producto vende.
    assert!(
        despues.contains("CUMPLIDO"),
        "la entrega no consta como cumplida después de entregarla: {despues}"
    );

}
