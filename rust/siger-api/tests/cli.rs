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

/// **La siembra de SECOP clasifica vivos / históricos / sin-calendario, y lee
/// bien un CSV con comas dentro de comillas.**
///
/// Ejerce el binario de verdad contra un CSV escrito a mano (directiva 28): el
/// `objeto` del primer contrato lleva una coma DENTRO de comillas, que es el caso
/// para el que existe el lector propio —si se partiera, ese contrato tendría las
/// columnas corridas y las fechas caerían en la casilla equivocada—. Las tres
/// clases van en el mismo archivo con su control: un contrato vivo (fin futuro y
/// en ejecución) genera calendario, uno cerrado no, y uno sin fecha de inicio
/// entra igual pero sin calendario (los 336 se importan; no se inventa la fecha).
#[test]
fn importar_secop_siembra_y_clasifica_los_tres_casos() {
    let tmp = tempfile::tempdir().unwrap();
    let hoy = chrono::Utc::now().date_naive();
    let inicio = (hoy - chrono::Duration::days(800)).to_string();
    let fin_futuro = (hoy + chrono::Duration::days(400)).to_string();
    let fin_pasado = (hoy - chrono::Duration::days(100)).to_string();

    let csv = format!(
        "nombre_entidad,objeto_del_contrato,fecha_de_inicio_del_contrato,\
         fecha_de_fin_del_contrato,estado_contrato,referencia_del_contrato,\
         proveedor_adjudicado,nombre_supervisor,id_contrato\n\
         Municipio X,\"asesoría jurídica, y apoyo\",{inicio},{fin_futuro},En ejecución,C-1,Prov A,Sup A,ID1\n\
         Municipio Y,informe de gestión,{inicio},{fin_pasado},Cerrado,C-2,Prov B,No definido,ID2\n\
         Municipio Z,contrato en borrador,,{fin_futuro},Borrador,C-3,Prov C,Sup C,ID3\n"
    );
    let ruta_csv = tmp.path().join("secop.csv");
    std::fs::write(&ruta_csv, csv).unwrap();

    let o = correr(
        &["importar-secop", ruta_csv.to_str().unwrap(), "demo.db"],
        tmp.path(),
        &[],
    );
    let t = texto(&o);
    assert_eq!(codigo(&o), 0, "la siembra no salió bien:\n{t}");
    assert!(t.contains("3 contratos"), "no importó los tres:\n{t}");
    assert!(t.contains("1 vivos"), "no clasificó un vivo:\n{t}");
    assert!(t.contains("2 históricos"), "no clasificó dos históricos:\n{t}");
    assert!(t.contains("1 sin calendario"), "no contó el borrador sin fecha:\n{t}");
    // El vivo, de dos años a un año vista, tiene deadlines por delante: si el
    // lector hubiera partido el `objeto` con comillas, sus fechas se habrían
    // corrido y no habría entregas — así que este número también vigila el CSV.
    assert!(
        !t.contains("0 entregas futuras"),
        "el contrato vivo no sembró ninguna entrega futura:\n{t}"
    );

    // La pareja NEGATIVA: re-sembrar sobre la misma base choca en el nombre único
    // (no duplica en silencio). Falla y no deja la base a medias.
    let otra = correr(
        &["importar-secop", ruta_csv.to_str().unwrap(), "demo.db"],
        tmp.path(),
        &[],
    );
    assert_ne!(codigo(&otra), 0, "re-sembrar sobre datos existentes no debería salir 0");
}

/// Un contrato con la fecha de inicio DESPUÉS de la de fin entra igual (los 336
/// se importan), pero SIN calendario — no se inventa un orden para las fechas
/// (directiva 20). Es el ramo `inicio > fin` de `con_calendario`, que la pareja
/// de «tres casos» no ejerce (allí el sin-calendario viene de un inicio ausente).
#[test]
fn importar_secop_con_inicio_tras_fin_entra_sin_calendario() {
    let tmp = tempfile::tempdir().unwrap();
    let hoy = chrono::Utc::now().date_naive();
    let inicio = (hoy + chrono::Duration::days(100)).to_string(); // inicio en el futuro…
    let fin = (hoy - chrono::Duration::days(100)).to_string(); // …y fin ANTES del inicio
    let csv = format!(
        "nombre_entidad,objeto_del_contrato,fecha_de_inicio_del_contrato,\
         fecha_de_fin_del_contrato,estado_contrato,referencia_del_contrato\n\
         Municipio W,objeto,{inicio},{fin},En ejecución,C-9\n"
    );
    let ruta = tmp.path().join("invertido.csv");
    std::fs::write(&ruta, csv).unwrap();

    let o = correr(&["importar-secop", ruta.to_str().unwrap(), "demo.db"], tmp.path(), &[]);
    let t = texto(&o);
    assert_eq!(codigo(&o), 0, "{t}");
    assert!(t.contains("1 contratos"), "{t}");
    assert!(t.contains("1 sin calendario"), "inicio>fin debería quedar sin calendario:\n{t}");
}

/// **`--solo` siembra un solo prospecto**, que es lo que hace la demo «aquí está
/// SU contrato» sin volcar los 336. Y su pareja: un filtro que no casa nada NO
/// siembra una base vacía en silencio — falla (directiva 20).
#[test]
fn importar_secop_solo_filtra_a_un_prospecto() {
    let tmp = tempfile::tempdir().unwrap();
    let csv = "nombre_entidad,objeto_del_contrato,fecha_de_inicio_del_contrato,\
               fecha_de_fin_del_contrato,estado_contrato,referencia_del_contrato,id_contrato\n\
               Municipio X,uno,2024-01-01,2024-12-31,Cerrado,C-1,ID1\n\
               Municipio Y,dos,2024-01-01,2024-12-31,Cerrado,C-2,ID2\n\
               Municipio Z,tres,2024-01-01,2024-12-31,Cerrado,C-3,ID3\n";
    let ruta = tmp.path().join("secop.csv");
    std::fs::write(&ruta, csv).unwrap();

    // Casa una entidad: un solo contrato sembrado.
    let uno = correr(
        &["importar-secop", ruta.to_str().unwrap(), "una.db", "--solo", "Municipio Y"],
        tmp.path(),
        &[],
    );
    let t = texto(&uno);
    assert_eq!(codigo(&uno), 0, "{t}");
    assert!(t.contains("filtrado por «Municipio Y»"), "{t}");
    assert!(t.contains("1 contratos"), "el filtro no dejó exactamente uno:\n{t}");

    // Casa por id de contrato, también.
    let por_id = correr(
        &["importar-secop", ruta.to_str().unwrap(), "porid.db", "--solo", "ID3"],
        tmp.path(),
        &[],
    );
    assert!(texto(&por_id).contains("1 contratos"), "no casó por id:\n{}", texto(&por_id));

    // PAREJA ROJA: un filtro que no casa nada falla, no siembra base vacía.
    let vacio = correr(
        &["importar-secop", ruta.to_str().unwrap(), "vacia.db", "--solo", "Municipio Inexistente"],
        tmp.path(),
        &[],
    );
    assert_ne!(codigo(&vacio), 0, "un filtro sin coincidencias no debería sembrar");
    assert!(texto(&vacio).contains("no se sembró nada"), "{}", texto(&vacio));
}

/// Un CSV al que le falta una columna OBLIGATORIA no siembra a ciegas: falla
/// nombrando la columna. El control es la prueba de arriba, cuyo CSV sí las trae
/// todas y sale con 0.
#[test]
fn importar_secop_sin_una_columna_obligatoria_falla() {
    let tmp = tempfile::tempdir().unwrap();
    // Sin `fecha_de_fin_del_contrato`.
    let csv = "nombre_entidad,objeto_del_contrato,fecha_de_inicio_del_contrato,\
               estado_contrato,referencia_del_contrato\n\
               Municipio X,objeto,2024-01-01,En ejecución,C-1\n";
    let ruta = tmp.path().join("malo.csv");
    std::fs::write(&ruta, csv).unwrap();

    let o = correr(&["importar-secop", ruta.to_str().unwrap(), "demo.db"], tmp.path(), &[]);
    assert_ne!(codigo(&o), 0, "sembró sin una columna obligatoria");
    assert!(
        texto(&o).contains("fecha_de_fin_del_contrato"),
        "no dijo qué columna falta:\n{}",
        texto(&o)
    );
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

/// **El aviso que no salió tiene que verse desde fuera del proceso.**
///
/// `siger-api alertas` es lo que corre un temporizador, y hasta hoy devolvía
/// éxito SIEMPRE: con el SMTP mal configurado imprimía «120 fallidas» y salía
/// con 0, así que `systemctl` quedaba en verde para siempre mientras nadie
/// recibía el aviso que es el producto que se vende. Es la misma laguna que la
/// cabecera de este archivo describe para `respaldar`, en el otro subcomando.
///
/// La pareja va dentro, y sin ella el rojo no significaría nada: **el mismo
/// binario, la misma base y el mismo día** salen con 0 cuando el buzón está
/// escuchando. Lo único que cambia entre las dos mitades es si hay alguien al
/// otro lado del puerto.
#[test]
fn una_pasada_de_alertas_que_no_avisa_sale_con_fallo_y_una_que_avisa_no() {
    let dir = tempfile::tempdir().unwrap();
    let bd = dir.path().join("siger.db");
    let url = format!("sqlite://{}?mode=rwc", bd.display());

    // Un puerto sin nadie detrás: los envíos serán rechazados.
    let muerto = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let p = l.local_addr().unwrap().port();
        drop(l);
        p
    };
    // Y un sink que acepta todo, para el control.
    let escucha = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let vivo = escucha.local_addr().unwrap().port();
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        for s in escucha.incoming().flatten() {
            let mut w = s.try_clone().unwrap();
            let mut r = BufReader::new(s);
            let _ = w.write_all(b"220 sink\r\n");
            let _ = w.flush();
            let mut l = String::new();
            while r.read_line(&mut l).unwrap_or(0) > 0 {
                let up = l.trim_end().to_uppercase();
                let _ = if up.starts_with("EHLO") || up.starts_with("HELO") {
                    w.write_all(b"250-sink\r\n250 OK\r\n")
                } else if up.starts_with("DATA") {
                    let _ = w.write_all(b"354 fin\r\n");
                    let _ = w.flush();
                    let mut c = String::new();
                    loop {
                        c.clear();
                        if r.read_line(&mut c).unwrap_or(0) == 0 || c.trim_end() == "." {
                            break;
                        }
                    }
                    w.write_all(b"250 OK\r\n")
                } else if up.starts_with("QUIT") {
                    let _ = w.write_all(b"221 adios\r\n");
                    break;
                } else {
                    w.write_all(b"250 OK\r\n")
                };
                let _ = w.flush();
                l.clear();
            }
        }
    });

    let entorno = |puerto: u16| -> Vec<(&'static str, String)> {
        vec![
            ("SIGER_SECRETO_SESION", SECRETO.to_string()),
            ("DATABASE_URL", url.clone()),
            ("SIGER_DIR_ENTREGAS", "storage/entregas".to_string()),
            ("ALERTAS_ACTIVAS", "true".to_string()),
            ("SMTP_HOST", "127.0.0.1".to_string()),
            ("SMTP_TLS", "ninguno".to_string()),
            ("SMTP_PUERTO", puerto.to_string()),
            ("SMTP_DESDE", "SIGER <siger@local>".to_string()),
        ]
    };
    let correr_con = |puerto: u16| {
        let e = entorno(puerto);
        let refs: Vec<(&str, &str)> = e.iter().map(|(k, v)| (*k, v.as_str())).collect();
        correr(&["alertas"], dir.path(), &refs)
    };

    // Una pasada en seco crea la base y aplica las migraciones. Con ella vacía
    // no hay nada que avisar: sale con 0, y eso ya dice que el 1 de después no
    // viene de que el binario no arranque.
    let vacia = correr_con(vivo);
    assert_eq!(codigo(&vacia), 0, "sin nada que avisar sale bien: {vacia:?}");

    // Ahora una entrega que vence hoy, con un operador con correo.
    let hoy = chrono::Utc::now().date_naive().to_string();
    let sql = format!(
        "INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) VALUES (1,'Op','op@x.co','h','OPERADOR',1); \
         INSERT INTO informes (id,nombre,recurrencia) VALUES (1,'Inf','{{\"modo\":\"INMEDIATO\"}}'); \
         INSERT INTO entregas (informe_id,usuario_id,fecha_nominal,fecha_limite,estado) \
         VALUES (1,1,'n1','{hoy}','PENDIENTE');"
    );
    let sqlite = Command::new("sqlite3").arg(&bd).arg(&sql).status();
    if !matches!(sqlite, Ok(s) if s.success()) {
        eprintln!("sin sqlite3 en el PATH: no se pudo sembrar; la prueba no midió nada");
        return;
    }

    // EL CASO ROJO: el correo no responde.
    let caido = correr_con(muerto);
    assert_ne!(codigo(&caido), 0, "una pasada que no pudo avisar NO puede salir con 0: {caido:?}");
    let err = String::from_utf8_lossy(&caido.stderr);
    assert!(err.contains("INCOMPLETA"), "y tiene que decir por qué: {err}");

    // LA PAREJA: mismo binario, misma base, mismo día — y con el buzón vivo, 0.
    let bueno = correr_con(vivo);
    assert_eq!(codigo(&bueno), 0, "con el correo en pie sale bien: {bueno:?}");
    let out = String::from_utf8_lossy(&bueno.stdout);
    assert!(out.contains("1 de proximidad"), "y avisó de verdad: {out}");
}

/// **Una ventana de proximidad absurda tiene que impedir el ARRANQUE, no matar
/// el reloj a medianoche.**
///
/// `alertas::correr` calcula `hoy + días`, y `NaiveDate + Duration` entra en
/// pánico al desbordar. Como el reloj vive en un `tokio::spawn` sin supervisor,
/// ese pánico se llevaba la generación y las alertas para siempre **sin tumbar
/// el servidor**: se veía igual que un sistema sano y quieto.
///
/// Se cierra en la configuración y no en la aritmética porque ahí es donde está
/// la causa: se rechaza lo imposible —el planificador solo crea entregas 730
/// días por delante, así que una ventana de diez años ya no puede señalar a
/// nada— y se rechaza RUIDOSAMENTE, en el arranque.
///
/// Tres casos, porque uno solo no diría nada: el absurdo falla, el negativo
/// falla, y **un valor grande pero legítimo pasa** — sin esa tercera mitad, una
/// regla que rechazara todo saldría igual de verde.
#[test]
fn una_ventana_de_proximidad_imposible_impide_arrancar_y_una_grande_no() {
    let tmp = tempfile::tempdir().unwrap();
    instalacion(tmp.path());

    let con = |dias: &str| -> Output {
        let mut e = entorno_valido();
        e.push(("ALERTAS_ACTIVAS", "true"));
        e.push(("SMTP_HOST", "127.0.0.1"));
        e.push(("SMTP_TLS", "ninguno"));
        e.push(("SMTP_PUERTO", "2525"));
        e.push(("SMTP_DESDE", "SIGER <siger@local>"));
        e.push(("ALERTAS_DIAS_PROXIMIDAD", dias));
        correr(&["alertas"], tmp.path(), &e)
    };

    // Absurdo: por encima del techo. Falla, y dice cuál es el rango.
    let malo = con("100000000");
    assert_ne!(codigo(&malo), 0, "un valor imposible no puede arrancar: {}", texto(&malo));
    assert!(
        texto(&malo).contains("ALERTAS_DIAS_PROXIMIDAD"),
        "y tiene que decir cuál es la variable: {}",
        texto(&malo)
    );

    // Negativo: avisar «tres días después» no es proximidad, es incumplimiento.
    assert_ne!(codigo(&con("-1")), 0, "un negativo tampoco");

    // LA PAREJA que impide que esto sea una regla que dice que no a todo: un año
    // entero de anticipación es inusual y perfectamente legítimo.
    let bueno = con("365");
    assert_eq!(codigo(&bueno), 0, "365 días es raro pero válido: {}", texto(&bueno));
}
