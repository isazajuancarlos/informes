//! De dónde viene quien pide. Lo usa el freno del login como mitad de su clave.
//!
//! # Por qué esto es un módulo y no una línea
//!
//! El freno cuenta por pareja (correo, origen). Si el origen se pudiera elegir
//! desde fuera, el atacante escribiría uno distinto en cada petición, nunca
//! llegaría al umbral, y tendríamos un freno que existe, se prueba, sale en la
//! documentación y no frena nada. La forma de conseguir eso es precisamente la
//! más cómoda: leer `X-Forwarded-For` y creérselo.
//!
//! Así que hay dos fuentes y solo una es de fiar por sí sola:
//!
//! - **La dirección del socket** (`ConnectInfo`). El cliente no la elige: es de
//!   dónde llegó la conexión TCP. Es el defecto.
//! - **`X-Real-IP`**, solo si el operador declara `SIGER_TRAS_PROXY=1`. Detrás de
//!   un proxy, el socket es siempre el del proxy y todos los clientes se
//!   confundirían en uno; pero la cabecera solo vale lo que valga el proxy que la
//!   escribe. El sitio de nginx de este repositorio pone
//!   `proxy_set_header X-Real-IP $remote_addr`, que **pisa** lo que traiga el
//!   cliente. Con un proxy que no la reescriba, encender esto devuelve el agujero.
//!
//! Se toma `X-Real-IP` y no `X-Forwarded-For` a propósito: la segunda es una
//! lista a la que el cliente puede anteponer valores, y elegir bien el salto
//! correcto exige saber cuántos proxies hay delante — un número que nadie
//! mantiene actualizado. `X-Real-IP` es un valor, no una lista.
//!
//! # Ausente no es «desconocido»
//!
//! Si se declara el proxy y la cabecera no llega, el dato NO ESTÁ, y aquí no se
//! rellena con la dirección del socket: sería la del proxy, es decir la misma
//! para todo el mundo, y el freno pasaría a contar solo por cuenta — que es la
//! variante que se descartó al diseñarlo, porque deja que un desconocido frene al
//! dueño de la cuenta. Se devuelve `None` y quien llama falla ruidosamente.

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{request::Parts, HeaderMap},
};

use crate::rutas::Estado;

/// La cabecera que escribe el proxy inverso del repositorio.
pub const CABECERA: &str = "x-real-ip";

/// El origen del cliente, o `None` si no se puede saber.
///
/// Devuelve solo la **dirección**, nunca el puerto: el puerto cambia en cada
/// conexión, así que incluirlo daría una pareja distinta por intento y el freno
/// no llegaría jamás al umbral.
pub fn del_cliente(
    cabeceras: &HeaderMap,
    socket: Option<SocketAddr>,
    tras_proxy: bool,
) -> Option<String> {
    if tras_proxy {
        return cabeceras
            .get(CABECERA)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
    }
    socket.map(|s| s.ip().to_string())
}

/// El origen ya resuelto, para el manejador. `None` = no se pudo saber, y quien
/// lo reciba tiene que fallar, no seguir.
///
/// Es un extractor y no un parámetro del manejador porque `ConnectInfo` no admite
/// extracción opcional en axum 0.8: pedirlo directo haría que una petición sin
/// información de conexión —cualquier prueba que llame al `Router` sin
/// `into_make_service_with_connect_info`— se rechazara antes de entrar al
/// manejador, con un 500 genérico y sin pasar por el freno.
pub struct OrigenCliente(pub Option<String>);

impl FromRequestParts<Estado> for OrigenCliente {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        estado: &Estado,
    ) -> Result<Self, Self::Rejection> {
        let socket = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0);
        Ok(OrigenCliente(del_cliente(&parts.headers, socket, estado.cfg.tras_proxy)))
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn cabeceras(pares: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pares {
            h.insert(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                v.parse().unwrap(),
            );
        }
        h
    }

    fn socket(s: &str) -> Option<SocketAddr> {
        Some(s.parse().unwrap())
    }

    #[test]
    fn sin_proxy_manda_el_socket_y_sin_puerto() {
        // El puerto fuera: con él, cada conexión sería una pareja nueva y el
        // freno no llegaría al umbral ni siendo aporreado.
        let o = del_cliente(&cabeceras(&[]), socket("203.0.113.9:51514"), false);
        assert_eq!(o.as_deref(), Some("203.0.113.9"));
    }

    #[test]
    fn sin_proxy_la_cabecera_del_cliente_no_pinta_nada() {
        // LA prueba de este módulo. Sin ella, un `del_cliente` que mirara la
        // cabecera primero pasaría todas las demás — y el freno sería decorativo.
        let h = cabeceras(&[("x-real-ip", "1.1.1.1"), ("x-forwarded-for", "2.2.2.2")]);
        let o = del_cliente(&h, socket("203.0.113.9:51514"), false);
        assert_eq!(
            o.as_deref(),
            Some("203.0.113.9"),
            "una cabecera del cliente se coló como origen sin proxy declarado"
        );
    }

    #[test]
    fn con_proxy_manda_la_cabecera() {
        let h = cabeceras(&[("x-real-ip", "198.51.100.7")]);
        let o = del_cliente(&h, socket("127.0.0.1:9000"), true);
        assert_eq!(o.as_deref(), Some("198.51.100.7"));
    }

    #[test]
    fn con_proxy_y_sin_cabecera_el_dato_no_esta() {
        // Y NO se rellena con la dirección del socket, que sería la del proxy: la
        // misma para todos, y el freno degradado a contar solo por cuenta.
        assert_eq!(del_cliente(&cabeceras(&[]), socket("127.0.0.1:9000"), true), None);
        // Vacía es lo mismo que ausente.
        let h = cabeceras(&[("x-real-ip", "   ")]);
        assert_eq!(del_cliente(&h, socket("127.0.0.1:9000"), true), None);
    }

    #[test]
    fn sin_proxy_y_sin_socket_tampoco_se_inventa_nada() {
        assert_eq!(del_cliente(&cabeceras(&[]), None, false), None);
    }
}
