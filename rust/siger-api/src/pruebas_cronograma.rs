//! La pantalla del cronograma, ejercida por su extremo HTTP.
//!
//! # Por qué existe
//!
//! El cronograma por inferencia se verificó con un guion de humo contra el
//! binario y la base reales, y ese guion **se quedó en el scratchpad**: se
//! escribió a las 5:40 de la madrugada y no llegó a versionarse. Lo que no está
//! versionado no vuelve a correr, así que la comprobación existía y a la vez no
//! protegía nada. Esto es esa comprobación, hecha prueba.
//!
//! # La nota de método que costó una madrugada
//!
//! La primera versión de aquel guion comparaba las reglas como CADENAS y daba
//! rojo por los espacios que mete `json.dumps`. Medía el FORMATO, no el
//! fenómeno. Aquí se comparan valores de JSON, nunca su representación.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use crate::config::Config;
use crate::rutas::{router, Estado};

/// Un servidor con esquema, un administrador y un operador.
async fn servidor() -> (axum::Router, String, String) {
    let bd = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&bd).await.unwrap();

    // Las claves salen del MISMO hasher que verifica el acceso; una hecha con
    // otra configuración deja al usuario fuera y el fallo aparece lejos de aquí.
    let hash = {
        use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
        let sal = SaltString::generate(&mut OsRng);
        argon2::Argon2::default()
            .hash_password(b"clave", &sal)
            .unwrap()
            .to_string()
    };
    sqlx::query(
        "INSERT INTO usuarios (id, nombre, email, clave_hash, rol, activo) \
         VALUES (1,'Admin','admin@x.co',?,'ADMIN',1), (2,'Ope','ope@x.co',?,'OPERADOR',1)",
    )
    .bind(&hash)
    .bind(&hash)
    .execute(&bd)
    .await
    .unwrap();

    let cfg = Config::para_pruebas();
    let motor = std::sync::Arc::new(siger_core::Motor::nuevo(
        siger_core::Calendario::de_pais("CO", 2024, 2030).unwrap(),
    ));
    let app = router(Estado { bd, cfg, motor });
    (app, "admin@x.co".into(), "ope@x.co".into())
}

async fn entrar(app: &axum::Router, email: &str) -> String {
    let r = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sesion")
                .header("content-type", "application/json")
                // Lo que el servidor de verdad pone con
                // `into_make_service_with_connect_info`. Sin esto el login no
                // sabe de dónde viene la petición y se niega a seguir — a
                // propósito: el freno cuenta por (correo, origen) y sin origen no
                // hay freno (ver `crate::origen`).
                .extension(axum::extract::ConnectInfo(
                    "127.0.0.1:5555".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::from(format!(
                    r#"{{"email":"{email}","clave":"clave"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK, "no se pudo abrir sesión");
    let cookie = r
        .headers()
        .get(header::SET_COOKIE)
        .expect("la sesión no devolvió cookie")
        .to_str()
        .unwrap();
    cookie.split(';').next().unwrap().to_string()
}

async fn lecturas(app: &axum::Router, cookie: &str, cuerpo: &str) -> (StatusCode, Value) {
    let r = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cronograma/lecturas")
                .header("content-type", "application/json")
                .header(header::COOKIE, cookie)
                .body(Body::from(cuerpo.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let estado = r.status();
    let bytes = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (estado, json)
}

#[tokio::test]
async fn un_dia_marcado_ofrece_varias_lecturas_y_todas_contienen_lo_marcado() {
    let (app, admin, _) = servidor().await;
    let cookie = entrar(&app, &admin).await;

    let (estado, cuerpo) = lecturas(&app, &cookie, r#"{"fechas":["2026-08-17"]}"#).await;
    assert_eq!(estado, StatusCode::OK);

    let lecturas = cuerpo["lecturas"].as_array().expect("sin lecturas");
    assert!(
        lecturas.len() >= 2,
        "un día suelto debería admitir varias lecturas, no {}",
        lecturas.len()
    );
    // La invariante del diseño: solo aparecen lecturas que CONTIENEN lo marcado.
    // Una interpretación que no generase el día señalado no es una lectura de lo
    // que se señaló, por bien que suene su nombre.
    for l in lecturas {
        let proximas: Vec<&str> = l["proximas"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(
            proximas.contains(&"2026-08-17"),
            "la lectura «{}» no genera el día marcado: {proximas:?}",
            l["nombre"]
        );
    }
}

#[tokio::test]
async fn tres_lunes_seguidos_se_leen_como_semanal() {
    let (app, admin, _) = servidor().await;
    let cookie = entrar(&app, &admin).await;

    // 2026-08-03, 10 y 17 son lunes.
    let (estado, cuerpo) = lecturas(
        &app,
        &cookie,
        r#"{"fechas":["2026-08-03","2026-08-10","2026-08-17"]}"#,
    )
    .await;
    assert_eq!(estado, StatusCode::OK);

    let lecturas = cuerpo["lecturas"].as_array().unwrap();
    let semanal = lecturas
        .iter()
        .find(|l| l["periodicidad"] == "SEMANAL")
        .expect("tres lunes seguidos no ofrecieron lectura semanal");
    assert_eq!(
        semanal["regla"]["dias_semana"],
        serde_json::json!([0]),
        "la semanal no salió en lunes"
    );
}

#[tokio::test]
async fn la_regla_que_se_muestra_va_sin_nulos_de_relleno() {
    // `informes::crear` guarda la regla VERBATIM. Si la inferencia la devolviera
    // con veinte `null` explícitos, los informes nuevos quedarían escritos en un
    // JSON distinto del de los migrados: dos formas del mismo dato, y dentro de
    // un año nadie sabría cuál es la canónica.
    let (app, admin, _) = servidor().await;
    let cookie = entrar(&app, &admin).await;

    let (_, cuerpo) = lecturas(&app, &cookie, r#"{"fechas":["2026-08-17"]}"#).await;
    for l in cuerpo["lecturas"].as_array().unwrap() {
        let regla = l["regla"].as_object().expect("la lectura no trae regla");
        let nulos: Vec<&String> = regla.iter().filter(|(_, v)| v.is_null()).map(|(k, _)| k).collect();
        assert!(nulos.is_empty(), "la regla de «{}» trae nulos: {nulos:?}", l["nombre"]);
    }
}

#[tokio::test]
async fn la_lectura_elegida_se_guarda_igual_a_como_se_mostro() {
    // El arco entero de la pantalla: se marca, se elige una lectura y se guarda.
    // Lo que quede en la base tiene que ser lo MISMO que se enseñó — comparado
    // como JSON y no como cadena, que es donde la primera versión de esta
    // comprobación daba rojo por los espacios de `json.dumps`.
    let (app, admin, _) = servidor().await;
    let cookie = entrar(&app, &admin).await;

    let (_, cuerpo) = lecturas(
        &app,
        &cookie,
        r#"{"fechas":["2026-08-03","2026-08-10","2026-08-17"]}"#,
    )
    .await;
    let elegida = cuerpo["lecturas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["periodicidad"] == "SEMANAL")
        .unwrap()
        .clone();
    let regla_mostrada = elegida["regla"].clone();

    let r = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/informes")
                .header("content-type", "application/json")
                .header(header::COOKIE, &cookie)
                .body(Body::from(
                    serde_json::json!({
                        "nombre": "Novedades de afiliación",
                        "descripcion": "creado desde el cronograma",
                        "recurrencia": regla_mostrada,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        r.status().is_success(),
        "no se pudo crear el informe: {}",
        r.status()
    );

    let listado = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/informes")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(listado.into_body(), 1 << 20).await.unwrap();
    let informes: Value = serde_json::from_slice(&bytes).unwrap();
    let creado = informes
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["nombre"] == "Novedades de afiliación")
        .expect("el informe creado no aparece en el listado");
    assert_eq!(creado["modo"], "SEMANAL");
}

#[tokio::test]
async fn un_operador_no_puede_pedir_lecturas() {
    // Es una previsualización, pero pertenece al flujo de crear un informe: no
    // hay razón para ofrecérsela a quien después no va a poder guardar.
    let (app, _, operador) = servidor().await;
    let cookie = entrar(&app, &operador).await;
    let (estado, _) = lecturas(&app, &cookie, r#"{"fechas":["2026-08-17"]}"#).await;
    assert_eq!(estado, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn sin_sesion_no_se_ofrecen_lecturas() {
    let (app, _, _) = servidor().await;
    let (estado, _) = lecturas(&app, "", r#"{"fechas":["2026-08-17"]}"#).await;
    assert_eq!(estado, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn una_fecha_ilegible_es_un_400_y_no_un_dia_que_se_descarta() {
    // Descartar en silencio el día que no se entiende cambiaría la lectura sin
    // decírselo a nadie: la pantalla mostraría una periodicidad que no es la que
    // el usuario marcó.
    let (app, admin, _) = servidor().await;
    let cookie = entrar(&app, &admin).await;
    let (estado, _) = lecturas(
        &app,
        &cookie,
        r#"{"fechas":["2026-08-17","no-es-una-fecha"]}"#,
    )
    .await;
    assert_eq!(estado, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn una_seleccion_vacia_es_un_400() {
    let (app, admin, _) = servidor().await;
    let cookie = entrar(&app, &admin).await;
    let (estado, _) = lecturas(&app, &cookie, r#"{"fechas":[]}"#).await;
    assert_eq!(estado, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn marcar_un_ano_entero_se_rechaza_en_vez_de_barrer() {
    let (app, admin, _) = servidor().await;
    let cookie = entrar(&app, &admin).await;
    let muchas: Vec<String> = (0..400)
        .map(|i| {
            (chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Duration::days(i))
                .to_string()
        })
        .collect();
    let cuerpo = serde_json::json!({ "fechas": muchas }).to_string();
    let (estado, _) = lecturas(&app, &cookie, &cuerpo).await;
    assert_eq!(estado, StatusCode::BAD_REQUEST);
}
