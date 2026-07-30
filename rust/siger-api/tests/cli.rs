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
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("no arrancó");

    // Se espera a que la migración haya dejado la base, y NO un tiempo fijo: un
    // `sleep` generoso hace la suite lenta y uno corto la hace intermitente.
    let bd = dir.join("siger.db");
    let limite = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < limite {
        if bd.exists() && std::fs::metadata(&bd).map(|m| m.len() > 0).unwrap_or(false) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    hijo.kill().ok();
    hijo.wait().ok();
    assert!(bd.exists(), "el binario no creó la base en 30 s");
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
