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
pub async fn panel_css() -> Response {
    recurso(PANEL_CSS, "text/css; charset=utf-8")
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
fn pantalla_acceso(error: Option<&str>) -> Markup {
    envoltura("Entrar", None, html! {
        div.centro {
            div.caja {
                h1 { "SIGER" }
                p.lema { "Sistema de gestión de reportes" }
                @if let Some(e) = error {
                    div.error role="alert" { (e) }
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

pub async fn acceso_pagina() -> Markup {
    pantalla_acceso(None)
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
            pantalla_acceso(Some(&format!(
                "Demasiados intentos. Vuelva a probar en {segundos} s."
            ))),
        )
            .into_response(),
        Err(err) => respuesta_de_error(err, pantalla_acceso),
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
    actor: crate::autz::Actor,
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
                        th { "Ajuste" } th { "Estado" } th { "Archivo" }
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
                            }
                        }
                    }
                }
            }
        }
    })
    .into_response()
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
    actor: crate::autz::Actor,
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

// ── Informes (el catálogo de obligaciones) ────────────────────────────────────

pub async fn informes_pagina(State(e): State<Estado>, actor: crate::autz::Actor) -> Response {
    // `listar_de` lleva `puede_leer` DENTRO, así que el permiso no depende de que
    // yo me acuerde de comprobarlo aquí. Es la forma que evita el fallo que
    // cometí en inventario: catorce rutas con la guardia olvidada.
    let filas = match crate::informes::listar_de(&e, &actor).await {
        Ok(v) => v,
        Err(err) => return error_en_pantalla("Informes", "/informes", err),
    };

    envoltura("Informes", Some("/informes"), html! {
        h2 { "Informes" }
        p.nota { "Las obligaciones definidas: quién las produce, cada cuánto y si están activas." }
        @if filas.is_empty() {
            p.vacio { "No hay informes definidos todavía." }
        } @else {
            div.tabla-ancha {
                table.datos {
                    thead { tr { th { "Nombre" } th { "Descripción" } th { "Periodicidad" } th { "Estado" } } }
                    tbody {
                        @for f in &filas {
                            tr.inactivo[!f.activo] {
                                td { (f.nombre) }
                                td { (f.descripcion.clone().unwrap_or_default()) }
                                // El modo viene ya extraído del JSON por SQL. Si
                                // falta, se DICE que falta en vez de dejar la
                                // celda vacía: una celda en blanco se lee como
                                // «sin periodicidad», que es otra cosa.
                                td { (f.modo.clone().unwrap_or_else(|| "sin regla".into())) }
                                td { @if f.activo { "Activo" } @else { "Inactivo" } }
                            }
                        }
                    }
                }
            }
        }
    })
    .into_response()
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
    let filas = match crate::cuentas::listar_de(&e, &actor).await {
        Ok(v) => v,
        Err(err) => return error_en_pantalla("Cuentas", "/cuentas", err),
    };

    envoltura("Cuentas", Some("/cuentas"), html! {
        h2 { "Cuentas" }
        p.nota {
            "El alta, la baja y el restablecimiento de contraseñas siguen en `/api`. "
            "Esta pantalla es el padrón; las escrituras entran cuando tengan su formulario "
            "y su prueba, no antes."
        }
        div.tabla-ancha {
            table.datos {
                thead { tr {
                    th { "Nombre" } th { "Correo" } th { "Rol" }
                    th { "Segundo factor" } th { "Estado" }
                } }
                tbody {
                    @for c in &filas {
                        tr.inactivo[!c.activo] {
                            td { (c.nombre) }
                            td { (c.email) }
                            td { (c.rol) }
                            // Booleano derivado, no la fecha de enrolamiento: la
                            // administración solo necesita saber si hay algo que
                            // retirar, y cuándo se enroló alguien no es asunto
                            // de nadie más.
                            td { @if c.segundo_factor { "Sí" } @else { "No" } }
                            td { @if c.activo { "Activa" } @else { "Desactivada" } }
                        }
                    }
                }
            }
        }
    })
    .into_response()
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
    })
}

pub async fn mi_clave_pagina() -> Markup {
    pantalla_mi_clave(None, false)
}

pub async fn mi_clave_enviar(
    State(e): State<Estado>,
    actor: crate::autz::Actor,
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
fn pantalla_cronograma(resultado: Option<&crate::planificador::Resumen>, error: Option<&str>) -> Markup {
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
        p.nota {
            "La previsualización de una regla sigue en "
            code { "POST /api/periodicidad/previsualizar" }
            ": su formulario entra con su propia prueba."
        }
    })
}

pub async fn cronograma_pagina() -> Markup {
    pantalla_cronograma(None, None)
}

pub async fn cronograma_generar(State(e): State<Estado>, actor: crate::autz::Actor) -> Response {
    match crate::rutas::generar_de(&e, &actor).await {
        Ok(r) => pantalla_cronograma(Some(&r), None).into_response(),
        Err(crate::rutas::Error(codigo, mensaje)) => {
            (codigo, pantalla_cronograma(None, Some(&mensaje))).into_response()
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// La envoltura sirve los tres archivos LOCALES y ninguno de fuera: los
    /// perfiles `onprem` y `standalone` son sin internet.
    #[test]
    fn la_envoltura_no_apunta_a_ningun_origen_externo() {
        let html = envoltura("t", Some("/entregas"), html! { p { "x" } }).into_string();
        assert!(html.contains(r#"src="/htmx.min.js""#));
        assert!(html.contains(r#"src="/boi.js""#));
        assert!(html.contains(r#"href="/panel.css""#));
        assert!(!html.contains("http://"), "apunta fuera: {html:.300}");
        assert!(!html.contains("https://"), "apunta fuera: {html:.300}");
    }

    /// Sin sesión no se dibuja la barra: enumerar las secciones le dice a quien
    /// no ha entrado qué hay dentro.
    #[test]
    fn las_pantallas_de_acceso_no_llevan_barra_ni_nombran_las_secciones() {
        for html in [
            pantalla_acceso(None).into_string(),
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
            pantalla_acceso(Some("x")).into_string(),
            pantalla_codigo(Some("x")).into_string(),
            pantalla_mi_clave(Some("x"), true).into_string(),
            pantalla_cronograma(None, Some("x")).into_string(),
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
