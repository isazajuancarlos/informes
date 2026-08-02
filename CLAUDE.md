# CLAUDE.md — SIGER (informes)

> **Configuración general: `~/.claude/CLAUDE.md`** — reglas que no se rompen,
> las directivas numeradas y las máquinas montadas. Se carga en toda sesión y
> **no se copia aquí**; este archivo lleva SOLO lo propio de SIGER.
>
> **Esta instancia se arranca desde esta carpeta**: `cd /mnt/data/informes && claude`.
> Desde el 2026-08-01 no se trabajan varios proyectos desde una sola sesión — y
> aquí importa doblemente: `/security-review` construye su diff con el `cwd`.

Guía para Claude Code en este repositorio. **Verificado contra el código, no
citado de memoria. Si algo aquí no cuadra con lo que ves, gana el repositorio —
y actualiza este archivo.**

## El producto

Se llama **SIGER** — *Sistema Integral de Gestión de Informes*. La carpeta se
llama `informes`: el repositorio no es el producto.

Gestiona obligaciones de entrega de informes: define cada obligación (quién la
produce, quién la recibe, cada cuánto), genera los vencimientos, avisa, recibe la
entrega, la firma y deja rastro auditable. Cuatro roles (ADMIN, OPERADOR,
SUPERVISOR, TERCERO) con control de acceso por alcance.

## Rust puro (cutover 2026-07-28)

SIGER **se migró entero a Rust** (estándar técnico de Xiliux) y el Python quedó
en la historia de git. Hoy es **un solo binario** con la interfaz **Angular
embebida** dentro del ejecutable y **SQLite** como base. No queda `app/`,
`main.py`, NiceGUI, Docker ni pytest: si lees documentación que los mencione,
está desactualizada.

Superviven, y a propósito, dos scripts de Python que **sirven al cliente**, no a
la app: `rust/sembrar_demo.py` (siembra la base de demostración) y
`verificar_clausulado.py` (regenera/verifica el deliverable legal
`MODELO_LICENCIA_INFORMES.md`).

### Estructura

```
rust/
  Cargo.toml    # workspace; MSRV 1.85, publish = false, license LicenseRef-Xiliux-Comercial
  deny.toml  .cargo/audit.toml   # cadena de suministro: fuentes permitidas y el único ignore
  siger-core/   # dominio puro: el motor, sin I/O
    calendario.rs recurrencia.rs fechas.rs inferencia.rs patrones.rs compat.rs
    tests/oraculo.rs, oraculo_inferencia.rs  # oráculos diferenciales (ver abajo)
  siger-api/    # binario axum + Angular embebido (rust-embed) + SQLite (SQLx)
    build.rs     # rompe el release si la interfaz NO quedó dentro (ver abajo)
    main.rs rutas.rs db.rs config.rs sesion.rs autz.rs
    acceso.rs origen.rs  # freno del login: las dos consultas y de dónde viene quien pide
    dosfactores.rs segundofactor.rs  # 2FA: la REGLA y las columnas (#212)
    qr.rs        # el QR del enrolamiento, en SVG y sin dependencias
    cuentas.rs participacion.rs informes.rs flujo.rs estadisticas.rs
    alertas.rs planificador.rs firma.rs importar.rs estaticos.rs respaldo.rs
    pruebas_cronograma.rs   # solo #[cfg(test)]: el cronograma de punta a punta
    tests/cli.rs            # los subcomandos ejercidos como proceso, no como función
    tests/oraculo_qr.json   # las 40 versiones de QR según libqrencode, congeladas
    migrations/  # 0001_inicial … 0007_segundo_factor (sqlx::migrate!)
    estaticos/   # DONDE SE EMPOTRA la interfaz. En git solo va el .gitkeep
angular/        # la interfaz; se construye e incrusta en el binario
despliegue/     # unidades systemd, nginx y los guiones de la demo (ver DESPLIEGUE.md)
```

**El grueso del cableado está en `rutas.rs` (~2.000 líneas): router, extractores,
lectura de documentos y 25 de las pruebas.** Es el primer sitio donde mirar
cuando algo va de HTTP; `autz.rs` es el único donde se decide quién ve qué.

En la RAÍZ quedan `app/`, `tests/`, `.venv/`, `.nicegui/`, `herramientas/` y
varios `*.db`: son **restos sin versionar** del Python —`git ls-files` no
devuelve ni uno— y no son la fuente de nada. No los edites ni los cites. (Las
`tests/` que sí cuentan son las de dentro de `rust/`.)

## Comandos

```bash
cd rust
cargo build --release            # produce target/release/siger-api (Angular embebido)
cargo test  --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings   # el CI usa -D warnings

cd ../angular && npm ci && npx ng build                  # reconstruir la interfaz

./rust/construir.sh              # el ARTEFACTO: interfaz dentro del binario
./rust/humo.sh                   # y probarlo por HTTP, como lo prueba el mundo
python rust/sembrar_demo.py      # base de demostración en rust/demo/siger.db
```

**Correr UNA prueba** (el nombre es un filtro por subcadena, no una ruta):

```bash
cd rust
cargo test -p siger-api --bin siger-api -- --list        # los 98 nombres, para elegir
cargo test -p siger-api rutas::pruebas_acceso           # el freno del login entero
cargo test -p siger-api --bin siger-api estaticos::     # un módulo
cargo test -p siger-core --test oraculo -- --nocapture  # solo el oráculo del motor
cargo test -p siger-api --test cli                      # los subcomandos, como proceso
cargo test --workspace --all-targets --no-fail-fast     # ver TODOS los rojos
```

`--no-fail-fast` importa: sin él `cargo test` para en el primer binario que
falla y las pruebas de los demás crates **no llegan a ejecutarse** — un rojo
puede estar tapando otros. El CI corre sin él a propósito (falla rápido).

Convención: **el nombre de cada prueba es la frase que afirma**
(`el_servidor_lleno_dice_ocupado_y_no_no_descifra`), y los módulos se llaman
`tests` o `pruebas` indistintamente — filtra por el nombre, no por el módulo.

**Una prueba en verde no dice nada; lo que dice algo es que se ponga ROJA al
romper lo que afirma vigilar.** Antes de dar por buena una salvaguarda, múta­la y
mira quién la caza — y si NO cae nadie, sospecha primero de la mutación: añadir
`clave_hash` al `SELECT` de `cuentas.rs` no filtra el hash, porque la puerta es el
struct y `FromRow` ignora las columnas que no declara. La mutación inofensiva
acusa al banco, no a la prueba.

MSRV 1.85 (evitar API que exija más; p. ej. `% 2 != 0`, no `is_multiple_of`).
El CI (`.github/workflows/ci.yml`) tiene cuatro trabajos: `rust` (test +
clippy), `frontend` (ng build), `cargo-audit (RustSec)` y `cargo-deny (supply
chain)`. Se dispara en los PR contra `main` y en los empujes a `main`; **una
rama sin PR abierto no lanza CI**, y es deliberado: este repositorio es privado
y sus minutos salen del cupo de la cuenta.

**Lo que el CI NO comprueba, y por eso existen los dos guiones de arriba:** no
construye el artefacto ni empotra la interfaz, así que la prueba
`estaticos::pruebas` —la que exige que el index empotrado se pueda ejecutar bajo
`script-src 'self'`— **se salta siempre allí**. Su verde en el CI no dice nada
del binario. `construir.sh` la corre con la interfaz ya dentro (paso 4/4, y
exige que se haya EJECUTADO, no solo que salga 0), y `humo.sh` arranca el
binario de release contra una base temporal y le habla por socket: interfaz
servida de verdad, el `.js` que el index pide viviendo DENTRO, las cabeceras en
la respuesta real y el freno del login frenando al sexto intento.

**`cargo build --release` a secas falla si `estaticos/` está vacío**, y lo hace
`build.rs` a propósito: `rust-embed` lee del disco en *debug* y empotra en
*release*, así que un clon limpio producía —sin avisar— un binario que arranca,
sirve la API y devuelve 404 en la raíz. El camino permisivo existe pero **hay
que pedirlo**: `SIGER_SIN_INTERFAZ=1 cargo build --release`. En *debug* no
aplica, por eso `cargo test` funciona sin haber construido la interfaz.

Subcomandos del binario: `importar <origen> <destino>` (vuelca una base del
Python anterior), `alertas` (una pasada, para un timer),
`respaldar <carpeta>` / `restaurar <carpeta> [--forzar]` (base en caliente vía
`VACUUM INTO` + los archivos de entrega, y la copia **se verifica** antes de
darse por buena), `generar-secreto`, `generar-claves-firma`, `hashear` (Argon2,
mismos parámetros que el acceso). Todos leen del entorno **dónde** está la base:
respaldan lo que el servicio usa de verdad, no lo que alguien creía.

Configuración por entorno en `config.rs`; **el binario no arranca si falta un
secreto** (no hay defectos peligrosos). Plantilla en `.env.example`. Las
opciones se documentan como **cascadas con tabla** en el doc-comment de su
función (`permisos_de_cripto`, `direccion_de_escucha`): al añadir una, sigue
ese formato — y un valor presente pero ilegible **no cae al defecto**, falla.

## Invariantes que explican casi todo

1. **El flujo es lo único que el producto modela con columnas.** Quién produce,
   quién recibe, quién consume vive en **una sola tabla** de participación
   (`participacion.rs`). Lo demás —campos de la ficha, reglas— es configuración
   del cliente y vive como datos, no como esquema.
2. **Lo opcional nunca puede impedir cumplir (modo degradado, #71).** La firma
   sin clave, el correo sin SMTP, la alerta que falla: todo degrada con gracia.
   Un bloqueo convertiría a Xiliux en la causa del incumplimiento del cliente
   ante su ente de control. Verificado por inyección de fallo en `alertas.rs`
   (tests `degradado`): un SMTP caído no marca, no pierde y no propaga.
3. **Ante la ausencia de un dato, se falla ruidosamente; nunca se sustituye por un
   defecto.** `config.rs` no trae valores por defecto para secretos.

## Administración de cuentas (`cuentas.rs`)

`GET/POST /api/cuentas`, `PUT /api/cuentas/{id}`, `POST /api/cuentas/{id}/clave`
(restablecer la de otro) y `POST /api/mi-clave` (la propia, cualquier rol).
Pantallas en `angular/src/app/paginas/{cuentas,mi-clave}.ts`.

**Recurso aparte de `/api/usuarios` a propósito**, siendo la misma tabla:
`/api/usuarios` significa «quién puede ser parte de un flujo» y solo trae los
activos, porque lo consume el selector de partes. La administración necesita
también los desactivados —si no, reactivar a alguien exigiría `sqlite3`— y el
rol. Dos vocabularios sobre una fila; el valor lo fija quien ya lee.

Cuatro reglas que no se pueden relajar sin romper algo (las siete están cubiertas
por un banco de mutación, ver abajo):

1. **Solo `Admin`.** `Supervisor` NO, aunque su alcance también sea `Todas`: eso
   es para LEER entregas. Si administrar colgara de `puede_leer`, un rol de
   consulta podría fabricarse un administrador.
2. **Todo cambio de contraseña sube `token_version` en la MISMA sentencia.** Es
   lo que hace que cambiar la clave eche a quien entró con la vieja — la razón de
   ser de la columna (#194). En dos sentencias habría un instante con la clave
   nueva puesta y las sesiones viejas vivas.
3. **`/api/mi-clave` exige la contraseña actual** y, tras cambiarla, **reemite la
   cookie** con la versión nueva: sigues dentro aquí y caes en el resto. Sin la
   comprobación, una sesión robada se queda con la cuenta para siempre; sin la
   reemisión, cambiar la clave te expulsa a ti y nadie usa el botón.
4. **El último administrador activo no se puede degradar ni desactivar.** Es un
   imposible, no un inusual: sin administrador no se puede ascender a nadie y la
   única salida sería abrir la base a mano.

El hash sale del **mismo** Argon2 que verifica el acceso, con turno del semáforo
de cripto (`ESPERA_CRIPTO`), como el login y el descifrado en reposo.

## La bitácora (`auditoria`, escrita por `rutas::auditar`)

Una tabla —`momento`, `evento`, `email`, `detalle`— y **una sola función que la
escribe**. Que el registro falle no tumba la operación, pero tampoco pasa en
silencio: se anota en el log del proceso.

**SIGER no trata dato sensible** (art. 5 de la Ley 1581): `stakeholders` es
nombre + externo, `usuarios` es nombre, correo y rol, y las entregas son
documentos institucionales. Por eso el hallazgo M2 —la bitácora de acceso a la
historia clínica— **no se traslada tal cual desde medico**; lo que sí aplica aquí
es que quien SACA un documento o el padrón de personas quede registrado.

Lo que se anota, y por qué esa lista y no otra:

| Evento | Cuándo |
|---|---|
| `entrega_descargada` | el documento **salió** — se anota tras `Documento::Claro`, no al entrar |
| `entrega_ajena_solicitada` | alguien pidió una entrega que EXISTE y no le corresponde (descarga o verificación) |
| `cuentas_listadas` | se leyó el padrón completo de personas, con cuántas filas y sin ids |
| `acceso`, `acceso_fallido`, `acceso_frenado`, `salida`, `salida_total` | la puerta |
| `cuenta_creada`, `cuenta_actualizada`, `clave_restablecida`, `clave_cambiada`, `clave_propia_rechazada` | administración de cuentas |
| `2fa_enrolamiento_iniciado`, `2fa_activado`, `2fa_desactivado`, `2fa_desactivacion_rechazada`, `2fa_codigos_renovados`, `2fa_codigos_rechazados` | el segundo factor, desde la cuenta propia |
| `acceso_falta_segundo_factor`, `2fa_fallido`, `2fa_codigo_reutilizado`, `2fa_recuperacion_usada`, `2fa_frenado`, `2fa_secreto_ilegible` | el segundo paso de la puerta |
| `informe_creado`, `informe_actualizado`, `participacion_actualizada`, `justificacion_*`, `entrega_registrada`, `entrega_contenido_no_concuerda` | el flujo |

Tres decisiones que se rompen solas si alguien las "simplifica":

1. **`entrega_descargada` se escribe DESPUÉS de leer el documento.** Los otros
   tres estados de `Documento` no entregaron nada, y anotar una lectura que no
   ocurrió es la misma clase de mentira que acusar de alteración al que estaba
   ocupado.
2. **Un id inexistente NO se anota; uno ajeno sí.** La segunda consulta
   (`SELECT 1 FROM entregas WHERE id = ?`) corre solo en el camino de error y
   existe para DISCRIMINAR: un 404 por enlace viejo llenaría la bitácora de
   falsos positivos hasta que nadie la mirara (directiva 23). **La respuesta al
   cliente es el mismo 404 en los dos casos**: lo que discrimina es lo que se
   escribe, no lo que se contesta. `Alcance::Todas` **no lleva caso aparte** —su
   `WHERE` es solo `id = ?`, así que llegar ahí ya significa que no existe—, y el
   banco de mutación lo comprobó: el `if` que había se podía quitar sin que
   ninguna prueba cayera, porque no decidía nada.
3. **`listar_entregas`, `/api/usuarios` y `/api/stakeholders` NO se anotan.** Los
   pide cada pintado de pantalla; una bitácora que crece con cada carga ahoga la
   señal que existe para dar.

Lo sujetan cinco pruebas en `rutas.rs` y una en `cuentas.rs`, y un banco de
mutación de siete casos las cubre. Lo que hoy **no** se registra, y está dicho
aquí para que no se dé por hecho: el intento rechazado por `solo_admin`, y el
veredicto de `verificar_entrega` (que es trazabilidad de firma, no acceso a dato
personal).

## Acceso y alcance (`autz.rs`)

Roles `Admin | Operador | Supervisor | Tercero`. El **alcance** de lo que cada
quien ve es un valor explícito: `Todas` (admin/supervisión), `DeUsuario(id)`,
`DeStakeholder(id)`, `Ninguna` («no le corresponde nada» es un valor legítimo, no
un hueco). Las consultas filtran por alcance; no hay «filtro opcional».

## La puerta: freno y revocación (`acceso.rs`, `origen.rs`, `sesion.rs`)

La cookie es `id|version|caducidad|firma` (HMAC-SHA256, `HttpOnly`,
`SameSite=Strict`; `Secure` lo añade el proxy). `version` es el `token_version`
del usuario: viaja firmado y se compara en la misma relectura que ya se hacía
para el rol, así que **subir el entero cierra de golpe todas las sesiones de esa
persona** sin guardar estado de sesión en el servidor.

Quien sube el entero es `acceso::revocar_sesiones`, y su único llamante es
`POST /api/salir-todo` («cerrar sesión en todos los dispositivos»). **No recibe
cuerpo ni id**: revoca al usuario que la cookie firmada identifica. Si aceptara
un id sería un botón para echar a cualquiera sabiendo su número. Un administrador
que quiera dejar fuera a alguien usa el camino que ya existe —desactivar la
cuenta, que `activo` se relee en cada petición—; no hay revocación de sesiones
ajenas porque no resuelve ningún caso que ése no cubra.

Dos detalles que se rompen solos si alguien los "simplifica": el incremento es
`token_version + 1` **dentro del SQL** (calcularlo en Rust pierde incrementos
bajo concurrencia — medido: 6 de 100), y el `bool` que devuelve **no se
descarta** (cero filas no es un error de SQL, así que descartarlo respondería
«cerré todas tus sesiones» sin haber cerrado ninguna).

El freno al ensayo de contraseñas cuenta por pareja **(correo, origen)**: cinco
fallos sin castigo, luego 15 s doblando con techo en 5 min, y un acierto lo borra
—que un acierto borre es lo que obliga a mirar **quién escribe el acierto**: ver
el punto 2 del segundo factor, más abajo—.
**No bloquea la cuenta** — un bloqueo deja fuera al usuario legítimo, que es peor
que el ataque que evita. La REGLA vive en `guaca::freno`, compartida con medico;
aquí solo están las dos consultas. El origen es la dirección del socket salvo que
se declare `SIGER_TRAS_PROXY=1`, y si entonces no llega `X-Real-IP` **el acceso
falla**: rellenarlo con la dirección del proxy degradaría el freno a contar solo
por cuenta.

Las tres respuestas de rechazo —correo inexistente, cuenta desactivada, clave
mala— cuestan **lo mismo**: cuando no hay usuario se verifica contra un hash
señuelo. Sin eso, el mensaje es idéntico y el cronómetro dice qué correos existen.

## El segundo factor (`dosfactores.rs`, `segundofactor.rs`, #212)

TOTP (RFC 6238), 6 dígitos, paso de 30 s, **una** ventana de tolerancia a cada
lado. El reparto es el mismo que el del freno: la REGLA en `dosfactores.rs` —sin
`sqlx`, sin `Config`, sin ningún tipo de SIGER— y las columnas en
`segundofactor.rs`. `dosfactores.rs` está escrito **para mudarse a `guaca`** el
día que medico necesite lo mismo; hasta entonces vive aquí, porque guaca se
lleva desde otra instancia y se consume por `rev` fijado.

`GET /api/2fa` (estado), `POST /api/2fa/{iniciar,activar,desactivar,codigos}`
(cuenta propia, cualquier rol) y `POST /api/2fa/verificar` — el único que **no**
exige sesión, porque es el segundo paso del login.

`codigos` renueva los diez de recuperación **sin tocar el secreto**, y exige la
contraseña por lo mismo que `desactivar`: entrega diez credenciales que entran en
lugar del segundo factor. Existe porque sin él la única forma de reponerlos era
desactivar y volver a enrolar —QR nuevo en todos los teléfonos y, sobre todo, un
rato con la cuenta protegida por la contraseña sola, que es el estado del que el
segundo factor viene a sacarla—. Quedarse sin códigos es lo normal: se gastan de
uno en uno, y cada uno se usa el día que el teléfono no está.

Seis cosas que se rompen solas si alguien las "simplifica":

1. **El login con 2FA no emite cookie de sesión**, sino un token *pendiente* en
   **otra cookie** (`siger_pendiente`, 5 min) y firmado en **otro dominio**
   (`firmar_en`). La ficha #212 proponía meterlo como un campo más de la carga
   firmada: **no se puede**, `sesion::verificar` exige exactamente tres campos y
   eso tiene su prueba. La separación por dominio es más fuerte — misma carga,
   firma distinta— y no toca el formato.
2. **El freno cubre los DOS pasos, y el primero no lo repone.**
   `/api/2fa/verificar` consulta `guaca::freno` y anota en `intento_acceso` igual
   que el login. Sin eso, quien ya tiene la contraseña prueba el millón de
   códigos y el segundo factor no añade nada. Y la otra mitad, que costó una
   auditoría: **el login con segundo factor NO anota nada en `intento_acceso`**,
   ni éxito ni fallo. Anotaba el éxito —«la contraseña era buena, no frenemos a
   quien va a teclear su código»— y como `guaca::freno` cuenta *los fallos
   posteriores al último éxito*, ese éxito ponía a cero el contador que es el
   único freno del segundo paso. Quien tenía la contraseña lo reponía a voluntad:
   cuatro códigos malos, volver a entrar, otros cuatro, sin esperar nunca. El
   éxito lo escribe quien abre de verdad: el camino sin 2FA, y
   `segundofactor::verificar` al final del segundo paso. Lo sujeta
   `volver_a_entrar_no_le_borra_los_fallos_al_segundo_paso`, que es la única que
   cae si alguien lo revierte —la de los cinco fallos seguidos no lo ve.
3. **El contador usado se guarda** (`totp_ultimo_contador`). Es todo el
   antirreplay: un código visto por encima del hombro sirve 90 s si no se
   quema.
4. **Activar exige teclear un código correcto.** Activar al generar el QR dejaría
   fuera de su cuenta a quien tenga el reloj mal, y sin poder entrar a
   arreglarlo.
5. **Sin `SIGER_CLAVE_REPOSO` no se puede enrolar, y se dice.** El secreto se
   guarda cifrado con `guaca::reposo`; en claro, quien copie el archivo genera
   los códigos igual que su dueño. No se guarda en claro «mientras tanto». Esto
   **no** rompe el modo degradado (#71): sin frase el sistema funciona entero y
   nadie queda fuera — lo único que no se puede es activar un factor nuevo.
6. **Desactivar exige la contraseña actual**, y se lleva los códigos de
   recuperación. Sin eso, una sesión robada apaga el segundo factor.

`Veredicto` tiene **cuatro** estados por la misma razón que `Documento`:
`SecretoIlegible` es una avería del despliegue y no se cobra al usuario —ni
cuenta como fallo para el freno ni le dice que su código está mal—, y
`Reutilizado` se anota aparte de `Incorrecto` aunque el cliente reciba lo mismo:
un código repetido escondido entre los dedazos es un ataque que no se ve.

Recuperación: diez códigos de un solo uso, entregados **una vez** y guardados
solo como hash Argon2. `gastar_codigo_recuperacion` los compara **todos** sin
salir en el primer acierto (si no, el tiempo diría cuántos quedan) y los marca
con `usado_en IS NULL` en el `UPDATE`, que es lo que impide gastar uno dos veces
en peticiones simultáneas.

`totp-rs` va con `default-features = false` (su base es HMAC + base32; `qr` e
`image` no entran en un binario offline) y **`TOTP::generate` toma SEGUNDOS, no
el contador** — pasarle el contador compila, valida el código actual y multiplica
la ventana por 30, de 30 s a 15 minutos. Lo caza
`un_codigo_no_sirve_mas_de_noventa_segundos`.

La interfaz son **dos archivos**: `paginas/segundo-factor.ts` (estado,
enrolamiento con QR, códigos de recuperación y baja con contraseña) y el segundo
paso dentro de `paginas/acceso.ts`. La entrada está en el menú de la cuenta,
junto a «Cambiar mi contraseña». El primer formulario del acceso **desaparece**
al pasar al código y la contraseña se borra de memoria: lo que el servidor espera
en ese momento es la cookie pendiente, no las credenciales otra vez.

### El QR se dibuja en el servidor (`qr.rs`)

Un codificador propio —modo byte, nivel M, versiones 1..40, máscara por
penalización— que devuelve **SVG**, y el enrolamiento lo manda ya dibujado en
`qr_svg`. No hay librería de QR ni en npm ni en Rust: la *feature* `qr` de
`totp-rs` arrastra `image` a un binario offline, y una de npm son ~20 KB en el
paquete inicial para una pantalla que cada cuenta abre una vez.

**Cómo se sabe que está bien, que es lo único difícil de esto**: un QR mal
generado no falla, sale un dibujo plausible que ningún teléfono lee. Se mide
contra `qrencode` (libqrencode) con un oráculo dorado —`tests/oraculo_qr.json`,
que regenera `tests/generar_oraculo_qr.py`— comparando **módulo a módulo las
cuarenta versiones**, una por fila de la tabla de bloques. Dos consecuencias que
no se pueden «arreglar» sin romper el oráculo: la N3 se lee como una *razón*
1:1:3:1:1 a cualquier escala (no como once módulos fijos), y sus dos condiciones
de borde son las de libqrencode al pie de la letra. La máscara es una heurística
de calidad: las ocho producen símbolos legibles, así que igualar al oráculo no
cuesta corrección y compra una prueba total.

En la pantalla el SVG va dentro de un `<img>` con `data:` —la CSP lo admite
(`img-src 'self' data:`)— y no con `innerHTML`: así el navegador lo trata como un
documento aparte y sin guiones.

## Firma de entregas (`firma.rs`, Quipu, opcional)

Firma post-cuántica (ML-DSA-87 + Ed25519) de los campos auditables **dentro de la
misma transacción** que fija estado, fecha y archivo, vía el contenedor de Quipu
(`encode_signed`/`decode_verified`). Sin `QUIPU_SIGNING_KEY`, la entrega se
registra igual, sin firma — la ausencia de firma nunca impide cumplir.

## Cifrado en reposo y el documento de cuatro estados (`rutas.rs`)

Con `SIGER_CLAVE_REPOSO`, el archivo de cada entrega se guarda envuelto por
`guaca::reposo` (Argon2id → AEAD → padding Padmé) y la fila lleva su bandera
`cifrado`; sin frase, se guarda en claro. **Nunca se sirve un blob cifrado**.

Lo que hay que entender antes de tocar aquí es el enum `Documento`, que tiene
**cuatro** estados y no dos:

| estado | qué significa | qué NO significa |
|---|---|---|
| `Claro(bytes)` | el documento | |
| `NoEsta` | no hay archivo en esa ruta | |
| `NoDescifra` | falta la frase, está rotada, o el blob fue alterado | |
| `Ocupado` | no se llegó a MIRAR el archivo: el semáforo estaba lleno | **no dice nada del archivo** |

`Ocupado` es un caso aparte a propósito. Si cayera en el mismo cajón que
`NoDescifra`, una tarde de mucho tráfico se leería como una entrega sospechosa —
y una verificación que confunde «no pude mirar» con «vi algo malo» no es un
defecto silencioso: es una **acusación falsa de manipulación**. Al añadir un
camino nuevo por el que un documento no se pueda leer, pregúntate en cuál de
los cuatro cae, y si no cae en ninguno, añade el quinto. Lo sujetan
`sin_la_frase_de_reposo_no_se_acusa_de_alteracion` y
`el_servidor_lleno_dice_ocupado_y_no_no_descifra`.

El semáforo existe porque cada descifrado reserva **64 MiB** de Argon2id en
`spawn_blocking`, y el vivero de tokio admite 512 hilos: sin tope serían 32 GiB
simultáneos. `SIGER_CRIPTO_PARALELA` lo fija (defecto: núcleos). El plazo de
espera lo comparten el descifrado y el Argon2 del login.

## El motor y sus oráculos (`siger-core`)

`recurrencia.rs` genera los vencimientos; `inferencia.rs` va del calendario a la
regla (`inferir`) y ofrece las lecturas posibles de una selección
(`interpretaciones`). **No se revisan: se MIDEN** contra corpus dorados que
produjo el motor de Python antes del cutover, congelados y vendorizados:
`tests/oraculo/{festivos_co,vencimientos}.json` (motor) y
`tests/oraculo_inferencia.json` + `tests/oraculo_interpretaciones.json`. Una sola
fecha distinta pone el test rojo.

## La interfaz (`angular/`)

**Angular 22.1.x** (`@angular/cli` y `@angular/build` 22.1.2) + `@angular/material`
22.1.0 + `material-icons` **empaquetado**, sin una sola petición a un CDN — el
producto se instala sin internet, y la CSP del binario es `script-src 'self'`.
PrimeNG está prohibido (ver el general). Siete páginas en `src/app/paginas/`, el
cliente HTTP y la sesión en `src/app/nucleo/`.

La versión no es libre: la política de Juan (#254) es **una sola versión de
Angular, común a todos los desarrollos, y la mejor —no la última por serlo—**.
Hoy informes marca el objetivo (22.1.x, la línea activa) y medico va por detrás
en 21.2, atado a que su migración de PrimeNG a Material (#255) termine: subirle
PrimeNG a 22 no es una opción porque **dejó de ser MIT**. Los paquetes de Angular
se peer-dependen con versión EXACTA entre sí, así que **no se suben de a uno**:
`ng update @angular/core@22 @angular/cli@22 @angular/material@22`, nunca un
`npm install` suelto sobre el árbol (da `ERESOLVE`).

Comprobar la subida **en el ejecutable, no en el `ng build`**: la interfaz va
empotrada y un binario con el bundle de la vez pasada compila igual de verde.
`humo.sh` sirve para eso —el nombre del `.js` cambia—, y la versión se lee dentro:
`strings rust/target/release/siger-api | grep ng-version`.

**No hay pruebas unitarias de la interfaz**: `vitest` y `tsconfig.spec.json`
están declarados, pero no existe ni un `.spec.ts`. `npm test` no comprueba nada
— lo que sujeta la interfaz es `ng build` en el CI, `estaticos::pruebas` sobre
el index empotrado, y `humo.sh` por socket. No cites `npm test` como verde.

## Pendiente conocido

- **Licencia offline — escrita, sin mergear.** No está en `main`, pero **no la
  escribas de nuevo**: vive en la rama `feat/licencia-offline-rust` (PR #17), son
  712 líneas de `licencia.rs` más interfaz, `hex.rs` y `DESPLIEGUE.md`, y su CI
  estaba en verde antes del corte de facturación. Mantiene el modo degradado:
  **nunca bloquea nada**. Solo espera turno de merge.
- **Volcado: el hash de contraseña (#231)** — igual, escrito y sin mergear, en
  `fix/volcado-comprueba-el-hash` (PR #16).

**Antes de dar por pendiente cualquier cosa de esta sección, mira las ramas y los
PR abiertos.** Dos de los tres «pendientes» de `main` son en realidad trabajo
terminado esperando el lunes, y el CI parado hace que se acumulen.

Es lo **único** pendiente, y `rust/README.md` dice lo mismo. Si encuentras otro
documento que dé por pendiente la inferencia, la firma, el planificador, la
subida o la importación, está desactualizado: las cinco están hechas.

## Documentos de negocio (no se editan sin pedirlo)

`EULA_BORRADOR.md`, `LICENCIAMIENTO.md`, `MODELO_LICENCIA_INFORMES.md`,
`CONTRATO_SERVICIOS_DESARROLLO.md`, `PRECIOS_PORTAFOLIO.md`, `ANCLAJE_PRECIOS.md`,
los análisis de minutas: trabajo jurídico de Juan. `verificar_clausulado.py`
comprueba que EULA y modelo de licencia no se contradigan. Despliegue en
`DESPLIEGUE.md`.
