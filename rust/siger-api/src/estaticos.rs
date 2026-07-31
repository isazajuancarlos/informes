//! La interfaz viaja **dentro** del binario.
//!
//! Es lo que convierte el despliegue en copiar un archivo: sin Python, sin
//! `pip`, sin Node y sin servidor web delante. Y tiene una consecuencia de
//! seguridad que no es accesoria: al servirse el SPA desde el mismo origen que
//! la API, la sesión puede ser una cookie `HttpOnly` + `SameSite=Strict`. No hay
//! token en `localStorage` que un XSS pueda exfiltrar, ni CORS que configurar,
//! ni CSRF que resolver.
//!
//! Como además `onprem` y `standalone` son perfiles **sin internet**, aquí no
//! cabe ninguna fuente, icono ni script traído de un CDN: lo que no esté dentro
//! del binario, no existe.

use axum::{
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;

/// El resultado de `ng build`, copiado a `estaticos/` por el guion de
/// empaquetado. La carpeta lleva un `.gitkeep` para que el binario compile en
/// una copia limpia aunque nadie haya construido la interfaz todavía.
#[derive(Embed)]
#[folder = "estaticos/"]
struct Interfaz;

pub async fn servir(uri: Uri) -> Response {
    let ruta = uri.path().trim_start_matches('/');
    let ruta = if ruta.is_empty() { "index.html" } else { ruta };

    if let Some(archivo) = Interfaz::get(ruta) {
        let tipo = archivo.metadata.mimetype();
        return ([(header::CONTENT_TYPE, tipo)], archivo.data).into_response();
    }

    // Rutas del enrutador del SPA (/entregas, /calendario…): las resuelve el
    // navegador, así que se le devuelve el index. Pero solo a una navegación —
    // a `/api/...` desconocido se le responde 404, para que un error de la
    // interfaz no llegue disfrazado de página.
    if ruta.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "no existe").into_response();
    }

    match Interfaz::get("index.html") {
        Some(index) => ([(header::CONTENT_TYPE, "text/html")], index.data).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "La interfaz no está empaquetada en este binario. \
             Construya con `npm run build` en angular/ y copie el resultado a \
             rust/siger-api/estaticos/.",
        )
            .into_response(),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// El `index.html` que se empotró **puede ejecutarse bajo `script-src 'self'`**.
    ///
    /// Esta prueba existe porque el fallo que cierra no se ve: con
    /// `inlineCritical` encendido, Angular emite
    /// `<link ... media="print" onload="this.media='all'">`. Ese `onload` es
    /// script en línea; con la CSP puesta el navegador NO lo ejecuta, la hoja se
    /// queda en «print» y la aplicación se ve sin estilos **sin un solo error en
    /// consola ni en el servidor**. Se apagó en `angular.json`, pero eso es una
    /// opción de configuración que cualquier actualización de Angular o un
    /// `ng update` puede revertir sin que nadie lo note.
    ///
    /// Se salta si no hay interfaz empaquetada (copia limpia sin `ng build`): lo
    /// que no está no se puede comprobar, y fallar ahí sería castigar a quien solo
    /// compiló el servidor.
    #[test]
    fn el_index_empotrado_no_lleva_nada_en_linea_que_ejecutar() {
        let Some(index) = Interfaz::get("index.html") else {
            eprintln!("sin interfaz empaquetada: nada que comprobar");
            return;
        };
        let html = String::from_utf8_lossy(&index.data);

        // Manejadores en línea (`onload=`, `onclick=`…). Se busca con el signo
        // delante para no cazar `content=`, que contiene «on».
        for pista in [" onload=", " onclick=", " onerror=", " onchange=", " onsubmit="] {
            assert!(
                !html.contains(pista),
                "el index empotrado trae «{}», que `script-src 'self'` NO ejecuta. \
                 Casi seguro volvió `inlineCritical` en angular.json.",
                pista.trim()
            );
        }

        // Un `<script>` sin `src` es código en línea.
        for trozo in html.split("<script").skip(1) {
            let etiqueta = trozo.split('>').next().unwrap_or("");
            assert!(
                etiqueta.contains("src="),
                "hay un <script> en línea en el index: `script-src 'self'` no lo ejecuta"
            );
        }

        // Y nada traído de fuera: los perfiles `onprem` y `standalone` no tienen
        // internet, así que un host externo no es solo una fuga de privacidad —
        // es una tipografía que no carga en casa del cliente.
        assert!(
            !html.contains("http://") && !html.contains("https://"),
            "el index empotrado apunta a un host externo; `default-src 'self'` lo bloquea \
             y en un despliegue sin internet no cargaría igualmente"
        );
    }
}
