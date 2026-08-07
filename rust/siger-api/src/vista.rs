//! El panel de SIGER, tejido EN EL SERVIDOR con `maud` (directiva 26).
//!
//! Sustituye al SPA de Angular 22 + Material que vive en `angular/`. El HTML lo
//! compila `rustc` con los tipos del dominio dentro, así que una plantilla rota
//! es un error de compilación y no una pantalla en blanco a las tres semanas.
//!
//! `/api` (JSON) NO cambia: sigue siendo la superficie de integración, y este
//! panel es un segundo consumidor, server-rendered.
//!
//! **Nada de este módulo decide sobre seguridad.** El acceso y el segundo paso
//! los resuelven `rutas::intentar_acceso` y `segundofactor::intentar_segundo_paso`,
//! que son las MISMAS funciones que usa la API: aquí solo se les da forma de
//! HTML. Copiar ese razonamiento —el freno, el señuelo, el antirreplay del
//! contador, cuándo un fallo se le cobra al usuario— habría creado una segunda
//! copia que se queda atrás en el primer arreglo (directiva 1).

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use maud::{html, Markup, DOCTYPE};
use serde::Deserialize;

use crate::rutas::Estado;

// ── Lo vendorizado ────────────────────────────────────────────────────────────
// La MISMA versión de htmx y de `boi` que nexus, chasqui e inventario
// (directiva 26). Se comprueba con `md5sum`, no de memoria. Van LOCALES: los
// perfiles `onprem` y `standalone` son sin internet, y lo que no esté dentro del
// binario no existe.
const HTMX_JS: &str = include_str!("../estaticos/htmx.min.js");
const BOI_JS: &str = include_str!("../estaticos/boi.js");
const COMPORTAMIENTOS_JS: &str = include_str!("../estaticos/comportamientos.js");
const PANEL_CSS: &str = include_str!("../estaticos/panel.css");

fn recurso(cuerpo: &'static str, tipo: &'static str) -> Response {
    ([(header::CONTENT_TYPE, tipo)], cuerpo).into_response()
}

pub async fn htmx_js() -> Response {
    recurso(HTMX_JS, "application/javascript; charset=utf-8")
}
pub async fn boi_js() -> Response {
    recurso(BOI_JS, "application/javascript; charset=utf-8")
}
pub async fn comportamientos_js() -> Response {
    recurso(COMPORTAMIENTOS_JS, "application/javascript; charset=utf-8")
}
pub async fn panel_css() -> Response {
    recurso(PANEL_CSS, "text/css; charset=utf-8")
}

// ── Quién pide, en el panel ───────────────────────────────────────────────────

/// El mismo `Actor` que usa `/api`, pero **rechazando como se le rechaza a una
/// persona**: con una redirección a la entrada, no con un 401 en JSON.
///
/// Existe porque el panel lo estrenó mal y lo cazó el banco de navegador en su
/// primera corrida: quien tecleaba `/entregas` sin sesión recibía
/// `{"error":"sesión no válida"}` en crudo. Correcto para una máquina, inútil
/// para quien está delante — y encima da la impresión de que el sistema se rompió
/// en vez de que hay que entrar.
///
/// La AUTORIZACIÓN no cambia ni un ápice: por dentro es el extractor de siempre,
/// con su relectura del rol y de `token_version` en cada petición. Lo único que
/// cambia es la forma del rechazo.
pub struct ActorDePanel(pub crate::autz::Actor);

impl axum::extract::FromRequestParts<Estado> for ActorDePanel {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        estado: &Estado,
    ) -> Result<Self, Self::Rejection> {
        match crate::autz::Actor::from_request_parts(parts, estado).await {
            Ok(a) => Ok(ActorDePanel(a)),
            // 303 y no 302: tras un POST, el navegador tiene que pedir el destino
            // con GET o recargar reenviaría el formulario.
            Err(_) => {
                let mut cab = HeaderMap::new();
                cab.insert(header::LOCATION, "/entrar".parse().expect("ruta válida"));
                Err((StatusCode::SEE_OTHER, cab).into_response())
            }
        }
    }
}

// ── La envoltura ──────────────────────────────────────────────────────────────

/// Las secciones del panel. `activa` marca dónde estamos: un menú que no lo dice
/// obliga a mirar la URL.
pub const SECCIONES: [(&str, &str); 6] = [
    ("/entregas", "Entregas"),
    ("/cronograma", "Cronograma"),
    ("/partes", "Partes"),
    ("/informes", "Informes"),
    ("/estadisticas", "Estadísticas"),
    ("/cuentas", "Cuentas"),
];

/// El caparazón. `activa = None` es una pantalla SIN sesión —acceso y segundo
/// paso—: ahí no se dibuja la barra, porque enumerar las secciones le dice a
/// quien no ha entrado qué hay dentro.
pub fn envoltura(titulo: &str, activa: Option<&str>, cuerpo: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="es" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (titulo) " · SIGER" }
                link rel="stylesheet" href="/panel.css";
                // htmx como script clásico, boi como módulo. Los dos LOCALES: un
                // `<script src>` a un CDN es justo el vector que este stack
                // existe para cerrar, y además no cargaría sin internet.
                script src="/htmx.min.js" {}
                script type="module" src="/boi.js" {}
                script type="module" src="/comportamientos.js" {}
            }
            body {
                @if let Some(activa) = activa {
                    nav.barra {
                        span.marca { "SIGER" }
                        span.espacio {}
                        @for (ruta, nombre) in SECCIONES {
                            a href=(ruta) class=(if ruta == activa { "activo" } else { "" }) { (nombre) }
                        }
                        form.salir method="post" action="/salir" {
                            button type="submit" { "Salir" }
                        }
                    }
                }
                main.contenido { (cuerpo) }
            }
        }
    }
}

// ── Acceso ────────────────────────────────────────────────────────────────────

/// La pantalla de entrada. Es pública a propósito y **no dice nada del sistema
/// más allá de su nombre**: ni versión, ni cliente, ni secciones.
fn pantalla_acceso(error: Option<&str>, demo: &[crate::config::CuentaDemo]) -> Markup {
    envoltura("Entrar", None, html! {
        div.centro {
            div.caja {
                h1 { "SIGER" }
                p.lema { "Sistema de gestión de reportes" }
                @if let Some(e) = error {
                    div.error role="alert" { (e) }
                }
                // El cartel SOLO existe si `SIGER_DEMO_CUENTAS` trae algo. En una
                // instalación de cliente la lista está vacía y aquí no se pinta
                // nada: nadie ve un usuario y una clave que no son suyos. No hay
                // un interruptor que lo encienda —lo que se publica hay que
                // escribirlo entero en el entorno—, y por eso no puede aparecer
                // por accidente.
                @if !demo.is_empty() {
                    div.demo {
                        b { "Demostración pública" }
                        @for c in demo {
                            p {
                                (c.nombre) ": " code { (c.correo) } " / " code { (c.clave) }
                            }
                        }
                    }
                }
                form method="post" action="/entrar" {
                    label for="email" { "Correo" }
                    input #email type="email" name="email" autocomplete="username"
                          required maxlength="320";
                    label for="clave" { "Contraseña" }
                    input #clave type="password" name="clave" autocomplete="current-password" required;
                    button.primario type="submit" { "Entrar" }
                }
            }
        }
    })
}

/// La pantalla del segundo paso. Se llega con la cookie PENDIENTE puesta, que no
/// es una sesión y dura cinco minutos.
fn pantalla_codigo(error: Option<&str>) -> Markup {
    envoltura("Código de verificación", None, html! {
        div.centro {
            div.caja {
                h1 { "Código de verificación" }
                p.lema {
                    "Escriba el código de su aplicación, o uno de sus códigos de recuperación."
                }
                @if let Some(e) = error {
                    div.error role="alert" { (e) }
                }
                form method="post" action="/entrar/codigo" {
                    label for="codigo" { "Código" }
                    // `one-time-code` deja que el teléfono lo ofrezca solo, y
                    // `inputmode` saca el teclado numérico. Sin `pattern`: un
                    // código de recuperación no son seis dígitos, y un patrón
                    // que lo rechace dejaría fuera justo a quien ya está en
                    // apuros.
                    input #codigo name="codigo" autocomplete="one-time-code"
                          inputmode="numeric" required autofocus;
                    button.primario type="submit" { "Verificar" }
                }
                form.cancelar method="post" action="/salir" {
                    button.secundario type="submit" { "Cancelar" }
                }
            }
        }
    })
}

pub async fn acceso_pagina(State(e): State<Estado>) -> Markup {
    pantalla_acceso(None, &e.cfg.demo)
}

#[derive(Deserialize)]
pub struct Credenciales {
    email: String,
    clave: String,
}

/// `POST /entrar` — el MISMO acceso que `/api/sesion`, en HTML.
///
/// No decide nada: llama a `rutas::intentar_acceso` y le da forma. Los tres
/// finales del dominio se traducen a tres pantallas.
pub async fn acceso_enviar(
    State(e): State<Estado>,
    crate::origen::OrigenCliente(origen): crate::origen::OrigenCliente,
    axum::Form(f): axum::Form<Credenciales>,
) -> Response {
    let cred = crate::rutas::Credenciales::nuevas(f.email, f.clave);
    match crate::rutas::intentar_acceso(&e, origen, &cred).await {
        Ok(crate::rutas::Acceso::Abierta(cabeceras)) => a_donde(cabeceras, "/entregas"),
        Ok(crate::rutas::Acceso::SegundoFactor(cabeceras)) => a_donde(cabeceras, "/entrar/codigo"),
        // 429 y no 401: decir «credenciales incorrectas» cuando lo que pasa es
        // que hay que esperar manda al usuario legítimo a cambiar una contraseña
        // que estaba bien. El número de segundos SÍ se enseña, que es lo que le
        // deja saber si esperar o irse a hacer otra cosa.
        Ok(crate::rutas::Acceso::Frenado { segundos, cabeceras }) => (
            StatusCode::TOO_MANY_REQUESTS,
            cabeceras,
            pantalla_acceso(
                Some(&format!("Demasiados intentos. Vuelva a probar en {segundos} s.")),
                &e.cfg.demo,
            ),
        )
            .into_response(),
        // El cartel se conserva también en el fallo: si desapareciera al primer
        // error de tecleo, quien entra a la demostración se quedaría sin las
        // credenciales que acaba de necesitar.
        Err(crate::rutas::Error(codigo, mensaje)) => {
            (codigo, pantalla_acceso(Some(&mensaje), &e.cfg.demo)).into_response()
        }
    }
}

pub async fn codigo_pagina() -> Markup {
    pantalla_codigo(None)
}

#[derive(Deserialize)]
pub struct SoloCodigo {
    codigo: String,
}

/// `POST /entrar/codigo` — el MISMO segundo paso que `/api/2fa/verificar`.
pub async fn codigo_enviar(
    State(e): State<Estado>,
    cabeceras: HeaderMap,
    crate::origen::OrigenCliente(origen): crate::origen::OrigenCliente,
    axum::Form(f): axum::Form<SoloCodigo>,
) -> Response {
    match crate::segundofactor::intentar_segundo_paso(&e, &cabeceras, origen, &f.codigo).await {
        Ok(crate::segundofactor::SegundoPaso::Abierta(salida)) => a_donde(salida, "/entregas"),
        Ok(crate::segundofactor::SegundoPaso::Frenado { segundos, cabeceras }) => (
            StatusCode::TOO_MANY_REQUESTS,
            cabeceras,
            pantalla_codigo(Some(&format!(
                "Demasiados intentos. Vuelva a probar en {segundos} s."
            ))),
        )
            .into_response(),
        Err(err) => respuesta_de_error(err, pantalla_codigo),
    }
}

/// Redirección con las cabeceras que trae el dominio —la cookie, y el cierre del
/// pendiente cuando toca—. `303` y no `302`: tras un POST, el navegador tiene
/// que pedir el destino con GET, o recargar reenvía el formulario.
fn a_donde(mut cabeceras: HeaderMap, destino: &str) -> Response {
    cabeceras.insert(header::LOCATION, destino.parse().expect("ruta válida"));
    (StatusCode::SEE_OTHER, cabeceras).into_response()
}

/// Un error del dominio, vuelto pantalla.
///
/// **El texto que ya escribió el dominio se respeta tal cual**: «credenciales
/// incorrectas», «el servidor está ocupado», «use uno de sus códigos de
/// recuperación» están redactados a propósito para no delatar qué cuentas
/// existen y para decirle a quien está delante qué hacer. Reescribirlos aquí
/// sería deshacer ese trabajo — que es exactamente lo que pasó en el panel de
/// `inventario`, donde el texto crudo de SQL acabó en la pantalla.
///
/// Y el CÓDIGO también se conserva: 401, 503 y 500 dicen cosas distintas.
fn respuesta_de_error(err: crate::rutas::Error, pantalla: fn(Option<&str>) -> Markup) -> Response {
    let crate::rutas::Error(codigo, mensaje) = err;
    (codigo, pantalla(Some(&mensaje))).into_response()
}

/// `POST /salir` — cierra la sesión y el pendiente, y vuelve a la entrada.
pub async fn salir(State(e): State<Estado>) -> Response {
    let mut cabeceras = HeaderMap::new();
    cabeceras.append(
        header::SET_COOKIE,
        crate::sesion::cabecera_cerrar().parse().expect("ASCII"),
    );
    // También el pendiente: cancelar en mitad del segundo paso tiene que dejar
    // limpio, o el token sobrevive sus cinco minutos.
    cabeceras.append(
        header::SET_COOKIE,
        crate::sesion::cabecera_pendiente_cerrar().parse().expect("ASCII"),
    );
    let _ = &e;
    a_donde(cabeceras, "/entrar")
}

// ── Entregas ──────────────────────────────────────────────────────────────────

/// La pantalla central: qué hay que entregar, cuándo vence y cómo va.
///
/// **El alcance no se re-deriva aquí.** Se llama a `rutas::entregas_de`, que es
/// la MISMA función que sirve `GET /api/entregas` y mete el alcance dentro de la
/// consulta. Filtrar después de leer, o escribir aquí un segundo `WHERE`, es
/// como se acaba enseñando la entrega de otro cliente.
///
/// Y `actor: Actor` en la firma no es adorno: es un extractor, así que un
/// manejador que no lo pida **no tiene sesión** y uno que lo pida ya la exigió.
pub async fn entregas_pagina(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
) -> Response {
    let entregas = match crate::rutas::entregas_de(&e, &actor).await {
        Ok(v) => v,
        Err(err) => return error_en_pantalla("Entregas", "/entregas", err),
    };

    // Los contadores salen de la MISMA lista que se dibuja, no de otra consulta:
    // dos consultas distintas pueden discrepar y entonces el resumen miente
    // sobre la tabla que hay justo debajo.
    let vencidas = entregas.iter().filter(|x| x.estado == "vencida").count();
    let pendientes = entregas.iter().filter(|x| x.estado == "pendiente").count();

    envoltura("Entregas", Some("/entregas"), html! {
        h2 { "Entregas" }
        div.resumen {
            div { span { (entregas.len()) } " en total" }
            div { span { (pendientes) } " pendientes" }
            div { span { (vencidas) } " vencidas" }
        }
        @if entregas.is_empty() {
            // «No le corresponde ninguna» y «no hay ninguna» son cosas distintas
            // para quien mira: un tercero sin entidad vinculada tiene alcance
            // vacío por diseño, y decírselo evita que crea que el sistema falla.
            p.vacio {
                @if matches!(actor.alcance(), crate::autz::Alcance::Ninguna) {
                    "Su cuenta no tiene ninguna entidad vinculada, así que no le corresponde \
                     ninguna entrega. Pídale a un administrador que la vincule."
                } @else {
                    "No hay entregas que le correspondan."
                }
            }
        } @else {
            div.tabla-ancha {
                table.datos {
                    thead { tr {
                        th { "Informe" } th { "Nominal" } th { "Límite" }
                        th { "Ajuste" } th { "Estado" } th { "Archivo" } th { "Acciones" }
                    } }
                    tbody {
                        @for x in &entregas {
                            tr {
                                td { (x.informe) }
                                // La fecha NOMINAL da identidad y mide la
                                // vigencia; la LÍMITE mide el cumplimiento. Son
                                // dos campos porque significan dos cosas
                                // (directiva 32), y por eso se enseñan las dos.
                                td { (x.fecha_nominal) }
                                td { (x.fecha_limite) }
                                td { (x.tipo_ajuste) }
                                td { span.estado class=(clase_estado(&x.estado)) { (x.estado) } }
                                td {
                                    @match &x.archivo_nombre {
                                        None => span.vacio { "—" }
                                        Some(nombre) => {
                                            // El nombre y NUNCA la ruta: dar la
                                            // ruta fue exactamente el agujero
                                            // del portal anterior.
                                            a href={ "/api/entregas/" (x.id) "/archivo" } { (nombre) }
                                            @if x.tiene_firma { span.marca-firma title="Firmada" { " ✓" } }
                                            // Un archivo cuyo contenido no
                                            // concuerda con su extensión se
                                            // MARCA, no se esconde: quien revisa
                                            // tiene que enterarse.
                                            @if x.archivo_valido == Some(false) {
                                                span.alerta title="El contenido no concuerda con la extensión" {
                                                    " ⚠ revisar"
                                                }
                                            }
                                        }
                                    }
                                }
                                td { (acciones_de_entrega(x, &actor)) }
                            }
                        }
                    }
                }
            }
        }
    })
    .into_response()
}

/// Las acciones de una fila, **según lo que ese actor puede hacer de verdad**.
///
/// El botón no es la guardia —la guardia vive dentro de `justificar_de` y
/// `revisar_de`—, pero ofrecer una acción que va a dar 403 es una forma de
/// mentir: quien la pulsa cree que puede y descubre que no. Se enseña lo que se
/// puede, y lo que decide es el servidor de todas formas.
fn acciones_de_entrega(
    x: &crate::rutas::EntregaVista,
    actor: &crate::autz::Actor,
) -> Markup {
    let supervisa = matches!(
        actor.rol,
        crate::autz::Rol::Admin | crate::autz::Rol::Supervisor
    );
    html! {
        div.acciones {
            // Justificar: el responsable, o un admin en su nombre. Solo tiene
            // sentido cuando algo va mal — una entrega al día no se justifica.
            @if x.estado == "vencida" || x.estado == "pendiente" {
                a.enlace-accion href={ "/entregas/" (x.id) } { "Justificar" }
            }
            @if supervisa {
                a.enlace-accion href={ "/entregas/" (x.id) } { "Revisar" }
            }
            @if !supervisa && !(x.estado == "vencida" || x.estado == "pendiente") {
                span.vacio { "—" }
            }
        }
    }
}

fn clase_estado(estado: &str) -> &'static str {
    // Enumera lo que SÍ significa algo y manda el resto a «sin clase»
    // (directiva 23): un estado nuevo no debe heredar el color de otro.
    match estado {
        "vencida" => "mal",
        "entregada" | "aprobada" => "bien",
        "pendiente" => "espera",
        _ => "",
    }
}

/// Un error del dominio, vuelto pantalla CON la envoltura puesta, para que se
/// pueda navegar a otra parte. El texto y el código son los que ya escribió el
/// dominio; aquí no se reescriben.
fn error_en_pantalla(titulo: &str, activa: &str, err: crate::rutas::Error) -> Response {
    let crate::rutas::Error(codigo, mensaje) = err;
    (
        codigo,
        envoltura(titulo, Some(activa), html! {
            h2 { (titulo) }
            div.error role="alert" { (mensaje) }
        }),
    )
        .into_response()
}

// ── Estadísticas ──────────────────────────────────────────────────────────────

pub async fn estadisticas_pagina(State(e): State<Estado>, actor: crate::autz::Actor) -> Response {
    let st = match crate::estadisticas::resumen_de(&e, &actor).await {
        Ok(v) => v,
        Err(err) => return error_en_pantalla("Estadísticas", "/estadisticas", err),
    };

    envoltura("Estadísticas", Some("/estadisticas"), html! {
        h2 { "Estadísticas" }
        div.resumen {
            div { span { (st.total) } " entregas" }
            div { span { (st.pendientes) } " pendientes" }
            div { span { (st.cumplidas) } " cumplidas" }
            div { span { (st.extemporaneas) } " extemporáneas" }
            div { span { (st.incumplidas) } " incumplidas" }
        }
        h3 { "Lo que urge" }
        div.resumen {
            div { span class=(if st.vencidas > 0 { "alerta" } else { "" }) { (st.vencidas) } " vencidas" }
            div { span { (st.proximas_7d) } " vencen en 7 días" }
        }
        h3 { "Cumplimiento" }
        // `None` NO se dibuja como 100 %: se mide sobre lo que YA venció, y si
        // no ha vencido nada todavía no hay tasa que dar. Inventar un 100 %
        // sería el relleno que la directiva 20 prohíbe, y encima el más
        // halagador — nadie lo cuestionaría.
        @match st.tasa_cumplimiento {
            Some(t) => {
                p.tasa { span.numero { (t) "%" } " de lo que ya venció se cumplió a tiempo." }
            }
            None => {
                p.vacio { "Todavía no ha vencido ninguna entrega, así que no hay tasa que medir." }
            }
        }
    })
    .into_response()
}

// ── Notificaciones ────────────────────────────────────────────────────────────

pub async fn notificaciones_pagina(State(e): State<Estado>, actor: crate::autz::Actor) -> Response {
    let ns = match crate::flujo::notificaciones_de(&e, &actor).await {
        Ok(v) => v,
        Err(err) => return error_en_pantalla("Notificaciones", "", err),
    };
    let sin_leer = ns.iter().filter(|n| !n.leida).count();

    envoltura("Notificaciones", Some(""), html! {
        h2 { "Notificaciones" }
        p.nota { (sin_leer) " sin leer, de " (ns.len()) " (se muestran las 100 más recientes)." }
        @if ns.is_empty() {
            p.vacio { "No tiene notificaciones." }
        } @else {
            // La carga inicial y lo que devuelve htmx usan EL MISMO fragmento.
            // Tejerlo dos veces es como la fila de después acaba distinta de la
            // de antes, y el usuario ve saltar la pantalla.
            ul.notificaciones {
                @for n in &ns { (fila_de_notificacion(n)) }
            }
        }
    })
    .into_response()
}

/// Una fila. Se usa en la carga inicial y en la respuesta de htmx.
fn fila_de_notificacion(n: &crate::flujo::Notificacion) -> Markup {
    html! {
        li class=(if n.leida { "leida" } else { "" }) {
            div.cabeza { b { (n.titulo) } span.momento { (n.momento) } }
            p { (n.mensaje) }
            div.acciones {
                @if n.entrega_id.is_some() { a href="/entregas" { "Ver entregas" } }
                @if !n.leida {
                    // htmx cambia SOLO esta fila, sin recargar la página. Es
                    // exactamente lo que htmx cubre, y por eso aquí no hace
                    // falta JavaScript propio (directiva 26).
                    button.secundario
                           hx-post={ "/notificaciones/" (n.id) "/leida" }
                           hx-target="closest li"
                           hx-swap="outerHTML" { "Marcar leída" }
                }
            }
        }
    }
}

/// `POST /notificaciones/{id}/leida` — devuelve **solo la fila**, ya marcada.
///
/// La guardia es `flujo::marcar_leida_de`, la MISMA que usa la API: el
/// `AND usuario_id` vive DENTRO del `UPDATE`, porque comprobar la pertenencia
/// antes, en otra consulta, deja un hueco entre las dos.
///
/// Y la fila que se devuelve se RELEE de la base en vez de fabricarse a mano: si
/// el `UPDATE` no afectó a ninguna fila —la notificación era de otro— lo que
/// vuelve es la fila sin marcar, y se ve. Fabricarla marcada sería enseñar un
/// resultado que no ocurrió.
pub async fn notificacion_leida(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Response {
    if let Err(crate::rutas::Error(codigo, mensaje)) =
        crate::flujo::marcar_leida_de(&e, &actor, id).await
    {
        return (codigo, html! { li.error { (mensaje) } }).into_response();
    }
    let ns = crate::flujo::notificaciones_de(&e, &actor).await.unwrap_or_default();
    match ns.iter().find(|n| n.id == id) {
        Some(n) => fila_de_notificacion(n).into_response(),
        // Ya no está en su lista: se retira de la pantalla en vez de dejar una
        // fila que miente.
        None => html! {}.into_response(),
    }
}

// ── Partes ────────────────────────────────────────────────────────────────────

pub async fn partes_pagina(State(e): State<Estado>, actor: crate::autz::Actor) -> Response {
    let usuarios = match crate::participacion::usuarios_de(&e, &actor).await {
        Ok(v) => v,
        Err(err) => return error_en_pantalla("Partes", "/partes", err),
    };
    let externos = match crate::participacion::stakeholders_de(&e, &actor).await {
        Ok(v) => v,
        Err(err) => return error_en_pantalla("Partes", "/partes", err),
    };

    envoltura("Partes", Some("/partes"), html! {
        h2 { "Partes" }
        p.nota {
            "Una parte es interna —una persona del sistema— o externa —una entidad—, "
            "nunca las dos cosas. Por eso van en dos listas y no en un desplegable mezclado."
        }
        div.dos-columnas {
            section {
                h3 { "Internas (" (usuarios.len()) ")" }
                // Solo las ACTIVAS: esta lista alimenta el selector de partes, y
                // ofrecer a alguien desactivado sería proponer un flujo que no
                // puede funcionar. La administración ve también los desactivados,
                // y por eso es otra pantalla.
                p.nota { "Solo las cuentas activas." }
                (lista_de_partes(&usuarios, "No hay cuentas activas."))
            }
            section {
                h3 { "Externas (" (externos.len()) ")" }
                (lista_de_partes(&externos, "No hay entidades externas registradas."))
            }
        }
    })
    .into_response()
}

fn lista_de_partes(partes: &[crate::participacion::OpcionParte], vacio: &str) -> Markup {
    html! {
        @if partes.is_empty() {
            p.vacio { (vacio) }
        } @else {
            table.datos {
                thead { tr { th { "Nombre" } th { "Detalle" } } }
                tbody {
                    @for p in partes {
                        tr {
                            td { (p.nombre) }
                            td { (p.detalle.clone().unwrap_or_default()) }
                        }
                    }
                }
            }
        }
    }
}

// ── Cuentas (administración) ──────────────────────────────────────────────────

/// El padrón. `cuentas::listar_de` lleva DENTRO `solo_admin` **y** la línea de
/// bitácora, y las dos cosas importan aquí:
///
///   - es SOLO `Admin`. `Supervisor` NO, aunque su alcance también sea `Todas`:
///     eso es para LEER entregas. Si administrar colgara del alcance, un rol de
///     consulta podría fabricarse un administrador.
///   - un panel que leyera el padrón sin anotarlo sería la vía para leerlo sin
///     dejar rastro. Es la única lectura del sistema que devuelve dato personal
///     en bloque.
pub async fn cuentas_pagina(State(e): State<Estado>, actor: crate::autz::Actor) -> Response {
    pantalla_cuentas(&e, &actor, None, false).await
}

async fn pantalla_cuentas(
    e: &Estado,
    actor: &crate::autz::Actor,
    aviso: Option<&str>,
    es_error: bool,
) -> Response {
    let filas = match crate::cuentas::listar_de(e, actor).await {
        Ok(v) => v,
        Err(err) => return error_en_pantalla("Cuentas", "/cuentas", err),
    };

    envoltura("Cuentas", Some("/cuentas"), html! {
        h2 { "Cuentas" }
        @if let Some(a) = aviso {
            @if es_error { div.error role="alert" { (a) } }
            @else { div.aviso-resultado role="status" { (a) } }
        }
        div.tabla-ancha {
            table.datos {
                thead { tr {
                    th { "Nombre" } th { "Correo" } th { "Rol" }
                    th { "Segundo factor" } th { "Estado" } th { "Acciones" }
                } }
                tbody {
                    @for c in &filas { (fila_de_cuenta(c, actor.id)) }
                }
            }
        }

        h3 { "Dar de alta" }
        form.ficha method="post" action="/cuentas" {
            div.rejilla {
                label { "Nombre" input name="nombre" required; }
                label { "Correo" input type="email" name="email" required maxlength="320"; }
                label { "Contraseña inicial"
                    input type="password" name="clave" autocomplete="new-password" required; }
                label { "Rol"
                    select name="rol" required {
                        // De `ROLES_DE_CUENTA`, la misma lista que usa la
                        // edición. Dos listas serían dos verdades sobre qué
                        // roles existen, y la del panel sería la que se queda
                        // atrás — que es el fallo que tuve hoy en inventario con
                        // un rol «administrador» que no existía.
                        @for (v, etiqueta) in ROLES_DE_CUENTA {
                            option value=(v) { (etiqueta) }
                        }
                    } }
                label { "Entidad externa (solo para TERCERO)"
                    input type="number" name="stakeholder_id" min="1"
                          placeholder="id de la entidad"; }
            }
            p.nota {
                "Un TERCERO sin entidad vinculada no ve nada, y un rol interno con entidad "
                "es una contradicción: el servidor rechaza las dos."
            }
            button.primario type="submit" { "Crear cuenta" }
        }

        p.nota {
            "La edición de una cuenta —cambiar su rol o desactivarla— sigue en "
            code { "PUT /api/cuentas/{id}" } ": entra con su formulario y su prueba."
        }
    })
    .into_response()
}

/// Los roles, escritos como los lee `Rol::desde_bd`. Se enumeran aquí una vez y
/// los usan el alta y la edición: dos listas serían dos verdades sobre qué roles
/// existen, y la del panel sería la que se queda atrás.
const ROLES_DE_CUENTA: [(&str, &str); 4] = [
    ("OPERADOR", "Operador"),
    ("SUPERVISOR", "Supervisor"),
    ("ADMIN", "Administrador"),
    ("TERCERO", "Tercero"),
];

/// Una fila del padrón, con las acciones que corresponden a ESA fila.
///
/// `propio` es el id de quien mira. La propia cuenta NO ofrece estas dos, y no es
/// cosmética: restablecerse la clave por aquí saltaría la comprobación de la
/// contraseña actual, y quitarse el propio segundo factor sería la puerta de
/// atrás que una sesión robada usaría. El servidor las niega igual —lo hacen
/// `restablecer_clave_de` y `retirar_segundo_factor_de`—; no ofrecerlas evita
/// que alguien lo intente y crea que el sistema está roto.
fn fila_de_cuenta(c: &crate::cuentas::FichaCuenta, propio: i64) -> Markup {
    html! {
        tr.inactivo[!c.activo] {
            td { (c.nombre) }
            td { (c.email) }
            td { (c.rol) }
            // Booleano derivado, no la fecha de enrolamiento: la administración
            // solo necesita saber si hay algo que retirar, y cuándo se enroló
            // alguien no es asunto de nadie más.
            td { @if c.segundo_factor { "Sí" } @else { "No" } }
            td { @if c.activo { "Activa" } @else { "Desactivada" } }
            td.acciones-fila {
                @if c.id != propio {
                    // El rescate SOLO donde significa algo: una fila sin segundo
                    // factor no tiene nada que retirar, y ofrecerlo ahí es una
                    // acción que siempre falla.
                    @if c.segundo_factor {
                        form.enlinea method="post" action={ "/cuentas/" (c.id) "/quitar-2fa" } {
                            // Doble pulsación: retirar el segundo factor de
                            // alguien no se deshace, y deja su cuenta protegida
                            // solo por la contraseña. Lo hace `boi`; sin él el
                            // botón sigue funcionando, solo que a la primera.
                            button.secundario.chico
                                   data-confirmar="¿Seguro? Pulse otra vez"
                                   type="submit" { "Quitar 2FA" }
                        }
                    }
                    details.desplegable {
                        summary { "Restablecer clave" }
                        form.enlinea method="post" action={ "/cuentas/" (c.id) "/clave" } {
                            input type="password" name="clave_nueva"
                                  autocomplete="new-password"
                                  placeholder="Clave nueva" required;
                            button.secundario.chico type="submit" { "Aplicar" }
                        }
                    }
                    details.desplegable {
                        summary { "Editar" }
                        form.enlinea method="post" action={ "/cuentas/" (c.id) } {
                            input name="nombre" value=(c.nombre) required
                                  placeholder="Nombre";
                            select name="rol" {
                                @for (v, etiqueta) in ROLES_DE_CUENTA {
                                    option value=(v) selected[c.rol == v] { (etiqueta) }
                                }
                            }
                            input type="number" name="stakeholder_id" min="1"
                                  value=(c.stakeholder_id.map(|x| x.to_string()).unwrap_or_default())
                                  placeholder="entidad (TERCERO)";
                            label.casilla {
                                input type="checkbox" name="activo" value="true" checked[c.activo];
                                " Activa"
                            }
                            button.secundario.chico type="submit" { "Guardar" }
                        }
                    }
                } @else {
                    // La propia cuenta tampoco se edita por aquí: degradarse a
                    // uno mismo o desactivarse dejaría al sistema sin nadie que
                    // pueda arreglarlo, y esa comprobación vive en el dominio.
                    // No ofrecerla evita la tentación.
                    span.vacio { "(su cuenta)" }
                }
            }
        }
    }
}

#[derive(Deserialize)]
pub struct AltaCuenta {
    nombre: String,
    email: String,
    clave: String,
    rol: String,
    /// Llega vacío cuando no se escribe nada. **Vacío no es cero**: cero sería un
    /// id de entidad, y un rol interno con entidad lo rechaza el dominio.
    #[serde(default)]
    stakeholder_id: String,
}

pub async fn cuenta_crear(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::Form(f): axum::Form<AltaCuenta>,
) -> Response {
    let nueva = crate::cuentas::CuentaNueva {
        nombre: f.nombre,
        email: f.email,
        clave: f.clave,
        rol: f.rol,
        stakeholder_id: f.stakeholder_id.trim().parse().ok(),
    };
    match crate::cuentas::crear_de(&e, &actor, &nueva).await {
        Ok(c) => {
            let msg = format!("Cuenta creada: {} ({}).", c.nombre, c.email);
            pantalla_cuentas(&e, &actor, Some(&msg), false).await
        }
        Err(crate::rutas::Error(_, mensaje)) => {
            pantalla_cuentas(&e, &actor, Some(&mensaje), true).await
        }
    }
}

#[derive(Deserialize)]
pub struct ClaveNueva {
    clave_nueva: String,
}

pub async fn cuenta_clave(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::extract::Path(id): axum::extract::Path<i64>,
    axum::Form(f): axum::Form<ClaveNueva>,
) -> Response {
    let cuerpo = crate::cuentas::ClaveDeOtro { clave_nueva: f.clave_nueva };
    match crate::cuentas::restablecer_clave_de(&e, &actor, id, &cuerpo).await {
        Ok(()) => {
            pantalla_cuentas(
                &e,
                &actor,
                Some("Contraseña restablecida. Sus sesiones abiertas quedaron cerradas."),
                false,
            )
            .await
        }
        Err(crate::rutas::Error(_, mensaje)) => {
            pantalla_cuentas(&e, &actor, Some(&mensaje), true).await
        }
    }
}

pub async fn cuenta_editar(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::extract::Path(id): axum::extract::Path<i64>,
    cuerpo: String,
) -> Response {
    let cs = campos(&cuerpo);
    let cambio = crate::cuentas::CuentaCambio::nuevo(
        uno(&cs, "nombre").to_string(),
        uno(&cs, "rol").to_string(),
        // Una casilla sin marcar NO llega en el cuerpo: su ausencia significa
        // «desactivada». Lo define el navegador, así que aquí sí es una lectura
        // correcta y no un relleno por omisión.
        cs.iter().any(|(k, _)| k == "activo"),
        uno(&cs, "stakeholder_id").trim().parse().ok(),
    );
    match crate::cuentas::actualizar_de(&e, &actor, id, &cambio).await {
        Ok(c) => {
            let msg = format!("Cuenta de {} actualizada.", c.nombre);
            pantalla_cuentas(&e, &actor, Some(&msg), false).await
        }
        Err(crate::rutas::Error(_, mensaje)) => {
            pantalla_cuentas(&e, &actor, Some(&mensaje), true).await
        }
    }
}

pub async fn cuenta_quitar_2fa(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Response {
    match crate::cuentas::retirar_segundo_factor_de(&e, &actor, id).await {
        Ok(()) => {
            pantalla_cuentas(
                &e,
                &actor,
                Some("Segundo factor retirado, junto con sus códigos de recuperación."),
                false,
            )
            .await
        }
        Err(crate::rutas::Error(_, mensaje)) => {
            pantalla_cuentas(&e, &actor, Some(&mensaje), true).await
        }
    }
}

// ── Mi contraseña ─────────────────────────────────────────────────────────────

fn pantalla_mi_clave(aviso: Option<&str>, es_error: bool) -> Markup {
    envoltura("Mi contraseña", Some(""), html! {
        h2 { "Cambiar mi contraseña" }
        @if let Some(a) = aviso {
            @if es_error {
                div.error role="alert" { (a) }
            } @else {
                div.aviso-resultado role="status" { (a) }
            }
        }
        form.ficha method="post" action="/mi-clave" {
            div.rejilla {
                // La ACTUAL se pide siempre: sin ella, una sesión robada —una
                // cookie prestada, un navegador abierto— cambiaría la contraseña
                // y se quedaría con la cuenta para siempre.
                label { "Contraseña actual"
                    input type="password" name="clave_actual" autocomplete="current-password" required; }
                label { "Contraseña nueva"
                    input type="password" name="clave_nueva" autocomplete="new-password" required; }
            }
            button.primario type="submit" { "Cambiar" }
        }
        p.nota {
            "Al cambiarla se cierran sus sesiones en los demás dispositivos, y esta sigue "
            "abierta. Es lo que uno espera de cambiar una contraseña: que eche a quien "
            "hubiera entrado con la vieja, y no a uno mismo."
        }

        h3 { "Verificación en dos pasos" }
        p.nota {
            "Añade un código de su teléfono al entrar. "
            a href="/dos-pasos" { "Configurarla" }
        }

        h3 { "Cerrar sesión en todos los dispositivos" }
        p.nota {
            "Cierra todas sus sesiones, incluida ésta, SIN cambiar la contraseña. "
            "Sirve para cuando uno se dejó la sesión abierta en un equipo ajeno."
        }
        form.ficha method="post" action="/salir-todo" {
            button.secundario type="submit" { "Cerrar todas mis sesiones" }
        }
    })
}

/// Misma regla que `cronograma_pagina`, y aquí importa un poco más: es la
/// pantalla desde la que se cambia la contraseña propia, así que servirla a un
/// anónimo no filtra nada pero invita a intentarlo.
pub async fn mi_clave_pagina(_: ActorDePanel) -> Markup {
    pantalla_mi_clave(None, false)
}

pub async fn mi_clave_enviar(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::Form(f): axum::Form<crate::cuentas::MiClave>,
) -> Response {
    match crate::cuentas::cambiar_mi_clave_de(&e, &actor, &f).await {
        // Las cabeceras traen la cookie REEMITIDA con la versión nueva. Se
        // devuelven tal cual: perderlas aquí expulsaría a quien acaba de cambiar
        // su contraseña, que es el fallo que la reemisión existe para evitar.
        Ok(cabeceras) => {
            let mut cabeceras = cabeceras;
            cabeceras.insert(
                header::LOCATION,
                "/mi-clave?cambiada=1".parse().expect("ruta válida"),
            );
            (StatusCode::SEE_OTHER, cabeceras).into_response()
        }
        Err(crate::rutas::Error(codigo, mensaje)) => {
            (codigo, pantalla_mi_clave(Some(&mensaje), true)).into_response()
        }
    }
}

/// `POST /salir-todo` — cierra TODAS las sesiones, ésta incluida.
///
/// Va a la pantalla de entrada, y eso es lo correcto: la sesión de aquí se cerró
/// también. Dejar al usuario en una pantalla del panel con la cookie muerta le
/// haría creer que sigue dentro hasta el siguiente clic.
pub async fn salir_todo(State(e): State<Estado>, actor: crate::autz::Actor) -> Response {
    match crate::rutas::salir_todo_de(&e, &actor).await {
        Ok(cabeceras) => a_donde(cabeceras, "/entrar"),
        Err(crate::rutas::Error(codigo, mensaje)) => {
            (codigo, pantalla_mi_clave(Some(&mensaje), true)).into_response()
        }
    }
}

// ── Cronograma ────────────────────────────────────────────────────────────────

/// Lo que hace esta pantalla: **generar las entregas que faltan y decir qué
/// pasó**, incluidas las reglas que no se pudieron leer.
///
/// Lo que NO trae, y se dice aquí en vez de dejarlo implícito: el
/// previsualizador interactivo de reglas de la pantalla de Angular, que arma un
/// `ReglaJson` completo y pide `POST /api/periodicidad/previsualizar`. Ese
/// formulario tiene su propio dominio —modos, ajustes, calendario de festivos— y
/// meterlo a medias sería peor que no traerlo: una previsualización que no sale
/// del MISMO motor que genera es una promesa que después no se cumple, y por eso
/// el motor va en el estado del servidor. Entra con su formulario y su prueba.
/// Lo que la pantalla del cronograma tiene que enseñar en esta vuelta.
///
/// Va como estructura y no como ocho parámetros sueltos: con ocho, la llamada
/// era una fila de `None, None, Some(&texto), None, …` que no se puede leer ni
/// comprobar de un vistazo — y equivocarse de posición entre dos `Option<&str>`
/// del mismo tipo no lo caza el compilador. Fue clippy quien lo señaló, y tenía
/// razón: la firma había dejado de decir qué hace.
///
/// `Default` es el caso normal —la pantalla recién abierta—, y así cada llamante
/// nombra SOLO lo que trae.
#[derive(Default)]
struct VistaCronograma<'a> {
    resultado: Option<&'a crate::planificador::Resumen>,
    error: Option<&'a str>,
    /// Se conserva lo tecleado también cuando falla: reescribir una regla de
    /// veinte líneas porque el rango estaba mal es la clase de detalle que hace
    /// que una pantalla no se use.
    regla_tecleada: Option<&'a str>,
    previa: Option<&'a [crate::rutas::FilaPrevia]>,
    fechas_tecleadas: Option<&'a str>,
    lecturas: Option<&'a [siger_core::inferencia::Lectura]>,
}

fn pantalla_cronograma(v: VistaCronograma<'_>, desde: &str, hasta: &str) -> Markup {
    let VistaCronograma {
        resultado,
        error,
        regla_tecleada,
        previa,
        fechas_tecleadas,
        lecturas,
    } = v;
    envoltura("Cronograma", Some("/cronograma"), html! {
        h2 { "Cronograma" }
        @if let Some(e) = error {
            div.error role="alert" { (e) }
        }
        @if let Some(r) = resultado {
            div.aviso-resultado role="status" {
                b { (r.entregas_creadas) " entregas creadas" }
                " de " (r.informes_revisados) " informes revisados."
            }
            // LAS REGLAS ILEGIBLES SE ENSEÑAN, y es la mitad que importa. Un
            // informe activo cuya regla no se entiende NUNCA generará una tarea;
            // que el resumen diga «0 creadas» sin decir que tres reglas están
            // rotas es exactamente el silencio que este sistema persigue.
            @if !r.reglas_ilegibles.is_empty() {
                h3.alerta { (r.reglas_ilegibles.len()) " reglas no se pudieron leer" }
                p.nota {
                    "Estos informes están activos y no generarán ninguna entrega hasta que "
                    "se arregle su regla."
                }
                ul.problemas {
                    @for (id, motivo) in &r.reglas_ilegibles {
                        li { b { "Informe " (id) } ": " (motivo) }
                    }
                }
            }
        }
        form.ficha method="post" action="/cronograma/generar" {
            p {
                "Genera las entregas que falten según la regla de cada informe activo. "
                "Es idempotente: lo que ya existe no se duplica."
            }
            button.primario type="submit" { "Generar ahora" }
        }

        h3 { "Previsualizar una regla" }
        p.nota {
            "Qué vencimientos produce una regla en un rango, ANTES de guardarla. Sale del "
            "mismo motor que después genera las entregas: una previsualización que no "
            "saliera de ahí sería una promesa que después no se cumple."
        }
        form.ficha method="post" action="/cronograma/previsualizar" {
            label for="regla" { "Regla (JSON)" }
            textarea #regla name="regla" rows="5" required { (regla_tecleada.unwrap_or("")) }
            div.rejilla {
                label { "Desde" input type="date" name="desde" required value=(desde); }
                label { "Hasta" input type="date" name="hasta" required value=(hasta); }
            }
            button.secundario type="submit" { "Previsualizar" }
        }

        h3 { "¿Qué regla explica estas fechas?" }
        p.nota {
            "El camino contrario: se escriben los días en que hay que entregar y el sistema "
            "dice qué reglas los producirían. Una por línea, en "
            code { "AAAA-MM-DD" } "."
        }
        p.nota {
            "El ajuste de días no hábiles NO se deduce de las fechas —de una selección no sale "
            "si la entidad quiere gracia o anticipación—: se elige aquí, y se aplica a cada "
            "lectura antes de calcular. Mostrar unas fechas y guardar otras sería el peor "
            "defecto posible en esta pantalla."
        }
        form.ficha method="post" action="/cronograma/lecturas" {
            label for="fechas" { "Fechas" }
            textarea #fechas name="fechas" rows="5" required
                     placeholder="2026-01-15&#10;2026-02-15&#10;2026-03-15" {
                (fechas_tecleadas.unwrap_or(""))
            }
            div.rejilla {
                label.casilla { input type="checkbox" name="gracia" value="1"; " Se admite hasta el hábil siguiente" }
                label.casilla { input type="checkbox" name="anticipado" value="1"; " Se entrega el hábil anterior" }
                label.casilla { input type="checkbox" name="omitir_festivos" value="1"; " Si cae festivo, no hay vencimiento" }
            }
            button.secundario type="submit" { "Ver lecturas posibles" }
        }

        @if let Some(ls) = lecturas {
            @if ls.is_empty() {
                p.vacio { "Ninguna regla del catálogo explica esas fechas sin mentir." }
            } @else {
                p.nota { (ls.len()) " lecturas posibles:" }
                @for l in ls {
                    div.lectura {
                        b { (l.nombre) }
                        p { (l.dice) }
                        // Las primeras fechas que produciría, para verlo ANTES de
                        // decidir. Es lo que separa «esta regla suena bien» de
                        // «esta regla hace lo que quiero».
                        @if !l.proximas.is_empty() {
                            p.nota {
                                "Produciría: "
                                @for (i, f) in l.proximas.iter().enumerate() {
                                    @if i > 0 { ", " }
                                    (f)
                                }
                            }
                        }
                        details.desplegable {
                            summary { "Ver la regla (para copiarla arriba)" }
                            pre.regla { (serde_json::to_string(&l.regla).unwrap_or_default()) }
                        }
                    }
                }
            }
        }

        @if let Some(filas) = previa {
            @if filas.is_empty() {
                p.vacio { "Esa regla no produce ningún vencimiento en ese rango." }
            } @else {
                p.nota { (filas.len()) " vencimientos:" }
                div.tabla-ancha {
                    table.datos {
                        thead { tr {
                            th { "Nominal" } th { "Límite" } th { "Ajuste" } th { "¿Se movió?" }
                        } }
                        tbody {
                            @for f in filas {
                                tr {
                                    // Las dos fechas, siempre. La NOMINAL da
                                    // identidad y mide la vigencia; la LÍMITE
                                    // mide el cumplimiento (directiva 32).
                                    td { (f.nominal) }
                                    td { (f.limite) }
                                    td { (f.ajuste) }
                                    // Se DICE si se movió, en vez de dejar que
                                    // quien mira lo deduzca comparando las dos
                                    // fechas columna contra columna.
                                    td { @if f.se_movio { "sí — cayó en día no hábil" } @else { "no" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

/// Cómo tratar los días no hábiles, leído del formulario. Las tres casillas son
/// independientes y su ausencia significa «no», que es lo que define el
/// navegador — aquí sí es una lectura correcta y no un relleno por omisión.
fn ajuste_de_formulario(cs: &[(String, String)]) -> siger_core::inferencia::AjusteSel {
    let marcada = |k: &str| cs.iter().any(|(clave, _)| clave == k);
    siger_core::inferencia::AjusteSel {
        gracia: marcada("gracia"),
        anticipado: marcada("anticipado"),
        omitir_festivos: marcada("omitir_festivos"),
    }
}

/// El rango por defecto del previsualizador: de hoy a un año. No es un número
/// mágico ni el máximo que admite el servidor —ése es de cinco años—: es el
/// horizonte que alguien mira al definir una obligación anual.
fn rango_por_defecto() -> (String, String) {
    let hoy = chrono::Utc::now().date_naive();
    (hoy.to_string(), (hoy + chrono::Duration::days(365)).to_string())
}

/// El actor va aquí aunque esta pantalla no lo USE para nada: su pintado por
/// defecto es un formulario vacío, sin un solo dato del cliente. Exigirlo igual
/// es la regla —**una pantalla del panel necesita sesión**— y no el caso, porque
/// el día que este render traiga datos nadie se va a acordar de añadir la
/// puerta, y no fallaría nada (directiva 23). Lo cazó `humo.sh` pidiendo
/// `/cronograma` sin cookie: 200 donde debía haber un 303.
pub async fn cronograma_pagina(_: ActorDePanel) -> Markup {
    let (d, h) = rango_por_defecto();
    pantalla_cronograma(VistaCronograma::default(), &d, &h)
}

pub async fn cronograma_generar(State(e): State<Estado>, actor: crate::autz::Actor) -> Response {
    let (d, h) = rango_por_defecto();
    match crate::rutas::generar_de(&e, &actor).await {
        Ok(r) => pantalla_cronograma(
            VistaCronograma { resultado: Some(&r), ..Default::default() },
            &d,
            &h,
        )
        .into_response(),
        Err(crate::rutas::Error(codigo, mensaje)) => (
            codigo,
            pantalla_cronograma(
                VistaCronograma { error: Some(&mensaje), ..Default::default() },
                &d,
                &h,
            ),
        )
            .into_response(),
    }
}

/// `POST /cronograma/lecturas` — qué reglas podrían explicar unas fechas.
///
/// LAS FECHAS SE ESCRIBEN, no se marcan en un calendario. El Angular que se
/// retira traía un calendario múltiple, y esto no lo sustituye: sustituye a la
/// FUNCIÓN, que es «dime qué regla explica estos días». Un calendario es una
/// pantalla de verdad y entra cuando se construya; mientras tanto, escribir seis
/// fechas es peor de usar pero da exactamente la misma respuesta, y tenerla vale
/// más que no tenerla.
///
/// El AJUSTE va aparte y lo elige quien crea la obligación: de una selección de
/// fechas no se deduce si la entidad quiere gracia o anticipación, y mostrar unas
/// fechas y guardar otras sería el peor defecto posible en esta pantalla.
pub async fn cronograma_lecturas(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    cuerpo: String,
) -> Response {
    let cs = campos(&cuerpo);
    let texto = uno(&cs, "fechas").to_string();
    // Una línea vacía no es una fecha ilegible: se descarta antes de llegar al
    // dominio, que sí rechaza ruidosamente lo que no sabe leer.
    let fechas: Vec<String> = texto
        .split(['\n', ',', ';'])
        .map(|f| f.trim().to_string())
        .filter(|f| !f.is_empty())
        .collect();

    let ajuste = ajuste_de_formulario(&cs);
    let sel = crate::informes::Seleccion::nueva(fechas, ajuste);
    let (d, h) = rango_por_defecto();

    match crate::informes::lecturas_de(&e, &actor, &sel).await {
        Ok(ls) => pantalla_cronograma(
            VistaCronograma {
                fechas_tecleadas: Some(&texto),
                lecturas: Some(&ls),
                ..Default::default()
            },
            &d,
            &h,
        )
        .into_response(),
        Err(crate::rutas::Error(_, mensaje)) => {
            pantalla_cronograma(
                VistaCronograma {
                    error: Some(&mensaje),
                    fechas_tecleadas: Some(&texto),
                    ..Default::default()
                },
                &d,
                &h,
            )
            .into_response()
        }
    }
}

/// `POST /cronograma/previsualizar` — qué vencimientos produce una regla.
///
/// Conserva en el formulario lo que se tecleó, también cuando falla: reescribir
/// una regla de veinte líneas porque el rango estaba mal es la clase de detalle
/// que hace que una pantalla no se use.
pub async fn cronograma_previsualizar(
    State(e): State<Estado>,
    ActorDePanel(_actor): ActorDePanel,
    cuerpo: String,
) -> Response {
    let cs = campos(&cuerpo);
    let texto = uno(&cs, "regla").to_string();
    let desde_s = uno(&cs, "desde").to_string();
    let hasta_s = uno(&cs, "hasta").to_string();

    let fallo = |m: &str| {
        pantalla_cronograma(
            VistaCronograma { error: Some(m), regla_tecleada: Some(&texto), ..Default::default() },
            &desde_s,
            &hasta_s,
        )
        .into_response()
    };

    let regla: siger_core::ReglaJson = match serde_json::from_str(&texto) {
        Ok(v) => v,
        Err(err) => return fallo(&format!("la regla no tiene la forma esperada: {err}")),
    };
    let (Ok(desde), Ok(hasta)) = (desde_s.parse(), hasta_s.parse()) else {
        return fallo("las fechas del rango no son válidas");
    };

    let p = crate::rutas::PeticionPrevia::nueva(regla, desde, hasta);
    match crate::rutas::previsualizar_de(&e, &p).await {
        Ok(filas) => pantalla_cronograma(
            VistaCronograma {
                regla_tecleada: Some(&texto),
                previa: Some(&filas),
                ..Default::default()
            },
            &desde_s,
            &hasta_s,
        )
        .into_response(),
        Err(crate::rutas::Error(_, mensaje)) => fallo(&mensaje),
    }
}

// ── Detalle de una entrega: justificar y revisar ──────────────────────────────

/// La pantalla donde ocurre el trabajo: el hilo, la justificación y su revisión.
///
/// `comentarios_de` lleva DENTRO `entrega_visible`, que comprueba la pertenencia
/// EN LA CONSULTA. Así, pedir una entrega ajena por su número devuelve el mismo
/// 404 que una que no existe — que es lo que impide usar la pantalla para
/// averiguar qué números existen.
async fn pantalla_entrega(
    e: &Estado,
    actor: &crate::autz::Actor,
    id: i64,
    aviso: Option<&str>,
    es_error: bool,
) -> Response {
    let hilo = match crate::flujo::comentarios_de(e, actor, id).await {
        Ok(v) => v,
        Err(err) => return error_en_pantalla("Entrega", "/entregas", err),
    };
    let supervisa = matches!(
        actor.rol,
        crate::autz::Rol::Admin | crate::autz::Rol::Supervisor
    );

    envoltura("Entrega", Some("/entregas"), html! {
        h2 { "Entrega #" (id) }
        p { a href="/entregas" { "← Volver a entregas" } }
        @if let Some(a) = aviso {
            @if es_error { div.error role="alert" { (a) } }
            @else { div.aviso-resultado role="status" { (a) } }
        }

        h3 { "Historial" }
        @if hilo.is_empty() {
            p.vacio { "Todavía no hay comentarios en esta entrega." }
        } @else {
            ul.hilo {
                @for c in &hilo {
                    li {
                        div.cabeza { b { (c.autor) } span.momento { (c.momento) } }
                        p { (c.texto) }
                    }
                }
            }
        }

        h3 { "Entregar" }
        // Un TERCERO no entrega: RECIBE. Ofrecerle el formulario porque «también
        // ve la entrega» confundiría ver con hacer, y el servidor se lo negaría
        // igual — pero después de haberle hecho elegir un archivo.
        @if matches!(actor.rol, crate::autz::Rol::Tercero) {
            p.nota { "Su cuenta recibe informes; no registra entregas." }
        } @else {
            form.ficha method="post" action={ "/entregas/" (id) "/archivo" }
                 enctype="multipart/form-data" {
                label for="archivo" { "Archivo" }
                input #archivo type="file" name="archivo" required;
                p.nota {
                    "El estado —cumplida o extemporánea— lo decide la fecha límite, "
                    "no lo que diga el formulario."
                }
                button.primario type="submit" { "Registrar entrega" }
            }
        }

        h3 { "Justificar" }
        p.nota {
            "La justificación queda en el historial y avisa a supervisión. "
            "La escribe el responsable de la entrega; un administrador puede hacerlo en su nombre."
        }
        form.ficha method="post" action={ "/entregas/" (id) "/justificacion" } {
            label for="texto" { "Texto de la justificación" }
            // `textarea` y no `input`: una justificación es un texto, y un campo
            // de una línea empuja a escribir menos de lo que hace falta.
            textarea #texto name="texto" rows="4" required {}
            button.primario type="submit" { "Enviar justificación" }
        }

        @if supervisa {
            h3 { "Revisar" }
            p.nota {
                "El veredicto queda en el historial y se le avisa al responsable. "
                "La retroalimentación es opcional, pero es lo único que le dice qué corregir."
            }
            form.ficha method="post" action={ "/entregas/" (id) "/revision" } {
                label for="retro" { "Retroalimentación (opcional)" }
                textarea #retro name="retroalimentacion" rows="3" {}
                div.acciones {
                    // Dos botones con el MISMO nombre y distinto valor: el
                    // veredicto viaja en el que se pulsa. Sin JavaScript, y sin
                    // una casilla «aceptar» que se puede dejar a medias.
                    button.primario type="submit" name="aceptar" value="si" { "Aceptar" }
                    button.secundario type="submit" name="aceptar" value="no" { "Rechazar" }
                }
            }
        }
    })
    .into_response()
}

pub async fn entrega_pagina(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Response {
    pantalla_entrega(&e, &actor, id, None, false).await
}

#[derive(Deserialize)]
pub struct TextoJustificacion {
    texto: String,
}

pub async fn entrega_justificar(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::extract::Path(id): axum::extract::Path<i64>,
    axum::Form(f): axum::Form<TextoJustificacion>,
) -> Response {
    let j = crate::flujo::Justificacion { texto: f.texto };
    match crate::flujo::justificar_de(&e, &actor, id, &j).await {
        Ok(()) => {
            pantalla_entrega(&e, &actor, id, Some("Justificación enviada. Supervisión ya está avisada."), false).await
        }
        Err(crate::rutas::Error(_, mensaje)) => {
            pantalla_entrega(&e, &actor, id, Some(&mensaje), true).await
        }
    }
}

#[derive(Deserialize)]
pub struct VeredictoForm {
    aceptar: String,
    #[serde(default)]
    retroalimentacion: String,
}

pub async fn entrega_revisar(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::extract::Path(id): axum::extract::Path<i64>,
    axum::Form(f): axum::Form<VeredictoForm>,
) -> Response {
    // «si» acepta; CUALQUIER OTRA COSA rechaza. Es la dirección segura: un valor
    // que no se entiende no puede acabar aceptando una justificación (directiva
    // 23 — enumerar lo que SÍ significa algo).
    let r = crate::flujo::Revision {
        aceptar: f.aceptar == "si",
        retroalimentacion: f.retroalimentacion,
    };
    match crate::flujo::revisar_de(&e, &actor, id, &r).await {
        Ok(estado) => {
            let msg = format!("La justificación quedó {}. El responsable ya está avisado.", estado.to_lowercase());
            pantalla_entrega(&e, &actor, id, Some(&msg), false).await
        }
        Err(crate::rutas::Error(_, mensaje)) => {
            pantalla_entrega(&e, &actor, id, Some(&mensaje), true).await
        }
    }
}

/// `POST /entregas/{id}/archivo` — la MISMA subida que `/api`, en HTML.
///
/// `subir_de` lleva dentro todo lo que decide: la pertenencia comprobada en la
/// consulta (si no le corresponde, no se lee ni un byte), el tope de tamaño que
/// corta antes de leer, la extensión admitida, la firma y el cifrado en reposo.
/// Y el ESTADO sale de comparar con la fecha límite, no de lo que diga el
/// cliente: cumplida o extemporánea no son opciones de un formulario.
pub async fn entrega_subir(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::extract::Path(id): axum::extract::Path<i64>,
    partes: axum::extract::Multipart,
) -> Response {
    match crate::rutas::subir_de(&e, &actor, id, partes).await {
        Ok(r) => {
            let mut msg = format!(
                "Entrega registrada como {} — {} ({} bytes).",
                r.estado.to_lowercase(),
                r.nombre,
                r.bytes
            );
            if r.firmada {
                msg.push_str(" Firmada.");
            }
            // El aviso de contenido que no concuerda VIAJA a la pantalla. La
            // entrega quedó registrada y marcada para revisión: callarlo dejaría
            // sin revisar justo lo que hay que mirar.
            if r.archivo_valido == Some(false) {
                msg.push_str(
                    " AVISO: el contenido no concuerda con la extensión. Quedó marcada para revisión.",
                );
            }
            pantalla_entrega(&e, &actor, id, Some(&msg), false).await
        }
        Err(crate::rutas::Error(_, mensaje)) => {
            pantalla_entrega(&e, &actor, id, Some(&mensaje), true).await
        }
    }
}

// ── Verificación en dos pasos ─────────────────────────────────────────────────
//
// La pantalla de la cuenta PROPIA. Nada de aquí decide: los cinco caminos los
// resuelven las funciones de `segundofactor`, que son las mismas que usa `/api`,
// y que se llevan dentro lo que no puede tener dos versiones — que activar exige
// código Y contraseña, que desactivar y renovar exigen la contraseña, y que sin
// frase de reposo no se puede enrolar.

async fn pantalla_2fa(
    e: &Estado,
    actor: &crate::autz::Actor,
    codigos: Option<&[String]>,
    aviso: Option<&str>,
    es_error: bool,
) -> Response {
    let st = match crate::segundofactor::estado_de(e, actor).await {
        Ok(v) => v,
        Err(err) => return error_en_pantalla("Dos pasos", "", err),
    };

    envoltura("Verificación en dos pasos", Some(""), html! {
        h2 { "Verificación en dos pasos" }
        @if let Some(a) = aviso {
            @if es_error { div.error role="alert" { (a) } }
            @else { div.aviso-resultado role="status" { (a) } }
        }

        // LOS CÓDIGOS SE ENSEÑAN UNA VEZ, y la pantalla lo dice. Guardados solo
        // como hash Argon2: nadie —ni un administrador— puede volver a
        // mostrarlos. Si se pierden, el camino es renovarlos, no recuperarlos.
        @if let Some(cs) = codigos {
            div.codigos {
                h3 { "Sus diez códigos de recuperación" }
                p.alerta {
                    "Se muestran UNA sola vez. Guárdelos ahora, en papel o en su gestor de "
                    "contraseñas: se guardan cifrados y no hay forma de volver a verlos."
                }
                ul { @for c in cs { li { code { (c) } } } }
                p.nota {
                    "Cada uno sirve una vez y entra en lugar del código del teléfono. "
                    "Son la salida el día que el teléfono no esté."
                }
            }
        }

        h3 { "Estado" }
        div.resumen {
            div { span { @if st.activo { "Activo" } @else { "Inactivo" } } " segundo factor" }
            @if st.activo {
                // Cero códigos con el factor activo es un aviso, no un dato: el
                // día que falte el teléfono no habrá por dónde entrar.
                div {
                    span class=(if st.codigos_disponibles == 0 { "alerta" } else { "" }) {
                        (st.codigos_disponibles)
                    }
                    " códigos sin usar"
                }
            }
        }

        @if !st.puede_activarse && !st.activo {
            // Se DICE que el despliegue no puede, en vez de ofrecer un botón que
            // va a fallar. El secreto se guarda cifrado con la frase de reposo;
            // sin ella no se guarda en claro «mientras tanto».
            div.error role="alert" {
                "Este servidor no tiene configurada la frase de reposo, así que no se puede "
                "activar el segundo factor: el secreto se guarda cifrado, y sin frase no hay "
                "con qué cifrarlo. Es cosa de quien administra el servidor."
            }
        } @else if !st.activo {
            h3 { "Activar" }
            p.nota {
                "Primero se genera el código QR; después se teclea el número que muestre su "
                "aplicación, junto con su contraseña. El factor NO queda activo hasta ese "
                "segundo paso: así, un reloj mal puesto no le deja fuera de su cuenta."
            }
            form.ficha method="post" action="/dos-pasos/iniciar" {
                button.primario type="submit" { "Generar código QR" }
            }
        } @else {
            h3 { "Renovar los códigos de recuperación" }
            p.nota {
                "Genera diez nuevos y anula los anteriores. NO toca el secreto: su aplicación "
                "sigue funcionando igual y no hay que volver a escanear nada."
            }
            form.ficha method="post" action="/dos-pasos/codigos" {
                label { "Su contraseña"
                    input type="password" name="clave" autocomplete="current-password" required; }
                button.secundario type="submit" { "Renovar los diez" }
            }

            h3 { "Desactivar" }
            p.nota {
                "Apaga el segundo factor y borra sus códigos de recuperación. Su cuenta queda "
                "protegida solo por la contraseña."
            }
            form.ficha method="post" action="/dos-pasos/desactivar" {
                label { "Su contraseña"
                    input type="password" name="clave" autocomplete="current-password" required; }
                button.secundario data-confirmar="¿Seguro? Pulse otra vez" type="submit" {
                    "Desactivar"
                }
            }
        }
    })
    .into_response()
}

pub async fn dos_pasos_pagina(State(e): State<Estado>, actor: crate::autz::Actor) -> Response {
    pantalla_2fa(&e, &actor, None, None, false).await
}

/// El QR y el secreto, para escanear. **No activa nada**: el factor solo queda
/// puesto tras teclear un código, que es lo que impide que un reloj desajustado
/// deje a alguien fuera de su propia cuenta.
pub async fn dos_pasos_iniciar(State(e): State<Estado>, actor: crate::autz::Actor) -> Response {
    let enr = match crate::segundofactor::iniciar_de(&e, &actor).await {
        Ok(v) => v,
        Err(crate::rutas::Error(_, mensaje)) => {
            return pantalla_2fa(&e, &actor, None, Some(&mensaje), true).await;
        }
    };

    envoltura("Activar dos pasos", Some(""), html! {
        h2 { "Activar la verificación en dos pasos" }
        div.enrolamiento {
            div {
                h3 { "1. Escanee este código" }
                // El SVG va dentro de un `<img>` con `data:`, no con contenido
                // en línea: así el navegador lo trata como un documento aparte y
                // sin guiones. La CSP lo admite por `img-src 'self' data:`.
                img.qr src=(enr.uri_de_datos()) alt="Código QR para la aplicación de autenticación";
                p.nota {
                    "¿La cámara no lo lee? Teclee esta clave en su aplicación:"
                    br;
                    code.secreto { (enr.secreto()) }
                }
            }
            div {
                h3 { "2. Confirme" }
                p.nota {
                    "Escriba el número que muestre su aplicación y su contraseña. Hasta que no "
                    "lo haga, el segundo factor NO está activo y su cuenta entra como siempre."
                }
                form.ficha method="post" action="/dos-pasos/activar" {
                    label { "Código de la aplicación"
                        input name="codigo" inputmode="numeric" autocomplete="one-time-code"
                              required autofocus; }
                    label { "Su contraseña"
                        input type="password" name="clave" autocomplete="current-password" required; }
                    button.primario type="submit" { "Activar" }
                }
                p.nota {
                    "Al activarlo se le mostrarán diez códigos de recuperación, UNA sola vez."
                }
            }
        }
    })
    .into_response()
}

#[derive(Deserialize)]
pub struct CodigoYClaveForm {
    codigo: String,
    clave: String,
}

pub async fn dos_pasos_activar(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::Form(f): axum::Form<CodigoYClaveForm>,
) -> Response {
    let p = crate::segundofactor::CodigoYClave::nuevo(f.codigo, f.clave);
    match crate::segundofactor::activar_de(&e, &actor, &p).await {
        // Los códigos vuelven a la pantalla y se enseñan AQUÍ, una vez. No se
        // guardan en sesión ni se piden después: solo existen en esta respuesta.
        Ok(codigos) => {
            pantalla_2fa(
                &e,
                &actor,
                Some(&codigos),
                Some("Segundo factor activado."),
                false,
            )
            .await
        }
        Err(crate::rutas::Error(_, mensaje)) => {
            pantalla_2fa(&e, &actor, None, Some(&mensaje), true).await
        }
    }
}

#[derive(Deserialize)]
pub struct SoloClaveForm {
    clave: String,
}

pub async fn dos_pasos_desactivar(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::Form(f): axum::Form<SoloClaveForm>,
) -> Response {
    let p = crate::segundofactor::SoloClave::nueva(f.clave);
    match crate::segundofactor::desactivar_de(&e, &actor, &p).await {
        Ok(()) => {
            pantalla_2fa(
                &e,
                &actor,
                None,
                Some("Segundo factor desactivado. Sus códigos de recuperación se borraron."),
                false,
            )
            .await
        }
        Err(crate::rutas::Error(_, mensaje)) => {
            pantalla_2fa(&e, &actor, None, Some(&mensaje), true).await
        }
    }
}

pub async fn dos_pasos_codigos(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::Form(f): axum::Form<SoloClaveForm>,
) -> Response {
    let p = crate::segundofactor::SoloClave::nueva(f.clave);
    match crate::segundofactor::renovar_codigos_de(&e, &actor, &p).await {
        Ok(codigos) => {
            pantalla_2fa(
                &e,
                &actor,
                Some(&codigos),
                Some("Diez códigos nuevos. Los anteriores ya no sirven."),
                false,
            )
            .await
        }
        Err(crate::rutas::Error(_, mensaje)) => {
            pantalla_2fa(&e, &actor, None, Some(&mensaje), true).await
        }
    }
}

// ── Formularios con líneas: el parseo que axum::Form no hace ──────────────────
//
// La participación reemplaza el conjunto COMPLETO de partes de un informe, así
// que el formulario manda claves repetidas —`parte_rol=ORIGEN&parte_rol=DESTINO`—
// y `axum::Form` NO las junta en un `Vec`: `serde_urlencoded` se queda con la
// última y el resto desaparece EN SILENCIO. Un informe con cinco partes se
// guardaría con una, sin un solo error.

fn campos(cuerpo: &str) -> Vec<(String, String)> {
    cuerpo
        .split('&')
        .filter(|p| !p.is_empty())
        .filter_map(|par| {
            let (k, v) = par.split_once('=')?;
            Some((descodificar(k), descodificar(v)))
        })
        .collect()
}

/// `application/x-www-form-urlencoded`: `+` es espacio y `%XX` es un byte.
fn descodificar(s: &str) -> String {
    let bytes = s.replace('+', " ").into_bytes();
    let mut salida = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let alto = (bytes[i + 1] as char).to_digit(16);
            let bajo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(a), Some(b)) = (alto, bajo) {
                salida.push((a * 16 + b) as u8);
                i += 3;
                continue;
            }
        }
        salida.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&salida).into_owned()
}

fn uno<'a>(cs: &'a [(String, String)], clave: &str) -> &'a str {
    cs.iter().find(|(k, _)| k == clave).map(|(_, v)| v.as_str()).unwrap_or("")
}

fn todos<'a>(cs: &'a [(String, String)], clave: &str) -> Vec<&'a str> {
    cs.iter().filter(|(k, _)| k == clave).map(|(_, v)| v.as_str()).collect()
}

// ── Informes: alta y edición ──────────────────────────────────────────────────

/// El formulario de un informe. `id = None` es alta; `Some` es edición.
///
/// LA REGLA SE EDITA COMO JSON, y conviene decir por qué en vez de disimularlo:
/// una regla tiene modo, ajustes y calendario de festivos, y el editor visual que
/// eso merece es el previsualizador —que no está portado—. Un formulario a medias
/// sería peor: dejaría creer que se puede componer cualquier regla desde aquí.
/// Lo que SÍ está es la validación, que corre con el MISMO motor que después la
/// ejecuta: una regla ilegible se rechaza con el administrador delante, en vez de
/// guardarse y descubrir meses después que ese informe nunca generó una entrega.
fn formulario_informe(
    detalle: Option<&crate::informes::InformeDetalle>,
    aviso: Option<&str>,
    es_error: bool,
) -> Markup {
    let (titulo, accion) = match detalle {
        None => ("Nuevo informe", "/informes".to_string()),
        Some(d) => ("Editar informe", format!("/informes/{}", d.id)),
    };
    html! {
        h3 { (titulo) }
        @if let Some(a) = aviso {
            @if es_error { div.error role="alert" { (a) } }
            @else { div.aviso-resultado role="status" { (a) } }
        }
        form.ficha method="post" action=(accion) {
            div.rejilla {
                label { "Nombre"
                    input name="nombre" required maxlength="200"
                          value=(detalle.map(|d| d.nombre.as_str()).unwrap_or("")); }
                label { "Descripción"
                    input name="descripcion"
                          value=(detalle.and_then(|d| d.descripcion.as_deref()).unwrap_or("")); }
                @if detalle.is_some() {
                    label.casilla {
                        input type="checkbox" name="activo" value="true"
                              checked[detalle.is_some_and(|d| d.activo)];
                        " Activo"
                    }
                }
            }
            label for="recurrencia" { "Regla de periodicidad (JSON)" }
            textarea #recurrencia name="recurrencia" rows="6" required {
                (detalle.map(|d| d.recurrencia.as_str()).unwrap_or(""))
            }
            p.nota {
                "Se valida con el mismo motor que después la ejecuta. Si no se puede leer, "
                "se rechaza aquí: un informe con una regla que no parsea es un informe que "
                "nunca vencerá, y el fallo aparecería lejos de su causa."
            }
            button.primario type="submit" {
                @if detalle.is_some() { "Guardar cambios" } @else { "Crear informe" }
            }
        }
    }
}

async fn pantalla_informes(
    e: &Estado,
    actor: &crate::autz::Actor,
    aviso: Option<&str>,
    es_error: bool,
) -> Response {
    let filas = match crate::informes::listar_de(e, actor).await {
        Ok(v) => v,
        Err(err) => return error_en_pantalla("Informes", "/informes", err),
    };
    let administra = matches!(actor.rol, crate::autz::Rol::Admin);

    envoltura("Informes", Some("/informes"), html! {
        h2 { "Informes" }
        p.nota { "Las obligaciones definidas: quién las produce, cada cuánto y si están activas." }
        @if filas.is_empty() {
            p.vacio { "No hay informes definidos todavía." }
        } @else {
            div.tabla-ancha {
                table.datos {
                    thead { tr {
                        th { "Nombre" } th { "Descripción" } th { "Periodicidad" }
                        th { "Estado" } th { "Acciones" }
                    } }
                    tbody {
                        @for f in &filas {
                            tr.inactivo[!f.activo] {
                                td { (f.nombre) }
                                td { (f.descripcion.clone().unwrap_or_default()) }
                                // El modo viene ya extraído del JSON por SQL. Si
                                // falta, se DICE que falta en vez de dejar la
                                // celda vacía: una celda en blanco se lee como
                                // «no aplica», que es otra cosa.
                                td { (f.modo.clone().unwrap_or_else(|| "sin regla".into())) }
                                td { @if f.activo { "Activo" } @else { "Inactivo" } }
                                td {
                                    // Editar y las partes son de administración.
                                    // Un supervisor LEE la tabla y no toca nada;
                                    // ofrecerle los enlaces sería prometerle algo
                                    // que el servidor le va a negar.
                                    @if administra {
                                        a.enlace-accion href={ "/informes/" (f.id) } { "Editar" }
                                        a.enlace-accion href={ "/informes/" (f.id) "/partes" } { "Partes" }
                                    } @else {
                                        span.vacio { "—" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        @if administra { (formulario_informe(None, aviso, es_error)) }
    })
    .into_response()
}

pub async fn informes_pagina(State(e): State<Estado>, actor: crate::autz::Actor) -> Response {
    pantalla_informes(&e, &actor, None, false).await
}

/// Arma la entrada desde el formulario. La regla llega como TEXTO y se parsea
/// aquí; si no es JSON válido se dice cuál es el problema, en vez de mandar algo
/// que el validador rechazaría con otro mensaje más lejano.
fn entrada_de_formulario(
    cs: &[(String, String)],
) -> Result<crate::informes::InformeEntrada, String> {
    let texto = uno(cs, "recurrencia");
    let regla: serde_json::Value = serde_json::from_str(texto)
        .map_err(|err| format!("la regla no es JSON válido: {err}"))?;
    let desc = uno(cs, "descripcion").trim().to_string();
    Ok(crate::informes::InformeEntrada::nueva(
        uno(cs, "nombre").to_string(),
        // Vacío no es `Some("")`: una descripción en blanco es NO tener
        // descripción, y guardar la cadena vacía deja una fila que dice algo
        // distinto de lo que el usuario quiso decir.
        if desc.is_empty() { None } else { Some(desc) },
        regla,
        // Una casilla sin marcar no llega en el cuerpo. En el ALTA no hay
        // casilla y el informe nace activo, que es lo que dice el dominio.
        cs.iter().any(|(k, _)| k == "activo") || !cs.iter().any(|(k, _)| k == "recurrencia_editando"),
    ))
}

pub async fn informe_crear(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    cuerpo: String,
) -> Response {
    let cs = campos(&cuerpo);
    let entrada = match entrada_de_formulario(&cs) {
        Ok(v) => v,
        Err(m) => return pantalla_informes(&e, &actor, Some(&m), true).await,
    };
    match crate::informes::crear_de(&e, &actor, &entrada).await {
        Ok(id) => {
            pantalla_informes(&e, &actor, Some(&format!("Informe {id} creado.")), false).await
        }
        Err(crate::rutas::Error(_, mensaje)) => {
            pantalla_informes(&e, &actor, Some(&mensaje), true).await
        }
    }
}

pub async fn informe_pagina(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Response {
    pantalla_informe(&e, &actor, id, None, false).await
}

async fn pantalla_informe(
    e: &Estado,
    actor: &crate::autz::Actor,
    id: i64,
    aviso: Option<&str>,
    es_error: bool,
) -> Response {
    let d = match crate::informes::detalle_de(e, actor, id).await {
        Ok(v) => v,
        Err(err) => return error_en_pantalla("Informe", "/informes", err),
    };
    envoltura("Editar informe", Some("/informes"), html! {
        h2 { (d.nombre) }
        p {
            a href="/informes" { "← Volver a informes" }
            " · "
            a href={ "/informes/" (id) "/partes" } { "Partes de este informe" }
        }
        (formulario_informe(Some(&d), aviso, es_error))
        p.nota {
            "Cambiar la regla NO borra ni recalcula las entregas ya existentes: la historia "
            "no se reescribe. Las futuras salen de la regla nueva cuando corra el "
            "planificador; para verlas ya, «Generar ahora» en Cronograma."
        }
    })
    .into_response()
}

pub async fn informe_editar(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::extract::Path(id): axum::extract::Path<i64>,
    cuerpo: String,
) -> Response {
    let mut cs = campos(&cuerpo);
    // Marca para que `entrada_de_formulario` sepa que viene de la EDICIÓN, donde
    // la casilla existe y su ausencia significa «inactivo». En el alta no hay
    // casilla y el informe nace activo.
    cs.push(("recurrencia_editando".into(), "1".into()));
    let entrada = match entrada_de_formulario(&cs) {
        Ok(v) => v,
        Err(m) => return pantalla_informe(&e, &actor, id, Some(&m), true).await,
    };
    match crate::informes::actualizar_de(&e, &actor, id, &entrada).await {
        Ok(()) => pantalla_informe(&e, &actor, id, Some("Informe guardado."), false).await,
        Err(crate::rutas::Error(_, mensaje)) => {
            pantalla_informe(&e, &actor, id, Some(&mensaje), true).await
        }
    }
}

// ── Partes de un informe ──────────────────────────────────────────────────────

const ROLES_DE_PARTE: [(&str, &str); 5] = [
    ("ORIGEN", "Origen — de dónde sale la obligación"),
    ("EJECUTA", "Ejecuta — quien produce la entrega"),
    ("SUPERVISA", "Supervisa — quien revisa"),
    ("DESTINO", "Destino — a quién se le entrega"),
    ("USUARIO", "Usuario — quien lo consume"),
];

async fn pantalla_partes(
    e: &Estado,
    actor: &crate::autz::Actor,
    id: i64,
    aviso: Option<&str>,
    es_error: bool,
) -> Response {
    let partes = match crate::participacion::partes_de(e, actor, id).await {
        Ok(v) => v,
        Err(err) => return error_en_pantalla("Partes", "/informes", err),
    };
    let usuarios = crate::participacion::usuarios_de(e, actor).await.unwrap_or_default();
    let externos = crate::participacion::stakeholders_de(e, actor).await.unwrap_or_default();

    envoltura("Partes del informe", Some("/informes"), html! {
        h2 { "Partes del informe #" (id) }
        p { a href={ "/informes/" (id) } { "← Volver al informe" } }
        @if let Some(a) = aviso {
            @if es_error { div.error role="alert" { (a) } }
            @else { div.aviso-resultado role="status" { (a) } }
        }
        p.nota {
            "Sin partes, el planificador genera entregas SIN responsable y las alertas no "
            "tienen a quién avisar. El formulario reemplaza el conjunto COMPLETO: se manda la "
            "lista entera, no parches, para que no queden restos de una edición anterior."
        }

        form.ficha method="post" action={ "/informes/" (id) "/partes" } {
            table.datos #partes {
                thead { tr { th { "Quién" } th { "Papel" } th { "Principal" } th.num { "Orden" } } }
                tbody #cuerpo-partes {
                    @if partes.is_empty() {
                        (fila_de_parte(None, &usuarios, &externos))
                    } @else {
                        @for p in &partes { (fila_de_parte(Some(p), &usuarios, &externos)) }
                    }
                }
            }
            button.secundario type="button"
                   hx-get={ "/informes/" (id) "/partes/fila" }
                   hx-target="#cuerpo-partes" hx-swap="beforeend" { "Añadir parte" }
            p.nota {
                "Una parte es interna —una persona— o externa —una entidad—, nunca las dos. "
                "Deje en «—» la que no corresponda."
            }
            button.primario type="submit" { "Guardar las partes" }
        }
    })
    .into_response()
}

fn fila_de_parte(
    p: Option<&crate::participacion::ParteVista>,
    usuarios: &[crate::participacion::OpcionParte],
    externos: &[crate::participacion::OpcionParte],
) -> Markup {
    html! {
        tr.linea {
            td {
                label.chico { "Interna"
                    select name="parte_usuario" {
                        option value="" { "—" }
                        @for u in usuarios {
                            option value=(u.id) selected[p.and_then(|x| x.usuario_id) == Some(u.id)] {
                                (u.nombre)
                            }
                        }
                    } }
                label.chico { "Externa"
                    select name="parte_externo" {
                        option value="" { "—" }
                        @for x in externos {
                            option value=(x.id) selected[p.and_then(|y| y.stakeholder_id) == Some(x.id)] {
                                (x.nombre)
                            }
                        }
                    } }
            }
            td {
                select name="parte_rol" {
                    @for (valor, etiqueta) in ROLES_DE_PARTE {
                        option value=(valor) selected[p.is_some_and(|x| x.rol == valor)] { (etiqueta) }
                    }
                }
            }
            td {
                // La casilla NO llega al cuerpo cuando va sin marcar, así que
                // viaja acompañada de un campo oculto con el mismo nombre: la
                // posición se conserva y el valor es «no» salvo que se marque.
                // Sin él, una fila desmarcada desplazaría todas las siguientes.
                input type="hidden" name="parte_principal" value="no";
                input type="checkbox" name="parte_principal_marca" value="si"
                      checked[p.is_none_or(|x| x.es_principal)];
            }
            td.num { input type="number" name="parte_orden" min="0" step="1"
                           value=(p.map(|x| x.orden).unwrap_or(0)); }
        }
    }
}

pub async fn partes_pagina_informe(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Response {
    pantalla_partes(&e, &actor, id, None, false).await
}

pub async fn partes_fila(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
) -> Response {
    let usuarios = crate::participacion::usuarios_de(&e, &actor).await.unwrap_or_default();
    let externos = crate::participacion::stakeholders_de(&e, &actor).await.unwrap_or_default();
    fila_de_parte(None, &usuarios, &externos).into_response()
}

pub async fn partes_guardar(
    State(e): State<Estado>,
    ActorDePanel(actor): ActorDePanel,
    axum::extract::Path(id): axum::extract::Path<i64>,
    cuerpo: String,
) -> Response {
    let cs = campos(&cuerpo);
    let internos = todos(&cs, "parte_usuario");
    let externos = todos(&cs, "parte_externo");
    let roles = todos(&cs, "parte_rol");
    let ordenes = todos(&cs, "parte_orden");
    // La casilla marcada añade su propio valor DESPUÉS del oculto, así que hay
    // dos entradas por fila cuando está marcada y una cuando no. Se cuentan las
    // marcadas por posición reconstruyendo la secuencia.
    let principales = todos(&cs, "parte_principal_marca");

    let mut partes = Vec::new();
    for (i, rol) in roles.iter().enumerate() {
        let interno: Option<i64> = internos.get(i).and_then(|v| v.parse().ok());
        let externo: Option<i64> = externos.get(i).and_then(|v| v.parse().ok());
        // Una fila con las DOS vacías es una fila que el usuario dejó sin llenar
        // —el botón «Añadir parte» las crea vacías—: se descarta en vez de
        // mandarla y que `valida_parte` rechace el conjunto entero.
        if interno.is_none() && externo.is_none() {
            continue;
        }
        partes.push(crate::participacion::ParteEntrada::nueva(
            interno,
            externo,
            (*rol).to_string(),
            // Con `principales` más corto que `roles`, la fila no estaba marcada.
            principales.len() > i,
            ordenes.get(i).and_then(|v| v.parse().ok()).unwrap_or(0),
        ));
    }

    match crate::participacion::reemplazar_de(&e, &actor, id, &partes).await {
        Ok(()) => {
            let msg = format!("{} partes guardadas.", partes.len());
            pantalla_partes(&e, &actor, id, Some(&msg), false).await
        }
        Err(crate::rutas::Error(_, mensaje)) => {
            pantalla_partes(&e, &actor, id, Some(&mensaje), true).await
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// **Ninguna pantalla del panel se sirve sin sesión.**
    ///
    /// Existe porque ya pasó: `/cronograma` y `/mi-clave` respondían **200** a
    /// un anónimo, y ninguna prueba lo veía. Lo cazó `humo.sh` —hablándole al
    /// binario por socket— y no el banco, que es la directiva 28 en su forma
    /// habitual: lo que no cruza la frontera del programa no se mide.
    ///
    /// Ninguna de las dos filtraba un dato: su pintado por defecto es un
    /// formulario vacío, y por eso el autor —yo— no necesitó el actor y no lo
    /// puso. Justo por eso hace falta un guardián y no dos arreglos: **la regla
    /// es que una pantalla del panel exige sesión aunque hoy no enseñe nada**,
    /// porque el día que ese render traiga datos no fallaría nada (directiva 23).
    ///
    /// Lee el FUENTE, que es donde está la respuesta, y compara los manejadores
    /// que `rutas.rs` cablea fuera de `/api` contra la firma que declaran aquí.
    #[test]
    fn ninguna_pantalla_del_panel_se_sirve_sin_sesion() {
        /// Lo que está abierto A PROPÓSITO, con su motivo. Cada línea es una
        /// decisión escrita; quitar una de aquí sin ponerle actor pone la prueba
        /// en rojo, que es lo que se quiere.
        const ABIERTAS: &[(&str, &str)] = &[
            ("acceso_pagina", "es la puerta: todavía no hay sesión"),
            ("acceso_enviar", "idem, y es quien la crea"),
            ("codigo_pagina", "segundo paso del login: hay token pendiente, no sesión"),
            ("codigo_enviar", "idem"),
            ("salir", "cerrar la sesión propia no necesita tenerla"),
            ("htmx_js", "activo estático"),
            ("boi_js", "activo estático"),
            ("comportamientos_js", "activo estático"),
            ("panel_css", "activo estático"),
        ];

        let rutas = include_str!("rutas.rs");
        let vista = include_str!("vista.rs");

        // SOLO la cadena del router, no el archivo entero: una mención en PROSA
        // —y este archivo tiene varias— pondría la prueba roja sin motivo.
        let inicio = rutas.find("Router::new()").expect("no se encontró el router");
        let cadena = &rutas[inicio..];

        let mut manejadores: Vec<&str> = Vec::new();
        for (i, _) in cadena.match_indices("crate::vista::") {
            let resto = &cadena[i + "crate::vista::".len()..];
            let fin = resto
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(resto.len());
            let nombre = &resto[..fin];
            if !nombre.is_empty() && !manejadores.contains(&nombre) {
                manejadores.push(nombre);
            }
        }
        // Un umbral, no un número exacto: exacto se actualiza a ciegas con cada
        // ruta nueva. 35 y no 10 — hay 41, así que la holgura es de seis
        // (directiva 30).
        assert!(
            manejadores.len() >= 35,
            "solo se encontraron {} manejadores de panel: ¿cambió la forma de \
             registrar rutas y esta prueba dejó de mirar nada?",
            manejadores.len()
        );

        let mut pelados = Vec::new();
        for nombre in &manejadores {
            if ABIERTAS.iter().any(|(n, _)| n == nombre) {
                continue;
            }
            let firma = format!("pub async fn {nombre}(");
            let Some(i) = vista.find(&firma) else { continue };
            // La lista de parámetros, contando paréntesis: `State(e)` lleva los
            // suyos, y cortar en el primer `)` decía que NINGÚN manejador pedía
            // actor. Comprobado: con el corte ingenuo salían 41 pelados de 41.
            let desde = i + firma.len() - 1;
            let mut prof = 0usize;
            let mut hasta = desde;
            for (k, c) in vista[desde..].char_indices() {
                match c {
                    '(' => prof += 1,
                    ')' => {
                        prof -= 1;
                        if prof == 0 {
                            hasta = desde + k;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if !vista[desde..hasta].contains("Actor") {
                pelados.push(*nombre);
            }
        }

        assert!(
            pelados.is_empty(),
            "estas pantallas del panel se sirven SIN SESIÓN: {pelados:?}.\n\
             Cualquiera con la URL las abre. Ponles `ActorDePanel` —o
             `crate::autz::Actor` si además usan el actor— o añádelas a \
             `ABIERTAS` con el motivo escrito si de verdad deben estarlo.",
        );
    }

    /// La envoltura sirve los tres archivos LOCALES y ninguno de fuera: los
    /// perfiles `onprem` y `standalone` son sin internet.
    #[test]
    fn la_envoltura_no_apunta_a_ningun_origen_externo() {
        let html = envoltura("t", Some("/entregas"), html! { p { "x" } }).into_string();
        assert!(html.contains(r#"src="/htmx.min.js""#));
        assert!(html.contains(r#"src="/boi.js""#));
        assert!(html.contains(r#"src="/comportamientos.js""#));
        assert!(html.contains(r#"href="/panel.css""#));
        assert!(!html.contains("http://"), "apunta fuera: {html:.300}");
        assert!(!html.contains("https://"), "apunta fuera: {html:.300}");
    }

    /// El comportamiento que la envoltura pide TIENE que existir dentro del
    /// binario, y tiene que ser el que la pantalla usa.
    ///
    /// Sin esto, `comportamientos.js` podría quedarse vacío o registrar otro
    /// nombre y el botón de «Quitar 2FA» perdería su confirmación **sin dar un
    /// error**: seguiría siendo un botón, y la primera pulsación retiraría el
    /// segundo factor de alguien. Es el mismo modo de fallo que la CSP muda de
    /// inventario — el JavaScript que no corre no se queja.
    #[test]
    fn el_comportamiento_de_confirmacion_viaja_en_el_binario_y_es_el_que_se_usa() {
        assert!(
            COMPORTAMIENTOS_JS.contains(r#"registrar("confirmar-doble", "[data-confirmar]""#),
            "el comportamiento «confirmar-doble» no está registrado"
        );
        assert!(
            COMPORTAMIENTOS_JS.contains(r#"from "./boi.js""#),
            "los comportamientos no se registran contra el núcleo de boi"
        );
        // Y la pareja: la pantalla que lo NECESITA lleva el atributo. Sin esta
        // mitad, el comportamiento podría estar y no usarlo nadie.
        let ficha = crate::cuentas::FichaCuenta {
            id: 2,
            nombre: "Otra".into(),
            email: "otra@x.co".into(),
            rol: "OPERADOR".into(),
            activo: true,
            stakeholder_id: None,
            segundo_factor: true,
        };
        let html = fila_de_cuenta(&ficha, 1).into_string();
        assert!(html.contains("data-confirmar"), "el botón de quitar 2FA va sin confirmación");
    }

    /// Sin sesión no se dibuja la barra: enumerar las secciones le dice a quien
    /// no ha entrado qué hay dentro.
    #[test]
    fn las_pantallas_de_acceso_no_llevan_barra_ni_nombran_las_secciones() {
        for html in [
            pantalla_acceso(None, &[]).into_string(),
            pantalla_codigo(None).into_string(),
        ] {
            assert!(!html.contains("class=\"barra\""), "trae la barra");
            for (_, nombre) in SECCIONES {
                assert!(!html.contains(nombre), "nombra la sección «{nombre}»");
            }
        }
    }

    /// La pareja del anterior (directiva 33): con sesión la barra SÍ está y
    /// marca dónde estamos. Sin esta mitad, una envoltura que no dibujara nunca
    /// nada pasaría igual.
    #[test]
    fn con_sesion_la_barra_esta_y_marca_la_seccion_actual() {
        let html = envoltura("t", Some("/partes"), html! {}).into_string();
        assert!(html.contains("class=\"barra\""));
        for (_, nombre) in SECCIONES {
            assert!(html.contains(nombre), "falta la sección «{nombre}»");
        }
        assert!(
            html.contains(r#"<a href="/partes" class="activo">"#),
            "no se marcó la sección actual: {html:.400}"
        );
    }

    /// Ni un manejador de evento en línea: `script-src 'self'` no los ejecuta, y
    /// la pantalla queda muerta sin dar un error. Es la lección de `inventario`,
    /// donde un `onchange="this.form.submit()"` dejó una pantalla inservible.
    #[test]
    fn ninguna_pantalla_lleva_manejadores_de_evento_en_linea() {
        let manejadores = [" onchange=", " onclick=", " onload=", " onerror=", " onsubmit=", " oninput="];
        for html in [
            pantalla_acceso(Some("x"), &[]).into_string(),
            pantalla_codigo(Some("x")).into_string(),
            pantalla_mi_clave(Some("x"), true).into_string(),
            pantalla_cronograma(
                VistaCronograma { error: Some("x"), regla_tecleada: Some("{}"), ..Default::default() },
                "2026-01-01",
                "2026-12-31",
            )
                .into_string(),
            // El formulario de un informe, en sus dos formas: alta y edición.
            formulario_informe(None, Some("x"), true).into_string(),
            envoltura("t", Some("/entregas"), html! { p { "x" } }).into_string(),
        ] {
            let bajo = html.to_ascii_lowercase();
            for m in manejadores {
                assert!(!bajo.contains(m), "hay un «{}» que la CSP no ejecuta", m.trim());
            }
        }
        // Pareja: la sonda tiene que saber VER uno.
        let falso = r#"<select onchange="x()">"#.to_ascii_lowercase();
        assert!(manejadores.iter().any(|m| falso.contains(m)), "la sonda no ve un manejador");
    }
}
