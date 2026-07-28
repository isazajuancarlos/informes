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
