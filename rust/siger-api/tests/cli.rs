//! El CLI, ejercido como lo ejerce quien lo usa: por su **código de salida**.
//!
//! # Por qué existe este archivo
//!
//! Hasta hoy había 56 pruebas y ninguna arrancaba el binario. Esa es la misma
//! laguna que en tunjo produjo seis rondas de revisión seguidas: el código estaba
//! probado y la SALIDA no, así que un comando podía informar de un éxito que no
//! había ocurrido y ninguna prueba lo veía.
//!
//! Importa el doble en `respaldar`: un respaldo que sale con 0 habiendo fallado
//! es peor que no tener respaldo, porque el cron queda verde y nadie mira. Aquí
//! se comprueba que **discrimina**: sale 0 cuando la copia es fiel y sale distinto
//! de 0 en cada forma conocida de no serlo.
//!
//! Las pruebas arrancan el binario de verdad (`CARGO_BIN_EXE_siger-api`), no una
//! función interna: la diferencia entre las dos es justo lo que no se estaba
//! probando.

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_siger-api");

/// Un secreto válido para las órdenes que leen la configuración entera. No es
/// una credencial: es el mínimo que el binario exige para no arrancar a ciegas.
const SECRETO: &str = "prueba-de-cli-con-mas-de-treinta-y-dos-bytes-de-largo";

fn correr(args: &[&str], dir: &Path, entorno: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(BIN);
    cmd.args(args).current_dir(dir);
    // `env_clear` no: el binario necesita el entorno mínimo del sistema. Se
    // limpian solo las variables que estas pruebas podrían heredar de la sesión
    // de quien las corre y que cambiarían el resultado sin que se vea.
    for clave in ["SIGER_SECRETO_SESION", "DATABASE_URL", "SIGER_DIR_ENTREGAS", "ALERTAS_ACTIVAS"] {
        cmd.env_remove(clave);
    }
    for (k, v) in entorno {
        cmd.env(k, v);
    }
    cmd.output().expect("el binario no se pudo ejecutar")
}

fn codigo(o: &Output) -> i32 {
    o.status.code().unwrap_or(-1)
}

fn texto(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// El entorno mínimo de una instalación: secreto, base y almacén.
fn entorno_valido() -> Vec<(&'static str, &'static str)> {
    vec![
        ("SIGER_SECRETO_SESION", SECRETO),
        ("DATABASE_URL", "sqlite://siger.db"),
        ("SIGER_DIR_ENTREGAS", "storage/entregas"),
    ]
}

/// Crea una instalación con esquema y un dato, arrancando el binario un momento.
fn instalacion(dir: &Path) {
    let mut hijo = Command::new(BIN)
        .current_dir(dir)
        .env("SIGER_SECRETO_SESION", SECRETO)
        .env("DATABASE_URL", "sqlite://siger.db")
        .env("PORT", "0")
        // `tracing_subscriber::fmt()` escribe en STDOUT, que es donde sale el
        // aviso que se espera abajo.
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("no arrancó");

    // Se espera al AVISO DE QUE ESTÁ ESCUCHANDO, y no a que el archivo de la base
    // exista y pese algo.
    //
    // Esa era la condición anterior, y medía otra cosa: SQLite escribe la
    // cabecera del archivo al abrir el pool, ANTES de que corran las migraciones.
    // Entre las dos cosas hay una ventana en la que el archivo existe, no está
    // vacío, y todavía no hay tabla `usuarios` — así que el `kill` podía caer en
    // medio de la migración y dejar una base a medias. En esta máquina casi nunca
    // pasaba; en la del CI, sí. Un `sleep` generoso lo taparía haciendo la suite
    // lenta, y uno corto la haría intermitente: lo que hay que esperar es la
    // señal que el binario emite cuando de verdad terminó de arrancar, que va
    // después de migrar y de ligar el puerto.
    let salida = hijo.stdout.take().expect("se pidió stdout por tubería");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for linea in std::io::BufReader::new(salida).lines().map_while(Result::ok) {
            if linea.contains("escuchando") {
                let _ = tx.send(());
                return;
            }
        }
    });
    let arrancado = rx.recv_timeout(std::time::Duration::from_secs(30)).is_ok();

    hijo.kill().ok();
    hijo.wait().ok();
    assert!(arrancado, "el binario no terminó de arrancar en 30 s");
    assert!(dir.join("siger.db").exists(), "arrancó sin dejar la base");
}

#[test]
fn hashear_sin_clave_falla_y_con_clave_sale_un_argon2() {
    let tmp = tempfile::tempdir().unwrap();

    let sin = correr(&["hashear"], tmp.path(), &[]);
    assert_ne!(codigo(&sin), 0, "hashear sin argumento salió 0");
    assert!(texto(&sin).contains("uso:"), "no explicó cómo se usa");

    let con = correr(&["hashear", "clave-de-prueba"], tmp.path(), &[]);
    assert_eq!(codigo(&con), 0, "{}", texto(&con));
    assert!(
        String::from_utf8_lossy(&con.stdout).starts_with("$argon2"),
        "el hash no es Argon2: {}",
        texto(&con)
    );
}

/// **El hash del CLI abre la puerta Y cuesta lo que la política dice.**
///
/// Son dos afirmaciones y hacen falta las dos, porque **la primera no implica la
/// segunda y eso es contraintuitivo**: un hash Argon2 lleva sus parámetros dentro
/// del propio PHC y el verificador los lee de ahí, así que un subcomando que
/// hasheara con `m=8 t=1` produciría una credencial que **entra perfectamente**.
/// No hay ningún síntoma: el alta funciona, el login funciona, y el primer
/// administrador del cliente queda protegido por un hash 2400 veces más barato
/// que el de todos los demás.
///
/// Medido por mutación el 2026-08-04: con `m=8 t=1 p=1` en `main.rs`, la
/// comprobación de `verificar` sigue verde y solo cae la de los parámetros. Por
/// eso el comentario que este subcomando llevaba —«otros parámetros dejan a
/// alguien fuera del sistema»— estaba al revés: el peligro no es que no entre, es
/// que entre.
///
/// Se verifica con `guaca::claves::verificar`, que es **la función que el login
/// llama de verdad** (`cuentas::clave_correcta`), no con una reimplementación:
/// una prueba que verificara con su propio Argon2 mediría el códec, no la puerta.
/// Y va con su pareja negativa —una contraseña distinta NO abre—: sin ella
/// pasaría igual un verificador que diga que sí a todo.
#[test]
fn el_hash_del_cli_abre_la_puerta_y_cuesta_lo_que_debe() {
    let tmp = tempfile::tempdir().unwrap();
    let salida = correr(&["hashear", "clave-de-prueba"], tmp.path(), &[]);
    assert_eq!(codigo(&salida), 0, "{}", texto(&salida));
    let hash = String::from_utf8_lossy(&salida.stdout).trim().to_string();

    assert!(
        guaca::claves::verificar("clave-de-prueba", &hash),
        "el hash que imprime el CLI no lo acepta el login: {hash}"
    );
    assert!(
        !guaca::claves::verificar("otra-clave", &hash),
        "el login aceptó una contraseña distinta contra ese hash"
    );

    // La mitad que la mutación obligó a escribir. `Params::DEFAULT` es la
    // política de contraseñas de SIGER (m=19456, t=2, p=1) — la misma que
    // `cuentas::por_debajo_de_la_politica` usa como vara, y la que aplica
    // `guaca::claves::hashear` por dentro.
    let phc = argon2::PasswordHash::new(&hash).expect("el hash del CLI se lee como PHC");
    assert_eq!(phc.algorithm.as_str(), "argon2id", "el CLI no emitió Argon2id");
    let params = argon2::Params::try_from(&phc).expect("el PHC trae parámetros de Argon2");
    let politica = argon2::Params::DEFAULT;
    assert_eq!(
        (params.m_cost(), params.t_cost(), params.p_cost()),
        (politica.m_cost(), politica.t_cost(), politica.p_cost()),
        "el CLI hashea con otros parámetros que el resto del sistema: \
         la credencial ENTRA igual, y por eso no se ve"
    );
}

#[test]
fn generar_secreto_sale_bien_y_es_largo() {
    let tmp = tempfile::tempdir().unwrap();
    let o = correr(&["generar-secreto"], tmp.path(), &[]);
    assert_eq!(codigo(&o), 0, "{}", texto(&o));
    let secreto = String::from_utf8_lossy(&o.stdout).trim().to_string();
    assert!(secreto.len() >= 32, "secreto corto: {} bytes", secreto.len());

    // Y dos llamadas no dan lo mismo: un «generador» constante pasaría todas las
    // comprobaciones de longitud del mundo.
    let otro = correr(&["generar-secreto"], tmp.path(), &[]);
    assert_ne!(secreto, String::from_utf8_lossy(&otro.stdout).trim());
}

#[test]
fn respaldar_sin_carpeta_falla_y_dice_como_se_usa() {
    let tmp = tempfile::tempdir().unwrap();
    let o = correr(&["respaldar"], tmp.path(), &entorno_valido());
    assert_ne!(codigo(&o), 0);
    assert!(texto(&o).contains("uso:"), "{}", texto(&o));
}

#[test]
fn respaldar_sin_secreto_falla_en_vez_de_respaldar_a_ciegas() {
    let tmp = tempfile::tempdir().unwrap();
    instalacion(tmp.path());
    // Sin `SIGER_SECRETO_SESION`: la configuración no es válida, así que la
    // instalación no corre. Respaldar algo que no corre daría una copia de un
    // sistema que nadie está usando y una falsa sensación de cobertura.
    let o = correr(&["respaldar", "copia"], tmp.path(), &[("DATABASE_URL", "sqlite://siger.db")]);
    assert_ne!(codigo(&o), 0, "respaldó sin configuración válida");
}

#[test]
fn el_ciclo_completo_sale_cero_y_deja_lo_que_dice() {
    let tmp = tempfile::tempdir().unwrap();
    instalacion(tmp.path());

    let o = correr(&["respaldar", "copia"], tmp.path(), &entorno_valido());
    assert_eq!(codigo(&o), 0, "{}", texto(&o));
    assert!(tmp.path().join("copia").join("siger.db").exists());
    assert!(tmp.path().join("copia").join("RESPALDO.txt").exists());
    assert!(
        texto(&o).contains("se comprobó"),
        "no dijo que hubiera verificado: {}",
        texto(&o)
    );

    // Y restaurar sobre una base vacía sale 0.
    std::fs::remove_file(tmp.path().join("siger.db")).unwrap();
    let r = correr(&["restaurar", "copia"], tmp.path(), &entorno_valido());
    assert_eq!(codigo(&r), 0, "{}", texto(&r));
    assert!(
        texto(&r).contains("Restaurado desde"),
        "el mensaje no dice la dirección de la copia: {}",
        texto(&r)
    );
}

#[test]
fn respaldar_sobre_una_carpeta_ocupada_sale_distinto_de_cero() {
    let tmp = tempfile::tempdir().unwrap();
    instalacion(tmp.path());
    std::fs::create_dir_all(tmp.path().join("copia")).unwrap();
    std::fs::write(tmp.path().join("copia").join("algo.txt"), b"x").unwrap();

    let o = correr(&["respaldar", "copia"], tmp.path(), &entorno_valido());
    assert_ne!(codigo(&o), 0, "pisó una carpeta ocupada: {}", texto(&o));
}

#[test]
fn restaurar_una_copia_rota_sale_distinto_de_cero() {
    // La prueba que justifica el archivo entero: el comando tiene que
    // DISCRIMINAR. Un respaldo corrupto que se restaura con éxito aparente es la
    // forma exacta en que se pierden los datos el día que hacen falta.
    let tmp = tempfile::tempdir().unwrap();
    instalacion(tmp.path());
    correr(&["respaldar", "copia"], tmp.path(), &entorno_valido());

    use std::io::{Seek, SeekFrom, Write};
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .open(tmp.path().join("copia").join("siger.db"))
        .unwrap();
    f.seek(SeekFrom::Start(4096)).unwrap();
    f.write_all(&[0xFF; 8192]).unwrap();
    f.sync_all().unwrap();
    drop(f);

    std::fs::remove_file(tmp.path().join("siger.db")).unwrap();
    let o = correr(&["restaurar", "copia"], tmp.path(), &entorno_valido());
    assert_ne!(codigo(&o), 0, "restauró una copia corrompida: {}", texto(&o));
}

#[test]
fn restaurar_desde_una_carpeta_que_no_es_un_respaldo_sale_distinto_de_cero() {
    let tmp = tempfile::tempdir().unwrap();
    instalacion(tmp.path());
    std::fs::create_dir_all(tmp.path().join("no-es")).unwrap();

    let o = correr(&["restaurar", "no-es"], tmp.path(), &entorno_valido());
    assert_ne!(codigo(&o), 0);
    assert!(texto(&o).contains("siger.db"), "{}", texto(&o));
}

#[test]
fn alertas_dormidas_sale_cero_y_lo_dice() {
    // El modo degradado tiene que ser un ÉXITO, no un fallo: un cron que se pone
    // rojo cada noche porque las alertas están apagadas a propósito es un cron
    // que nadie vuelve a mirar.
    let tmp = tempfile::tempdir().unwrap();
    instalacion(tmp.path());
    let o = correr(&["alertas"], tmp.path(), &entorno_valido());
    assert_eq!(codigo(&o), 0, "{}", texto(&o));
    assert!(texto(&o).contains("dormidas"), "{}", texto(&o));
}

#[test]
fn importar_sin_argumentos_falla() {
    let tmp = tempfile::tempdir().unwrap();
    let o = correr(&["importar"], tmp.path(), &entorno_valido());
    assert_ne!(codigo(&o), 0);
    assert!(texto(&o).contains("uso:"), "{}", texto(&o));
}

#[test]
fn revisar_hashes_lee_el_padron_y_dice_la_politica_de_verdad() {
    // Este subcomando existe porque la pregunta —«¿puedo subir `Params::DEFAULT`
    // sin dejar a nadie atrás?»— es un recuento del padrón, y antes se contestaba
    // con un `warn!` en CADA acceso de las mismas cuentas: una alarma que nada
    // cierra es una alarma que nadie mira.
    //
    // Aquí se comprueba lo que solo se puede comprobar cruzando la frontera del
    // proceso: que el subcomando esté cableado, abra la base de verdad, y que la
    // política que imprime salga de `Params::DEFAULT` y no de una copia escrita a
    // mano en el mensaje. La CLASIFICACIÓN de cada situación —incluido el hash
    // ilegible que hace salir distinto de cero— se prueba en `cuentas::pruebas::
    // situacion_de_clasifica_cada_caso`, y se prueba allí y no aquí porque sembrar
    // hashes a mano exigiría un `sqlite3` que este entorno no tiene: una prueba que
    // depende de un binario externo se cae en la máquina donde no está, y el rojo
    // no se parece a su causa.
    let tmp = tempfile::tempdir().unwrap();
    instalacion(tmp.path());
    let o = correr(&["revisar-hashes"], tmp.path(), &entorno_valido());
    let t = texto(&o);
    assert_eq!(codigo(&o), 0, "una instalación recién hecha no tiene nada malo:\n{t}");
    assert!(t.contains("m=19456 KiB, t=2, p=1"), "la política impresa no es la real:\n{t}");
    assert!(t.contains("cuentas en el padrón"), "{t}");
    // Y que de verdad LEE: una base recién migrada no trae ninguna cuenta.
    assert!(t.contains("0 cuentas en el padrón"), "{t}");
}
