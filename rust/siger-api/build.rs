//! Rompe la compilación de *release* si la interfaz no está empotrada.
//!
//! Por qué existe: `rust-embed` lee de disco en *debug* y empotra en *release*.
//! El contenido de `estaticos/` está en `.gitignore` —solo se versiona el
//! `.gitkeep`—, así que en un clon limpio la carpeta está VACÍA y
//! `cargo build --release` producía un binario sin interfaz **sin avisar**:
//! compila, arranca, sirve la API, y devuelve un 404 a quien abra la raíz. El
//! fallo solo se ve en el navegador de quien recibe el artefacto.
//!
//! `construir.sh` hace lo correcto; esto es lo que atrapa a quien no lo use. Y
//! está en el `build.rs` y no en `DESPLIEGUE.md` porque lo que puede ser código
//! no va en prosa (directiva 6): la documentación afirmaba, de hecho, que la
//! interfaz «se construye e incrusta como parte del build», y no era cierto.
//!
//! El camino permisivo existe pero hay que PEDIRLO: `SIGER_SIN_INTERFAZ=1`
//! compila a propósito un binario sin interfaz (perfilar, un artefacto de
//! solo-API). Lo que no se admite es obtenerlo por descuido.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=estaticos");
    println!("cargo:rerun-if-changed=estaticos/index.html");
    println!("cargo:rerun-if-env-changed=SIGER_SIN_INTERFAZ");

    let perfil = std::env::var("PROFILE").unwrap_or_default();
    if perfil != "release" {
        return;
    }

    if std::env::var_os("SIGER_SIN_INTERFAZ").is_some() {
        println!(
            "cargo:warning=SIGER_SIN_INTERFAZ: se compila a propósito un binario \
             sin interfaz Angular."
        );
        return;
    }

    // `index.html` y no la carpeta: la carpeta EXISTE siempre (lleva `.gitkeep`
    // versionado para que compile un clon limpio), así que comprobar su presencia
    // no distinguiría nada. El punto de entrada del SPA es lo que de verdad falta.
    if !Path::new("estaticos/index.html").exists() {
        panic!(
            "\n\n\
             La interfaz Angular NO está empotrada: falta \
             rust/siger-api/estaticos/index.html\n\n\
             Un binario de release sin interfaz arranca, sirve la API y responde \
             404 en la raíz.\n\
             Es un fallo que solo se ve en el navegador de quien recibe el \
             artefacto.\n\n\
             Construya el artefacto completo, que hace las dos cosas en el orden \
             correcto:\n\n    ./rust/construir.sh\n\n\
             Si de verdad quiere un binario sin interfaz, pídalo:\n\n    \
             SIGER_SIN_INTERFAZ=1 cargo build --release\n"
        );
    }
}
