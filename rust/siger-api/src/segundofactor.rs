//! El segundo factor, cableado: enrolamiento, login en dos pasos y recuperación.
//!
//! La REGLA vive en [`crate::dosfactores`] y no sabe nada de esta base; aquí
//! están las columnas, el cifrado del secreto y los cinco extremos. Es el mismo
//! reparto que el freno (`guaca::freno` decide, `acceso.rs` consulta), y por la
//! misma razón.
//!
//! # El login pasa a tener dos pasos, y el primero ya no abre nada
//!
//! `POST /api/sesion` con la contraseña correcta deja de emitir la cookie de
//! sesión cuando la cuenta tiene segundo factor: emite un token **pendiente**,
//! en otra cookie y firmado en otro dominio, que solo sirve para
//! `POST /api/2fa/verificar`. Si ese token valiera como sesión, el segundo factor
//! sería un cartel; lo sujeta
//! `sesion::tests::un_token_pendiente_no_vale_como_sesion_ni_al_reves`.
//!
//! # El freno cubre los DOS pasos
//!
//! Seis dígitos son un millón de combinaciones, y con tres ventanas admitidas hay
//! tres válidas a la vez. Sin freno, quien ya tiene la contraseña las prueba. Por
//! eso `/api/2fa/verificar` anota en `intento_acceso` igual que el login y
//! consulta el mismo `guaca::freno` ANTES de mirar el código: es la misma puerta,
//! no dos.
//!
//! # Sin frase de reposo no hay segundo factor, y se dice
//!
//! El secreto se guarda cifrado con `guaca::reposo`. Sin `SIGER_CLAVE_REPOSO` no
//! hay dónde guardarlo con sentido —en claro, quien copie el archivo genera los
//! códigos igual que su dueño—, así que el enrolamiento **se niega a empezar** y
//! explica por qué. La alternativa era guardarlo en claro «mientras tanto», que
//! es la directiva 20 al revés: un sistema que parece tener 2FA y no lo tiene.
//!
//! Que esto pueda faltar no rompe la invariante del modo degradado (#71): sin
//! frase, el sistema sigue funcionando entero y **nadie queda fuera**. Lo que no
//! se puede es ACTIVAR un factor nuevo.

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::autz::Actor;
use crate::cuentas::{clave_correcta, con_turno, hashear};
use crate::dosfactores::{self, Veredicto};
use crate::rutas::{auditar, Error, Estado};
use crate::sesion;

/// Cuántos códigos de recuperación se entregan en el enrolamiento.
///
/// Diez: bastantes para no quedarse sin ellos entre dos enrolamientos, pocos para
/// que quepan en un papel. Cada uno es 80 bits, así que la cantidad no es lo que
/// sostiene la seguridad.
const CODIGOS_RECUPERACION: usize = 10;

#[derive(Deserialize)]
pub struct SoloCodigo {
    codigo: String,
}

#[derive(Deserialize)]
pub struct SoloClave {
    clave: String,
}

/// Lo que pide `activar`: el código del teléfono **y** la contraseña.
///
/// Son las dos mitades de una misma afirmación: el código prueba que el
/// autenticador que se está atando es el que tiene delante quien lo ata, y la
/// contraseña prueba que quien lo ata es el dueño de la cuenta y no una sesión
/// robada. Sin la segunda, la primera se cumple igual de bien con el teléfono del
/// ladrón. Ver el porqué en [`activar`].
#[derive(Deserialize)]
pub struct CodigoYClave {
    codigo: String,
    clave: String,
}

#[derive(Serialize)]
pub struct Enrolamiento {
    /// Para teclear a mano cuando la cámara no lee el QR.
    secreto: String,
    /// El URI `otpauth://`, para el enlace «abrir en la aplicación» del teléfono.
    uri: String,
    /// El QR ya dibujado, en SVG.
    ///
    /// Se dibuja **aquí** y no en el navegador: una librería de QR en npm son
    /// veinte kilobytes de JavaScript dentro del paquete inicial para una
    /// pantalla que cada cuenta abre una vez, y una dependencia más que auditar en
    /// un producto que se instala sin internet. Ver [`crate::qr`].
    qr_svg: String,
}

#[derive(Serialize)]
pub struct EstadoFactor {
    activo: bool,
    /// Cuántos códigos de recuperación quedan sin gastar. Es lo que decide si la
    /// pantalla avisa de que hay que generar más.
    codigos_disponibles: i64,
    /// Si el despliegue puede activar el factor. `false` significa que falta la
    /// frase de reposo, y la interfaz lo dice en vez de ofrecer un botón que
    /// fallará.
    puede_activarse: bool,
}

// --- El secreto, que nunca se guarda en claro ---------------------------------

/// Cifra el secreto para la fila. El Argon2id de `guaca::reposo` reserva 64 MiB,
/// así que va con turno y fuera del hilo del runtime, como el resto de la cripto.
async fn cifrar_secreto(e: &Estado, secreto: String) -> Result<Vec<u8>, Error> {
    let Some(frase) = e.cfg.clave_reposo.clone() else {
        return Err(sin_frase());
    };
    con_turno(e, "cifrar el secreto del segundo factor", move || {
        guaca::reposo::cifrar(secreto.as_bytes(), &frase)
    })
    .await
}

/// Devuelve el secreto en claro de esa fila.
///
/// El `Result` interno distingue «no se pudo descifrar» de «no había secreto»: el
/// primero es una avería del despliegue —frase perdida o rotada— y decirle a la
/// persona que su código está mal la dejaría probando contra un servidor roto.
async fn descifrar_secreto(e: &Estado, blob: Vec<u8>) -> Result<Option<String>, Error> {
    let Some(frase) = e.cfg.clave_reposo.clone() else {
        return Ok(None);
    };
    let claro = con_turno(e, "descifrar el secreto del segundo factor", move || {
        guaca::reposo::descifrar(&blob, &frase).ok()
    })
    .await?;
    Ok(claro.and_then(|b| String::from_utf8(b).ok()))
}

fn sin_frase() -> Error {
    Error(
        StatusCode::CONFLICT,
        "este despliegue no tiene SIGER_CLAVE_REPOSO, y sin ella el secreto del \
         segundo factor se guardaría en claro. Configúrela y reinicie."
            .into(),
    )
}

// --- Extremos del enrolamiento -------------------------------------------------

/// `GET /api/2fa` — qué tiene esta cuenta.
pub async fn estado(State(e): State<Estado>, actor: Actor) -> Result<Json<EstadoFactor>, Error> {
    let confirmado: Option<String> =
        sqlx::query_scalar("SELECT totp_confirmado_en FROM usuarios WHERE id = ?")
            .bind(actor.id)
            .fetch_one(&e.bd)
            .await?;
    let disponibles: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM codigo_recuperacion WHERE usuario_id = ? AND usado_en IS NULL",
    )
    .bind(actor.id)
    .fetch_one(&e.bd)
    .await?;
    Ok(Json(EstadoFactor {
        activo: confirmado.is_some(),
        codigos_disponibles: disponibles,
        puede_activarse: e.cfg.clave_reposo.is_some(),
    }))
}

/// `POST /api/2fa/iniciar` — genera el secreto y devuelve el QR. **No activa
/// nada.**
///
/// Entre esto y `activar` hay un paso obligatorio: teclear un código que salga
/// del secreto. Activar aquí dejaría fuera de su cuenta a quien tenga el reloj
/// mal o cierre la pantalla antes de guardar el QR, y sin poder entrar a
/// arreglarlo — que es el fallo que el punto 5 de la ficha #212 describe.
pub async fn iniciar(State(e): State<Estado>, actor: Actor) -> Result<Json<Enrolamiento>, Error> {
    let ya: Option<String> =
        sqlx::query_scalar("SELECT totp_confirmado_en FROM usuarios WHERE id = ?")
            .bind(actor.id)
            .fetch_one(&e.bd)
            .await?;
    if ya.is_some() {
        return Err(Error(
            StatusCode::CONFLICT,
            "esta cuenta ya tiene segundo factor; desactívelo antes de volver a enrolar".into(),
        ));
    }

    let secreto = dosfactores::generar_secreto();
    let cifrado = cifrar_secreto(&e, secreto.clone()).await?;

    // El secreto se guarda SIN confirmar y el contador se limpia: un enrolamiento
    // nuevo no puede heredar el contador del anterior, que pertenece a otro
    // secreto y bloquearía códigos legítimos.
    sqlx::query(
        "UPDATE usuarios SET totp_secreto_cifrado = ?, totp_confirmado_en = NULL, \
         totp_ultimo_contador = NULL WHERE id = ?",
    )
    .bind(&cifrado)
    .bind(actor.id)
    .execute(&e.bd)
    .await?;

    auditar(&e.bd, "2fa_enrolamiento_iniciado", Some(&actor.email), None).await;
    let uri = dosfactores::uri_otpauth("SIGER", &actor.email, &secreto);
    // Un URI de enrolamiento cabe de sobra —la versión 40 admite 2.331 bytes—,
    // así que llegar aquí significa un correo absurdo. Se dice, no se calla: sin
    // QR la pantalla no tiene nada que enseñar.
    let codigo = crate::qr::codificar(uri.as_bytes()).map_err(|e| {
        Error(StatusCode::INTERNAL_SERVER_ERROR, format!("no se pudo dibujar el código QR: {e}"))
    })?;
    Ok(Json(Enrolamiento { qr_svg: codigo.a_svg(), uri, secreto }))
}

/// `POST /api/2fa/activar` — exige un código correcto **y la contraseña actual**,
/// y entrega los códigos de recuperación.
///
/// Los códigos se devuelven **una sola vez**: en la base solo queda su hash. Si
/// se pudieran volver a consultar, serían una segunda contraseña legible por
/// cualquiera que llegue a la sesión.
///
/// # Por qué la contraseña, si aquí no se apaga nada
///
/// `desactivar` la pedía y esto no, y esa asimetría era del revés. Encender el
/// factor no le quita protección a la cuenta, pero **decide de quién es el
/// teléfono que a partir de ahora manda**: con una sesión robada y sin saber
/// ninguna contraseña, se podía atar el autenticador propio a la cuenta ajena.
/// El ladrón no gana acceso —el primer paso del login le sigue pidiendo la
/// contraseña que no tiene— pero deja al dueño fuera de la suya, y la única salida
/// es que un administrador corra el rescate de `DELETE /api/cuentas/{id}/2fa`.
///
/// El código y la contraseña prueban cosas distintas y hacen falta las dos: el
/// código, que el autenticador que se ata está delante de quien lo ata; la
/// contraseña, que quien lo ata es el dueño. La primera se cumple igual de bien
/// con el teléfono del ladrón.
///
/// La contraseña se comprueba **después** de los dos conflictos y **antes** del
/// código, porque los conflictos son gratis y verificar la contraseña cuesta un
/// Argon2id de 64 MiB con turno de semáforo: no se paga por una petición que iba a
/// ser rechazada de todos modos.
///
/// Y no adelantan nada que valga a quien ya tiene la sesión. El primero —«ya
/// estaba activo»— es el mismo bit que `GET /api/2fa` publica en `activo`. El
/// segundo —«no hay enrolamiento en curso»— **sí** dice algo que ese extremo no
/// dice: que hay un secreto sin confirmar en la fila. Pero quien tenga la sesión
/// saca más llamando a `iniciar`, que tampoco pide contraseña y devuelve el secreto
/// entero; así que ese bit no le añade nada. Se anota la diferencia porque una
/// frase redonda de más —«no dicen NADA»— es la que mañana sostiene un veredicto
/// que no le corresponde.
pub async fn activar(
    State(e): State<Estado>,
    actor: Actor,
    Json(p): Json<CodigoYClave>,
) -> Result<Json<Vec<String>>, Error> {
    let fila: (Option<Vec<u8>>, Option<String>, String) = sqlx::query_as(
        "SELECT totp_secreto_cifrado, totp_confirmado_en, clave_hash FROM usuarios WHERE id = ?",
    )
    .bind(actor.id)
    .fetch_one(&e.bd)
    .await?;
    if fila.1.is_some() {
        return Err(Error(StatusCode::CONFLICT, "ya estaba activo".into()));
    }
    let Some(blob) = fila.0 else {
        return Err(Error(
            StatusCode::CONFLICT,
            "no hay enrolamiento en curso: pida primero el código QR".into(),
        ));
    };
    if !clave_correcta(&e, fila.2, p.clave).await? {
        auditar(&e.bd, "2fa_activacion_rechazada", Some(&actor.email), None).await;
        return Err(Error(StatusCode::UNAUTHORIZED, "la contraseña actual no es correcta".into()));
    }
    let Some(secreto) = descifrar_secreto(&e, blob).await? else {
        return Err(sin_frase());
    };

    let contador = match dosfactores::verificar(&secreto, &p.codigo, ahora_unix(), None) {
        Veredicto::Correcto { contador } => contador,
        Veredicto::SecretoIlegible => return Err(secreto_ilegible(&e, &actor).await),
        // En el enrolamiento no hay reutilización posible —no se ha usado
        // ninguno—, pero el caso se nombra igual: un `_ =>` aquí se tragaría el
        // estado nuevo que alguien añada al enum mañana.
        Veredicto::Reutilizado | Veredicto::Incorrecto => {
            return Err(Error(
                StatusCode::BAD_REQUEST,
                "ese código no corresponde. Compruebe la hora del teléfono.".into(),
            ))
        }
    };

    let ahora = chrono::Utc::now().to_rfc3339();
    let codigos = dosfactores::generar_codigos_recuperacion(CODIGOS_RECUPERACION);
    let hashes = hashear_codigos(&e, &codigos).await?;

    // Todo junto: activar el factor y dejar sus códigos de recuperación. Si se
    // hicieran en dos pasos y el segundo fallara, la cuenta quedaría con segundo
    // factor y sin ninguna salida — exactamente el bloqueo que el punto 4 de la
    // ficha manda evitar.
    let mut tx = e.bd.begin().await?;
    sqlx::query("UPDATE usuarios SET totp_confirmado_en = ?, totp_ultimo_contador = ? WHERE id = ?")
        .bind(&ahora)
        .bind(contador as i64)
        .bind(actor.id)
        .execute(&mut *tx)
        .await?;
    reemplazar_codigos(&mut tx, actor.id, &hashes, &ahora).await?;
    tx.commit().await?;

    auditar(&e.bd, "2fa_activado", Some(&actor.email), None).await;
    Ok(Json(codigos))
}

/// `POST /api/2fa/codigos` — diez códigos de recuperación nuevos, sin tocar el
/// secreto. Exige la contraseña actual.
///
/// # Por qué existe
///
/// Sin esto, la única forma de reponerlos era **desactivar y volver a enrolar**:
/// secreto nuevo, QR nuevo en todos los teléfonos, y —lo que de verdad importa—
/// un rato con la cuenta protegida por la contraseña sola, que es justo el estado
/// del que el segundo factor viene a sacarla. Quedarse sin códigos es lo normal:
/// se gastan de uno en uno y cada uno se usa el día que el teléfono no está.
///
/// # Por qué la contraseña
///
/// Mismo criterio que `desactivar`: esto entrega diez credenciales que entran en
/// lugar del segundo factor. Una sesión robada que pueda pedirlas se lleva la
/// cuenta entera aunque el teléfono siga en el bolsillo de su dueño.
pub async fn renovar_codigos(
    State(e): State<Estado>,
    actor: Actor,
    Json(p): Json<SoloClave>,
) -> Result<Json<Vec<String>>, Error> {
    let (hash, confirmado): (String, Option<String>) =
        sqlx::query_as("SELECT clave_hash, totp_confirmado_en FROM usuarios WHERE id = ?")
            .bind(actor.id)
            .fetch_one(&e.bd)
            .await?;
    // Sin factor activo no hay nada que respaldar: entregar códigos que entran en
    // lugar de un segundo factor que no existe sería una segunda contraseña, más
    // corta y en papel.
    if confirmado.is_none() {
        return Err(Error(
            StatusCode::CONFLICT,
            "esta cuenta no tiene segundo factor activo".into(),
        ));
    }
    if !clave_correcta(&e, hash, p.clave).await? {
        auditar(&e.bd, "2fa_codigos_rechazados", Some(&actor.email), None).await;
        return Err(Error(StatusCode::UNAUTHORIZED, "la contraseña actual no es correcta".into()));
    }

    let ahora = chrono::Utc::now().to_rfc3339();
    let codigos = dosfactores::generar_codigos_recuperacion(CODIGOS_RECUPERACION);
    let hashes = hashear_codigos(&e, &codigos).await?;

    // Los viejos se van en la MISMA transacción en que entran los nuevos. En dos
    // pasos, un fallo en medio deja a la persona sin ninguno — y con el teléfono
    // perdido eso es quedarse fuera de la cuenta.
    let mut tx = e.bd.begin().await?;
    reemplazar_codigos(&mut tx, actor.id, &hashes, &ahora).await?;
    tx.commit().await?;

    auditar(&e.bd, "2fa_codigos_renovados", Some(&actor.email), None).await;
    Ok(Json(codigos))
}

/// El Argon2 de cada código, FUERA de la transacción.
///
/// Diez hashes son diez veces 64 MiB con su turno de semáforo: hacerlos con la
/// transacción abierta la mantiene viva varios segundos y bloquea a los demás
/// escritores de SQLite, que es de un solo escritor.
async fn hashear_codigos(e: &Estado, codigos: &[String]) -> Result<Vec<String>, Error> {
    let mut hashes = Vec::with_capacity(codigos.len());
    for codigo in codigos {
        hashes.push(hashear(e, codigo.clone()).await?);
    }
    Ok(hashes)
}

/// Borra los códigos de esa cuenta y deja los nuevos. Va dentro de la transacción
/// de quien llama, para que el borrado y el alta no puedan quedar a medias.
async fn reemplazar_codigos(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    usuario_id: i64,
    hashes: &[String],
    ahora: &str,
) -> Result<(), Error> {
    sqlx::query("DELETE FROM codigo_recuperacion WHERE usuario_id = ?")
        .bind(usuario_id)
        .execute(&mut **tx)
        .await?;
    for hash in hashes {
        sqlx::query("INSERT INTO codigo_recuperacion (usuario_id, hash, creado_en) VALUES (?, ?, ?)")
            .bind(usuario_id)
            .bind(hash)
            .bind(ahora)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// `POST /api/2fa/desactivar` — exige la contraseña actual.
///
/// Sin ella, una sesión robada apagaría el segundo factor y volvería a dejar la
/// cuenta en una sola credencial: justo la que el ladrón ya tiene.
pub async fn desactivar(
    State(e): State<Estado>,
    actor: Actor,
    Json(p): Json<SoloClave>,
) -> Result<StatusCode, Error> {
    let hash: String = sqlx::query_scalar("SELECT clave_hash FROM usuarios WHERE id = ?")
        .bind(actor.id)
        .fetch_one(&e.bd)
        .await?;
    if !clave_correcta(&e, hash, p.clave).await? {
        auditar(&e.bd, "2fa_desactivacion_rechazada", Some(&actor.email), None).await;
        return Err(Error(StatusCode::UNAUTHORIZED, "la contraseña actual no es correcta".into()));
    }

    let mut tx = e.bd.begin().await?;
    sqlx::query(
        "UPDATE usuarios SET totp_secreto_cifrado = NULL, totp_confirmado_en = NULL, \
         totp_ultimo_contador = NULL WHERE id = ?",
    )
    .bind(actor.id)
    .execute(&mut *tx)
    .await?;
    // Los códigos de recuperación se van CON el factor. Dejarlos sería guardar
    // llaves de una cerradura que ya no existe, listas para el día que se vuelva
    // a enrolar.
    sqlx::query("DELETE FROM codigo_recuperacion WHERE usuario_id = ?")
        .bind(actor.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    auditar(&e.bd, "2fa_desactivado", Some(&actor.email), None).await;
    Ok(StatusCode::NO_CONTENT)
}

// --- El segundo paso del login -------------------------------------------------

/// `POST /api/2fa/verificar` — el código, con el token pendiente en la cookie.
///
/// Admite el código de seis dígitos **o** uno de recuperación. No se le pregunta
/// al cliente cuál manda: el formato los distingue y preguntarlo solo añadiría un
/// campo que se puede mentir.
pub async fn verificar(
    State(e): State<Estado>,
    cabeceras: HeaderMap,
    crate::origen::OrigenCliente(origen): crate::origen::OrigenCliente,
    Json(p): Json<SoloCodigo>,
) -> Result<Response, Error> {
    let rechazo = || Error(StatusCode::UNAUTHORIZED, "código incorrecto".into());

    let Some(cookie) = sesion::pendiente_de_cabeceras(&cabeceras) else {
        return Err(rechazo());
    };
    let Some(pendiente) = sesion::verificar_pendiente(&e.cfg.secreto_sesion, &cookie) else {
        return Err(rechazo());
    };

    let Some(origen) = origen else {
        // El mismo criterio que el login: sin origen no se puede frenar, y un
        // segundo factor sin freno es un millón de intentos.
        tracing::error!("segundo factor sin origen: revise SIGER_TRAS_PROXY y X-Real-IP");
        return Err(Error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no se pudo determinar el origen de la petición".into(),
        ));
    };

    /// Lo que hace falta saber del usuario para el segundo paso: quién es, si
    /// sigue valiendo, y su secreto con el último contador gastado.
    type FilaFactor = (String, i64, i64, Option<Vec<u8>>, Option<String>, Option<i64>);
    let (email, activo, version, blob, confirmado, ultimo) = sqlx::query_as::<_, FilaFactor>(
        "SELECT email, activo, token_version, totp_secreto_cifrado, totp_confirmado_en, \
         totp_ultimo_contador FROM usuarios WHERE id = ?",
    )
    .bind(pendiente.usuario_id)
    .fetch_optional(&e.bd)
    .await?
    .ok_or_else(rechazo)?;

    // Lo que valía cuando se emitió el pendiente puede haber dejado de valer en
    // esos cinco minutos: la cuenta se desactiva, o alguien cierra todas las
    // sesiones. Se recomprueba aquí y no se hereda del token.
    if activo == 0 || version != pendiente.version || confirmado.is_none() {
        return Err(rechazo());
    }

    let ahora = chrono::Utc::now();
    let freno = crate::acceso::estado(&e.bd, &email, &origen, ahora).await?;
    if freno.frenado() {
        auditar(&e.bd, "2fa_frenado", Some(&email), None).await;
        let mut cabeceras = HeaderMap::new();
        cabeceras.insert(
            header::RETRY_AFTER,
            freno.segundos_restantes().to_string().parse().expect("un número es ASCII"),
        );
        return Ok((StatusCode::TOO_MANY_REQUESTS, cabeceras, "demasiados intentos").into_response());
    }

    let Some(blob) = blob else { return Err(rechazo()) };
    let Some(secreto) = descifrar_secreto(&e, blob).await? else {
        return Err(Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "el servidor no puede leer el segundo factor ahora mismo".into(),
        ));
    };

    let ultimo = ultimo.map(|v| v as u64);
    let veredicto = dosfactores::verificar(&secreto, &p.codigo, ahora.timestamp() as u64, ultimo);

    let contador_nuevo = match veredicto {
        Veredicto::Correcto { contador } => Some(contador),
        Veredicto::SecretoIlegible => {
            // Una avería del servidor no se cobra al usuario: ni cuenta como
            // fallo para el freno ni le dice que su código está mal.
            tracing::error!("usuario {}: el secreto TOTP guardado no es legible", pendiente.usuario_id);
            auditar(&e.bd, "2fa_secreto_ilegible", Some(&email), None).await;
            return Err(Error(
                StatusCode::SERVICE_UNAVAILABLE,
                "el servidor no puede comprobar el segundo factor ahora mismo".into(),
            ));
        }
        Veredicto::Reutilizado | Veredicto::Incorrecto => {
            // Un código de recuperación es la otra puerta legítima, y se prueba
            // aquí y no antes: el camino normal es el de seis dígitos.
            if !gastar_codigo_recuperacion(&e, pendiente.usuario_id, &p.codigo).await? {
                crate::acceso::anotar(&e.bd, &email, &origen, false, ahora).await?;
                auditar(
                    &e.bd,
                    if matches!(veredicto, Veredicto::Reutilizado) {
                        // Un código bueno que ya se gastó no es un dedazo: es un
                        // código repetido, y merece su propia línea en la
                        // bitácora aunque la respuesta al cliente sea idéntica.
                        "2fa_codigo_reutilizado"
                    } else {
                        "2fa_fallido"
                    },
                    Some(&email),
                    None,
                )
                .await;
                return Err(rechazo());
            }
            auditar(&e.bd, "2fa_recuperacion_usada", Some(&email), None).await;
            None
        }
    };

    // El contador se quema en el MISMO `UPDATE` que comprueba que nadie lo quemó
    // antes, igual que `usado_en IS NULL` en los códigos de recuperación. En dos
    // pasos —leer arriba, escribir aquí— hay un hueco de cientos de milisegundos:
    // entre la lectura y esta línea pasan el turno del semáforo y un Argon2id de
    // 64 MiB descifrando el secreto. Dos peticiones con el MISMO código que
    // entren a la vez leen las dos el contador viejo y abren las dos, que es
    // exactamente lo que este campo existe para impedir.
    //
    // Cero filas significa que otra petición ya avanzó el contador hasta aquí o
    // más allá: el código está gastado, y esta es la que llegó tarde. No es un
    // caso raro de laboratorio pero tampoco cuesta nada cerrarlo, y sin esto la
    // frase «el contador guardado es TODO el antirreplay» no era cierta del todo.
    if let Some(c) = contador_nuevo {
        let filas = sqlx::query(
            "UPDATE usuarios SET totp_ultimo_contador = ? \
             WHERE id = ? AND (totp_ultimo_contador IS NULL OR totp_ultimo_contador < ?)",
        )
        .bind(c as i64)
        .bind(pendiente.usuario_id)
        .bind(c as i64)
        .execute(&e.bd)
        .await?
        .rows_affected();
        if filas != 1 {
            crate::acceso::anotar(&e.bd, &email, &origen, false, ahora).await?;
            auditar(&e.bd, "2fa_codigo_reutilizado", Some(&email), None).await;
            return Err(rechazo());
        }
    }
    crate::acceso::anotar(&e.bd, &email, &origen, true, ahora).await?;
    auditar(&e.bd, "acceso", Some(&email), None).await;

    let valor = sesion::emitir(&e.cfg.secreto_sesion, pendiente.usuario_id, version, e.cfg.minutos_sesion);
    let mut salida = HeaderMap::new();
    salida.append(
        header::SET_COOKIE,
        sesion::cabecera_abrir(&valor, e.cfg.minutos_sesion).parse().expect("ASCII"),
    );
    // El pendiente se quema al usarse. Si sobreviviera, valdría para un segundo
    // intento con otro código.
    salida.append(
        header::SET_COOKIE,
        sesion::cabecera_pendiente_cerrar().parse().expect("ASCII"),
    );
    Ok((salida, Json(serde_json::json!({ "ok": true }))).into_response())
}

/// Consume un código de recuperación si alguno casa. `true` = se gastó uno.
///
/// Se comparan todos los vigentes y **se sigue comparando aunque uno case**: salir
/// en el primer acierto haría que el tiempo de respuesta dijera cuántos códigos
/// quedan sin gastar.
async fn gastar_codigo_recuperacion(e: &Estado, usuario_id: i64, tecleado: &str) -> Result<bool, Error> {
    let vigentes: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, hash FROM codigo_recuperacion WHERE usuario_id = ? AND usado_en IS NULL",
    )
    .bind(usuario_id)
    .fetch_all(&e.bd)
    .await?;

    // Normalizado como se entrega: mayúsculas y con el guion. Quien lo copia de
    // un papel escribe minúsculas la mitad de las veces.
    let tecleado = tecleado.trim().to_uppercase();

    let mut cual = None;
    for (id, hash) in vigentes {
        if clave_correcta(e, hash, tecleado.clone()).await? {
            cual = Some(id);
        }
    }
    let Some(id) = cual else { return Ok(false) };

    // El `usado_en IS NULL` en el UPDATE es lo que impide que dos peticiones
    // simultáneas gasten el mismo código dos veces: gana una, la otra afecta a
    // cero filas.
    let filas = sqlx::query("UPDATE codigo_recuperacion SET usado_en = ? WHERE id = ? AND usado_en IS NULL")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(id)
        .execute(&e.bd)
        .await?
        .rows_affected();
    Ok(filas == 1)
}

async fn secreto_ilegible(e: &Estado, actor: &Actor) -> Error {
    tracing::error!("usuario {}: el secreto TOTP guardado no es legible", actor.id);
    auditar(&e.bd, "2fa_secreto_ilegible", Some(&actor.email), None).await;
    Error(
        StatusCode::SERVICE_UNAVAILABLE,
        "el servidor no puede comprobar el segundo factor ahora mismo".into(),
    )
}

fn ahora_unix() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use serde_json::Value;
    use tower::ServiceExt;

    const CLAVE: &str = "la-clave-de-la-prueba";
    const FRASE: &str = "frase-de-reposo-del-despliegue-larga-y-propia";

    /// Un servidor con un usuario real. `con_frase` decide si el despliegue puede
    /// guardar el secreto cifrado, que es lo único que habilita el 2FA.
    async fn servidor(con_frase: bool) -> (axum::Router, Estado) {
        use argon2::password_hash::{PasswordHasher, SaltString};
        let bd = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&bd).await.unwrap();
        let sal = SaltString::from_b64("c2FsZGVsYXNwcnVlYmFzZGVsMmZhYQ").unwrap();
        let hash = argon2::Argon2::default()
            .hash_password(CLAVE.as_bytes(), &sal)
            .unwrap()
            .to_string();
        sqlx::query(
            "INSERT INTO usuarios (id,nombre,email,clave_hash,rol,activo) \
             VALUES (1,'Ada','ada@x.co',?,'ADMIN',1)",
        )
        .bind(&hash)
        .execute(&bd)
        .await
        .unwrap();

        let mut cfg = crate::config::Config::para_pruebas();
        cfg.clave_reposo = con_frase.then(|| FRASE.to_string());
        let estado = Estado {
            bd,
            cfg,
            motor: std::sync::Arc::new(siger_core::Motor::nuevo(
                siger_core::Calendario::de_pais("CO", 2024, 2030).unwrap(),
            )),
        };
        (crate::rutas::router(estado.clone()), estado)
    }

    fn peticion(metodo: &str, uri: &str, cookie: Option<&str>, cuerpo: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method(metodo).uri(uri).extension(
            axum::extract::ConnectInfo("127.0.0.1:5555".parse::<std::net::SocketAddr>().unwrap()),
        );
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, c);
        }
        match cuerpo {
            Some(j) => b
                .header("content-type", "application/json")
                .body(Body::from(j.to_string()))
                .unwrap(),
            None => b.body(Body::empty()).unwrap(),
        }
    }

    async fn llamar(app: &axum::Router, r: Request<Body>) -> (StatusCode, Value) {
        let res = app.clone().oneshot(r).await.unwrap();
        let estado = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
        (estado, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    /// Las cookies que trae una respuesta, por nombre.
    async fn cookies_de(app: &axum::Router, r: Request<Body>) -> (StatusCode, Value, Vec<String>) {
        let res = app.clone().oneshot(r).await.unwrap();
        let estado = res.status();
        let cookies: Vec<String> = res
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap().to_string())
            .collect();
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
        (estado, serde_json::from_slice(&bytes).unwrap_or(Value::Null), cookies)
    }

    fn valor_de(cookies: &[String], nombre: &str) -> Option<String> {
        cookies
            .iter()
            .find(|c| c.starts_with(&format!("{nombre}=")) && !c.contains("Max-Age=0"))
            .map(|c| c.split(';').next().unwrap().to_string())
    }

    async fn login(app: &axum::Router) -> (StatusCode, Value, Vec<String>) {
        cookies_de(
            app,
            peticion(
                "POST",
                "/api/sesion",
                None,
                Some(&format!(r#"{{"email":"ada@x.co","clave":"{CLAVE}"}}"#)),
            ),
        )
        .await
    }

    /// El código que generaría el teléfono en ese instante.
    fn codigo(secreto: &str, cuando: u64) -> String {
        let bytes = totp_rs::Secret::Encoded(secreto.to_string()).to_bytes().unwrap();
        totp_rs::TOTP::new(totp_rs::Algorithm::SHA1, 6, 1, 30, bytes).unwrap().generate(cuando)
    }

    fn ahora() -> u64 {
        chrono::Utc::now().timestamp() as u64
    }

    /// Borra los códigos de recuperación de la cuenta 1.
    ///
    /// No es un atajo cosmético: `gastar_codigo_recuperacion` compara **todos**
    /// los vigentes sin salir en el primer acierto, así que cada código erróneo
    /// paga once Argon2id de 64 MiB —uno del secreto y diez de los códigos— y una
    /// prueba de seis intentos se va a un minuto. Las pruebas del FRENO no
    /// afirman nada sobre la recuperación; las que sí lo hacen no llaman a esto.
    async fn sin_codigos_de_recuperacion(e: &Estado) {
        sqlx::query("DELETE FROM codigo_recuperacion").execute(&e.bd).await.unwrap();
    }

    /// Deja la cuenta 1 con segundo factor activo. Devuelve el secreto.
    async fn enrolar(app: &axum::Router, sesion: &str) -> String {
        enrolar_con_codigos(app, sesion).await.0
    }

    /// Igual, pero devuelve también los diez códigos de recuperación que se
    /// entregaron. Solo lo necesita quien afirme algo sobre ellos.
    async fn enrolar_con_codigos(app: &axum::Router, sesion: &str) -> (String, Vec<String>) {
        let (s, cuerpo) = llamar(app, peticion("POST", "/api/2fa/iniciar", Some(sesion), None)).await;
        assert_eq!(s, StatusCode::OK, "{cuerpo}");
        let secreto = cuerpo["secreto"].as_str().unwrap().to_string();
        let c = codigo(&secreto, ahora());
        let (s, cuerpo) = llamar(
            app,
            peticion(
                "POST",
                "/api/2fa/activar",
                Some(sesion),
                Some(&format!(r#"{{"codigo":"{c}","clave":"{CLAVE}"}}"#)),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{cuerpo}");
        let codigos = cuerpo
            .as_array()
            .expect("activar entrega los códigos")
            .iter()
            .map(|c| c.as_str().unwrap().to_string())
            .collect();
        (secreto, codigos)
    }

    /// Lo que llega a la pantalla de enrolamiento es un QR, y el QR es el del URI
    /// que va en la misma respuesta.
    ///
    /// Suena obvio y es justo el empalme donde se pierde: dibujar el secreto en
    /// vez del URI, o el URI de un enrolamiento anterior, produce un código que se
    /// escanea sin protestar y genera números que nunca casan. Y no se comprueba
    /// leyendo el campo `secreto` —eso sería creerle a la respuesta— sino sacando
    /// el secreto DEL URI, que es lo único que el teléfono llega a ver, y
    /// activando con un código generado a partir de él.
    #[tokio::test]
    async fn el_qr_lleva_el_uri_y_el_uri_lleva_el_secreto_que_luego_vale() {
        let (app, _) = servidor(true).await;
        let (_, _, cookies) = login(&app).await;
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).unwrap();

        let (s, cuerpo) =
            llamar(&app, peticion("POST", "/api/2fa/iniciar", Some(&sesion), None)).await;
        assert_eq!(s, StatusCode::OK, "{cuerpo}");
        let uri = cuerpo["uri"].as_str().expect("el URI viaja en la respuesta");
        let svg = cuerpo["qr_svg"].as_str().expect("el QR viaja dibujado");

        assert!(svg.starts_with("<svg "), "eso no es un SVG: {}", &svg[..svg.len().min(40)]);
        assert_eq!(
            svg,
            crate::qr::codificar(uri.as_bytes()).unwrap().a_svg(),
            "el QR dibujado no es el de este URI"
        );

        // Del URI al código, que es el camino del teléfono.
        let del_uri = uri
            .split("secret=")
            .nth(1)
            .and_then(|r| r.split('&').next())
            .expect("el URI lleva su secreto");
        let c = codigo(del_uri, ahora());
        let (s, cuerpo) = llamar(
            &app,
            peticion(
                "POST",
                "/api/2fa/activar",
                Some(&sesion),
                Some(&format!(r#"{{"codigo":"{c}","clave":"{CLAVE}"}}"#)),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "el código que sale del QR no activó: {cuerpo}");
    }

    #[tokio::test]
    async fn sin_segundo_factor_el_login_abre_sesion_como_siempre() {
        // La mitad que evita que el resto pase por la razón equivocada: lo que ya
        // funcionaba tiene que seguir funcionando igual.
        let (app, _) = servidor(true).await;
        let (s, cuerpo, cookies) = login(&app).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(cuerpo["segundo_factor"], serde_json::json!(false));
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).expect("cookie de sesión");
        let (s, _) = llamar(&app, peticion("GET", "/api/yo", Some(&sesion), None)).await;
        assert_eq!(s, StatusCode::OK);
    }

    #[tokio::test]
    async fn con_segundo_factor_la_contraseña_sola_no_abre_nada() {
        // **La prueba del asunto.** Si esto se pone verde por accidente —porque el
        // login siguiera emitiendo la cookie de sesión— el segundo factor sería un
        // cartel: quien tenga la contraseña entra igual.
        let (app, _) = servidor(true).await;
        let (_, _, cookies) = login(&app).await;
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).unwrap();
        enrolar(&app, &sesion).await;

        let (s, cuerpo, cookies) = login(&app).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(cuerpo["segundo_factor"], serde_json::json!(true));
        assert!(
            valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).is_none(),
            "el login entregó cookie de SESIÓN con el segundo factor puesto: {cookies:?}"
        );
        let pendiente =
            valor_de(&cookies, crate::sesion::NOMBRE_COOKIE_PENDIENTE).expect("cookie pendiente");

        // Y el pendiente, disfrazado de cookie de sesión, tampoco entra: su firma
        // es de otro dominio.
        let disfrazado = pendiente.replace(
            crate::sesion::NOMBRE_COOKIE_PENDIENTE,
            crate::sesion::NOMBRE_COOKIE,
        );
        let (s, _) = llamar(&app, peticion("GET", "/api/yo", Some(&disfrazado), None)).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "el token pendiente pasó por sesión");
    }

    #[tokio::test]
    async fn el_codigo_correcto_completa_el_login_y_no_sirve_dos_veces() {
        let (app, _) = servidor(true).await;
        let (_, _, cookies) = login(&app).await;
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).unwrap();
        let secreto = enrolar(&app, &sesion).await;

        let (_, _, cookies) = login(&app).await;
        let pendiente = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE_PENDIENTE).unwrap();

        // El código de la ventana SIGUIENTE: vale por tolerancia y su contador
        // avanza respecto del que se gastó al activar. Es también lo que pasa en
        // la vida real, donde entre enrolar y volver a entrar pasa más de medio
        // minuto.
        let c = codigo(&secreto, ahora() + 30);
        let (s, _, cookies2) = cookies_de(
            &app,
            peticion("POST", "/api/2fa/verificar", Some(&pendiente), Some(&format!(r#"{{"codigo":"{c}"}}"#))),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        let sesion2 = valor_de(&cookies2, crate::sesion::NOMBRE_COOKIE).expect("ahora sí, sesión");
        let (s, _) = llamar(&app, peticion("GET", "/api/yo", Some(&sesion2), None)).await;
        assert_eq!(s, StatusCode::OK);

        // El pendiente se quema al usarse: viene una cabecera que lo borra.
        assert!(
            cookies2.iter().any(|c| c.starts_with(crate::sesion::NOMBRE_COOKIE_PENDIENTE) && c.contains("Max-Age=0")),
            "el token pendiente sobrevivió a su uso: {cookies2:?}"
        );

        // Y el MISMO código, en otro login, ya no vale. Sin esto, quien vea el
        // código por encima del hombro entra durante minuto y medio.
        let (_, _, cookies) = login(&app).await;
        let pendiente2 = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE_PENDIENTE).unwrap();
        let (s, _) = llamar(
            &app,
            peticion("POST", "/api/2fa/verificar", Some(&pendiente2), Some(&format!(r#"{{"codigo":"{c}"}}"#))),
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "un código gastado volvió a servir");
    }

    /// El mismo código, tres veces A LA VEZ, abre UNA.
    ///
    /// El caso de arriba prueba el reintento en fila —el segundo llega cuando el
    /// primero ya escribió— y lo caza `Veredicto::Reutilizado` sin ayuda de la
    /// base. Este prueba el otro: tres peticiones que LEEN el contador antes de
    /// que ninguna lo escriba. Entre la lectura y la escritura pasan el turno del
    /// semáforo de cripto y un Argon2id de 64 MiB descifrando el secreto, así que
    /// el hueco son cientos de milisegundos y no un instante teórico.
    ///
    /// El token pendiente no ayuda a cerrarlo: es una carga firmada, sin estado
    /// en el servidor, y vale sus cinco minutos las veces que se presente. Lo
    /// único que quema el código es el contador.
    #[tokio::test]
    async fn el_mismo_codigo_a_la_vez_abre_una_sola_sesion() {
        let (app, _) = servidor(true).await;
        let (_, _, cookies) = login(&app).await;
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).unwrap();
        let secreto = enrolar(&app, &sesion).await;

        let (_, _, cookies) = login(&app).await;
        let pendiente = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE_PENDIENTE).unwrap();

        let c = codigo(&secreto, ahora() + 30);
        let cuerpo = format!(r#"{{"codigo":"{c}"}}"#);
        let una = || peticion("POST", "/api/2fa/verificar", Some(&pendiente), Some(&cuerpo));
        let (a, b, d) = tokio::join!(
            llamar(&app, una()),
            llamar(&app, una()),
            llamar(&app, una()),
        );

        let abiertas = [a.0, b.0, d.0].iter().filter(|s| **s == StatusCode::OK).count();
        assert_eq!(
            abiertas, 1,
            "el mismo código abrió {abiertas} sesiones a la vez: {:?}",
            [a.0, b.0, d.0]
        );
    }

    #[tokio::test]
    async fn activar_exige_un_codigo_correcto_para_no_dejar_a_nadie_fuera() {
        // El punto 5 de #212: si el enrolamiento activara sin comprobar, quien
        // tenga el reloj mal se queda fuera y ya no puede entrar a arreglarlo.
        let (app, e) = servidor(true).await;
        let (_, _, cookies) = login(&app).await;
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).unwrap();

        let (s, cuerpo) = llamar(&app, peticion("POST", "/api/2fa/iniciar", Some(&sesion), None)).await;
        assert_eq!(s, StatusCode::OK);
        assert!(cuerpo["uri"].as_str().unwrap().starts_with("otpauth://totp/SIGER:"));

        let (s, _) = llamar(
            &app,
            peticion(
                "POST",
                "/api/2fa/activar",
                Some(&sesion),
                // La contraseña VA, y es la buena: si faltara, el 400 podría venir
                // de la contraseña y esta prueba dejaría de decir nada del código.
                Some(&format!(r#"{{"codigo":"000000","clave":"{CLAVE}"}}"#)),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);

        // Y sigue SIN activar: el login todavía abre sesión sola.
        let confirmado: Option<String> =
            sqlx::query_scalar("SELECT totp_confirmado_en FROM usuarios WHERE id = 1")
                .fetch_one(&e.bd)
                .await
                .unwrap();
        assert!(confirmado.is_none(), "un código malo activó el factor");
        let (_, cuerpo, _) = login(&app).await;
        assert_eq!(cuerpo["segundo_factor"], serde_json::json!(false));
    }

    #[tokio::test]
    async fn sin_frase_de_reposo_no_se_enrola_y_se_explica_por_que() {
        // Directiva 20: no se guarda el secreto en claro «mientras tanto». Un 2FA
        // con el secreto legible junto a la base no es un segundo factor frente a
        // quien se llevó el archivo, que es el único atacante que importa aquí.
        let (app, e) = servidor(false).await;
        let (_, _, cookies) = login(&app).await;
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).unwrap();

        let (s, cuerpo) = llamar(&app, peticion("POST", "/api/2fa/iniciar", Some(&sesion), None)).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert!(cuerpo.to_string().contains("SIGER_CLAVE_REPOSO"), "{cuerpo}");

        // Y no quedó nada a medias en la fila.
        let blob: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT totp_secreto_cifrado FROM usuarios WHERE id = 1")
                .fetch_one(&e.bd)
                .await
                .unwrap();
        assert!(blob.is_none(), "se guardó un secreto sin poder cifrarlo");

        // El estado lo dice antes de que nadie pulse el botón.
        let (_, cuerpo) = llamar(&app, peticion("GET", "/api/2fa", Some(&sesion), None)).await;
        assert_eq!(cuerpo["puede_activarse"], serde_json::json!(false));
    }

    #[tokio::test]
    async fn un_codigo_de_recuperacion_entra_una_vez_y_se_gasta() {
        // El punto 4 de #212: que nadie quede fuera por perder el teléfono.
        let (app, _) = servidor(true).await;
        let (_, _, cookies) = login(&app).await;
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).unwrap();

        let (s, cuerpo) = llamar(&app, peticion("POST", "/api/2fa/iniciar", Some(&sesion), None)).await;
        assert_eq!(s, StatusCode::OK);
        let secreto = cuerpo["secreto"].as_str().unwrap().to_string();
        let c = codigo(&secreto, ahora());
        let (s, cuerpo) = llamar(
            &app,
            peticion(
                "POST",
                "/api/2fa/activar",
                Some(&sesion),
                Some(&format!(r#"{{"codigo":"{c}","clave":"{CLAVE}"}}"#)),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        let codigos: Vec<String> =
            cuerpo.as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert_eq!(codigos.len(), CODIGOS_RECUPERACION);

        let uno = codigos[0].clone();
        let (_, _, cookies) = login(&app).await;
        let pendiente = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE_PENDIENTE).unwrap();
        let (s, _, cookies2) = cookies_de(
            &app,
            peticion("POST", "/api/2fa/verificar", Some(&pendiente), Some(&format!(r#"{{"codigo":"{uno}"}}"#))),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "un código de recuperación no dejó entrar");
        assert!(valor_de(&cookies2, crate::sesion::NOMBRE_COOKIE).is_some());

        // Y una sola vez: es la diferencia entre una salida de emergencia y una
        // segunda contraseña permanente.
        let (_, _, cookies) = login(&app).await;
        let pendiente2 = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE_PENDIENTE).unwrap();
        let (s, _) = llamar(
            &app,
            peticion("POST", "/api/2fa/verificar", Some(&pendiente2), Some(&format!(r#"{{"codigo":"{uno}"}}"#))),
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "el código de recuperación se pudo reutilizar");
    }

    #[tokio::test]
    async fn desactivar_exige_la_contraseña_actual() {
        // Sin esto, una sesión robada apaga el segundo factor y devuelve la cuenta
        // a una sola credencial: la que el ladrón ya tiene.
        let (app, e) = servidor(true).await;
        let (_, _, cookies) = login(&app).await;
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).unwrap();
        enrolar(&app, &sesion).await;

        let (s, _) = llamar(
            &app,
            peticion("POST", "/api/2fa/desactivar", Some(&sesion), Some(r#"{"clave":"no-es-la-mia"}"#)),
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let confirmado: Option<String> =
            sqlx::query_scalar("SELECT totp_confirmado_en FROM usuarios WHERE id = 1")
                .fetch_one(&e.bd)
                .await
                .unwrap();
        assert!(confirmado.is_some(), "se desactivó sin la contraseña");

        let (s, _) = llamar(
            &app,
            peticion("POST", "/api/2fa/desactivar", Some(&sesion), Some(&format!(r#"{{"clave":"{CLAVE}"}}"#))),
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);

        // Los códigos de recuperación se van con el factor: llaves de una
        // cerradura que ya no existe.
        let quedan: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM codigo_recuperacion WHERE usuario_id = 1")
            .fetch_one(&e.bd)
            .await
            .unwrap();
        assert_eq!(quedan, 0);
    }

    #[tokio::test]
    async fn activar_exige_la_contraseña_para_que_nadie_ate_su_telefono_a_una_cuenta_ajena() {
        // La pareja de `desactivar_exige_la_contraseña_actual`, que faltaba y era
        // la asimetría del revés: apagar el factor pedía la contraseña y
        // ENCENDERLO no. Con una sesión robada y sin saber ninguna contraseña se
        // podía atar el autenticador propio a la cuenta ajena. No da acceso —el
        // primer paso del login sigue pidiendo la contraseña— pero deja al dueño
        // fuera de la suya, y la única salida es el rescate del administrador.
        let (app, e) = servidor(true).await;
        let (_, _, cookies) = login(&app).await;
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).unwrap();

        let (s, cuerpo) = llamar(&app, peticion("POST", "/api/2fa/iniciar", Some(&sesion), None)).await;
        assert_eq!(s, StatusCode::OK, "{cuerpo}");
        let secreto = cuerpo["secreto"].as_str().unwrap().to_string();

        // 1. Con el código BUENO y la contraseña mala: no activa. El código bueno
        //    es lo que hace que esto discrimine — si fuera malo, el rechazo podría
        //    venir del código y la prueba pasaría con el hueco abierto.
        let c = codigo(&secreto, ahora());
        let (s, _) = llamar(
            &app,
            peticion(
                "POST",
                "/api/2fa/activar",
                Some(&sesion),
                Some(&format!(r#"{{"codigo":"{c}","clave":"no-es-la-mia"}}"#)),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);

        let confirmado: Option<String> =
            sqlx::query_scalar("SELECT totp_confirmado_en FROM usuarios WHERE id = 1")
                .fetch_one(&e.bd)
                .await
                .unwrap();
        assert!(confirmado.is_none(), "se activó el factor sin saber la contraseña");
        // Y sin códigos de recuperación por el camino: si se hubieran generado, el
        // rechazo habría dejado diez credenciales colgando de un intento fallido.
        let colgando: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM codigo_recuperacion WHERE usuario_id = 1")
                .fetch_one(&e.bd)
                .await
                .unwrap();
        assert_eq!(colgando, 0, "un rechazo dejó códigos de recuperación escritos");

        // 2. Y con la contraseña BUENA, ABRE — la mitad sin la cual todo lo de
        //    arriba lo cumpliría igual un extremo que no activa nunca.
        let c = codigo(&secreto, ahora());
        let (s, cuerpo) = llamar(
            &app,
            peticion(
                "POST",
                "/api/2fa/activar",
                Some(&sesion),
                Some(&format!(r#"{{"codigo":"{c}","clave":"{CLAVE}"}}"#)),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{cuerpo}");
        assert_eq!(cuerpo.as_array().unwrap().len(), CODIGOS_RECUPERACION);

        // Y lo activado sirve: el login deja de abrir sesión con la contraseña sola.
        let (_, cuerpo, cookies) = login(&app).await;
        assert_eq!(cuerpo["segundo_factor"], serde_json::json!(true));
        assert!(
            valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).is_none(),
            "la contraseña sola siguió entregando cookie de sesión"
        );
    }

    #[tokio::test]
    async fn el_segundo_paso_tambien_tiene_freno() {
        // Seis dígitos son un millón, y con tres ventanas hay tres válidas a la
        // vez. Si el freno solo cubriera la contraseña, quien la tenga prueba
        // códigos sin límite y el segundo factor no añade nada.
        let (app, e) = servidor(true).await;
        let (_, _, cookies) = login(&app).await;
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).unwrap();
        enrolar(&app, &sesion).await;
        sin_codigos_de_recuperacion(&e).await;

        let (_, _, cookies) = login(&app).await;
        let pendiente = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE_PENDIENTE).unwrap();

        for i in 0..5 {
            let (s, _) = llamar(
                &app,
                peticion("POST", "/api/2fa/verificar", Some(&pendiente), Some(r#"{"codigo":"000000"}"#)),
            )
            .await;
            assert_eq!(s, StatusCode::UNAUTHORIZED, "el intento {i} no dio 401");
        }
        let (s, _) = llamar(
            &app,
            peticion("POST", "/api/2fa/verificar", Some(&pendiente), Some(r#"{"codigo":"000000"}"#)),
        )
        .await;
        assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "el segundo factor no tiene freno");
    }

    /// **Volver a entrar con la contraseña NO le borra los fallos al segundo
    /// paso.**
    ///
    /// La prueba de arriba mide cinco fallos seguidos, y por eso no veía esto: el
    /// login anotaba un ÉXITO en `intento_acceso` antes de emitir el token
    /// pendiente, y `guaca::freno` solo cuenta los fallos posteriores al último
    /// éxito. Quien ya tiene la contraseña —el atacante contra el que existe todo
    /// esto— reponía el contador a voluntad y el freno no llegaba a engancharse
    /// nunca: cuatro códigos malos, volver a entrar, otros cuatro.
    ///
    /// Cuatro fallos y luego volver a entrar, a propósito: con cuatro la espera
    /// todavía es cero, así que el ciclo del atacante no gastaba ni un segundo. Lo
    /// que se afirma es que los fallos se SUMAN a través del acceso de en medio:
    /// el quinto todavía pasa como fallo y el sexto ya está frenado. Si alguien
    /// vuelve a escribir el éxito en el primer paso, el contador arranca de cero
    /// tras el acceso, el sexto sale 401 y esta prueba se pone roja.
    #[tokio::test]
    async fn volver_a_entrar_no_le_borra_los_fallos_al_segundo_paso() {
        let (app, e) = servidor(true).await;
        let (_, _, cookies) = login(&app).await;
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).unwrap();
        enrolar(&app, &sesion).await;
        sin_codigos_de_recuperacion(&e).await;

        let malo = r#"{"codigo":"000000"}"#;
        let fallar = async |pendiente: &str| {
            llamar(&app, peticion("POST", "/api/2fa/verificar", Some(pendiente), Some(malo))).await.0
        };

        let (_, _, cookies) = login(&app).await;
        let pendiente = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE_PENDIENTE).unwrap();
        for i in 1..=4 {
            assert_eq!(fallar(&pendiente).await, StatusCode::UNAUTHORIZED, "el intento {i}");
        }

        // El acceso de en medio: la contraseña es correcta, así que pasa. Lo que
        // NO puede hacer es tocar el contador del segundo paso.
        let (s, _, cookies) = login(&app).await;
        assert_eq!(s, StatusCode::OK, "con cuatro fallos el acceso todavía no se frena");
        let pendiente = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE_PENDIENTE).unwrap();

        assert_eq!(fallar(&pendiente).await, StatusCode::UNAUTHORIZED, "el quinto intento");
        assert_eq!(
            fallar(&pendiente).await,
            StatusCode::TOO_MANY_REQUESTS,
            "volver a entrar con la contraseña le borró los fallos al segundo factor"
        );
    }

    /// Renovar los códigos mata los viejos y **no toca el secreto**.
    ///
    /// Las dos mitades importan y fallan al revés. Si el secreto se regenerara,
    /// la persona pediría diez papeles nuevos y se quedaría con un teléfono que
    /// genera códigos que ya no valen —sin saberlo hasta el siguiente acceso—. Y
    /// si los viejos sobrevivieran, el papel que se tiró a la basura seguiría
    /// abriendo la cuenta, que es exactamente de lo que se huye al renovarlos.
    #[tokio::test]
    async fn renovar_los_codigos_mata_los_viejos_y_deja_el_secreto_donde_estaba() {
        let (app, e) = servidor(true).await;
        let (_, _, cookies) = login(&app).await;
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).unwrap();
        let (secreto, viejos) = enrolar_con_codigos(&app, &sesion).await;

        // Con la contraseña equivocada no se entregan: son diez credenciales que
        // entran en lugar del segundo factor.
        let (s, _) = llamar(
            &app,
            peticion("POST", "/api/2fa/codigos", Some(&sesion), Some(r#"{"clave":"otra-cosa"}"#)),
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "los entregó sin la contraseña");

        let (s, cuerpo) = llamar(
            &app,
            peticion(
                "POST",
                "/api/2fa/codigos",
                Some(&sesion),
                Some(&format!(r#"{{"clave":"{CLAVE}"}}"#)),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{cuerpo}");
        let nuevos: Vec<String> =
            cuerpo.as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_string()).collect();
        assert_eq!(nuevos.len(), 10);
        assert!(nuevos.iter().all(|n| !viejos.contains(n)), "repitió un código viejo");

        let quedan: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM codigo_recuperacion WHERE usuario_id = 1")
                .fetch_one(&e.bd)
                .await
                .unwrap();
        assert_eq!(quedan, 10, "quedaron códigos de más: los viejos no se borraron");

        // Un código viejo ya no entra…
        let (_, _, cookies) = login(&app).await;
        let pendiente = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE_PENDIENTE).unwrap();
        let (s, _) = llamar(
            &app,
            peticion(
                "POST",
                "/api/2fa/verificar",
                Some(&pendiente),
                Some(&format!(r#"{{"codigo":"{}"}}"#, viejos[0])),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "un código viejo siguió abriendo la cuenta");

        // …y el teléfono sigue sirviendo: el secreto es el mismo.
        let (_, _, cookies) = login(&app).await;
        let pendiente = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE_PENDIENTE).unwrap();
        let c = codigo(&secreto, ahora() + 30);
        let (s, cuerpo) = llamar(
            &app,
            peticion(
                "POST",
                "/api/2fa/verificar",
                Some(&pendiente),
                Some(&format!(r#"{{"codigo":"{c}"}}"#)),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "renovar los códigos cambió el secreto: {cuerpo}");
    }

    /// Sin factor activo no se entregan códigos de recuperación.
    ///
    /// Serían una segunda contraseña, más corta y en papel, respaldando algo que
    /// no existe.
    #[tokio::test]
    async fn sin_segundo_factor_no_hay_codigos_que_renovar() {
        let (app, _) = servidor(true).await;
        let (_, _, cookies) = login(&app).await;
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).unwrap();

        let (s, _) = llamar(
            &app,
            peticion(
                "POST",
                "/api/2fa/codigos",
                Some(&sesion),
                Some(&format!(r#"{{"clave":"{CLAVE}"}}"#)),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn sin_token_pendiente_no_se_puede_verificar_nada() {
        // El extremo del segundo paso no puede ser una puerta abierta: sin el
        // pendiente —o con la cookie de sesión en su lugar— no hay a quién
        // identificar.
        let (app, _) = servidor(true).await;
        let (_, _, cookies) = login(&app).await;
        let sesion = valor_de(&cookies, crate::sesion::NOMBRE_COOKIE).unwrap();
        let secreto = enrolar(&app, &sesion).await;
        let c = codigo(&secreto, ahora() + 30);

        let (s, _) = llamar(
            &app,
            peticion("POST", "/api/2fa/verificar", None, Some(&format!(r#"{{"codigo":"{c}"}}"#))),
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);

        // Con una sesión de verdad, tampoco: pertenece al otro dominio de firma.
        let (s, _) = llamar(
            &app,
            peticion("POST", "/api/2fa/verificar", Some(&sesion), Some(&format!(r#"{{"codigo":"{c}"}}"#))),
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }
}
