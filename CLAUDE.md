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
la respuesta real, el freno del login frenando al sexto intento y el **login de
dos pasos completo** —enrolar, activar con el código que daría el teléfono, y
entrar tecleándolo—, más el cartel de la demostración: que el binario anuncie las
cuentas y que con lo anunciado se ENTRE. 40 comprobaciones.

Ese último añadido tiene su lección: hasta el 2026-08-02 las siete comprobaciones
del 2FA afirmaban solo que la puerta se CIERRA (sin sesión rechaza, la contraseña
sola no entrega cookie), y con eso un `verificar` que no emitiera el `Set-Cookie`
—o sea, nadie con segundo factor puede entrar— pasaba las 31 en verde. **Por cada
«X rechaza» hace falta su pareja «y con lo bueno, X ABRE», y que lo abierto
sirva** para entrar a otra ruta. Comprobado por mutación: quitando ese
`Set-Cookie` caen las dos comprobaciones nuevas y solo esas.

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

**La demostración pública anuncia sus credenciales, y ninguna otra instalación
puede hacerlo.** `GET /api/demostracion` (sin sesión, porque lo pide la pantalla
de acceso) devuelve lo que haya en `SIGER_DEMO_CUENTAS` —`correo:clave:nombre`,
separadas por `;`— y **una lista vacía en todo lo demás**. No existe un
`SIGER_DEMO=1`: dentro del binario no hay ninguna lista de cuentas «conocidas»
que alguien pueda encender, lo que se publica hay que escribirlo entero en el
entorno. Un valor a medias **no apaga el cartel en silencio**, impide arrancar
(directiva 20: un cartel mudo dejaría la demostración sin forma de entrar y
nadie se enteraría). En el VPS lo escribe `desplegar-demo.sh` desde
`sembrar_demo.py --anunciar`, que es la misma fuente que siembra las cuentas —
dos listas separadas acabarían anunciando una contraseña que ya no vale—, y
`humo.sh` lee lo anunciado y **entra con eso**.

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
(restablecer la de otro), `DELETE /api/cuentas/{id}/2fa` (quitarle el segundo
factor a otro) y `POST /api/mi-clave` (la propia, cualquier rol).
Pantallas en `angular/src/app/paginas/{cuentas,mi-clave}.ts`.

**Recurso aparte de `/api/usuarios` a propósito**, siendo la misma tabla:
`/api/usuarios` significa «quién puede ser parte de un flujo» y solo trae los
activos, porque lo consume el selector de partes. La administración necesita
también los desactivados —si no, reactivar a alguien exigiría `sqlite3`— y el
rol. Dos vocabularios sobre una fila; el valor lo fija quien ya lee.

Seis reglas que no se pueden relajar sin romper algo (las siete están cubiertas
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
5. **`DELETE /api/cuentas/{id}/2fa` no vale sobre uno mismo.** Es el rescate de
   quien perdió el teléfono **y** los diez códigos —hasta que existió, esa cuenta
   no tenía ninguna salida que no fuera `sqlite3`: desactivar el factor exige
   estar dentro, y restablecerle la contraseña no ayuda porque el segundo paso
   sigue en medio—. Pero si sirviera para el propio sería la puerta de atrás de
   la regla 6 de #212: una sesión robada de un administrador apagaría su segundo
   factor sin saber ninguna contraseña. El propio se quita desde `/api/2fa/
   desactivar`, que la pide. Y no es una grieta nueva: un administrador ya podía
   restablecer la contraseña de cualquiera. Lo que faltaba era poder hacerlo con
   su línea en la bitácora en vez de con un cliente de SQLite.

6. **El hash se pone al día en el acierto, y eso NO sube `token_version`.**
   `renovar_hash_si_hace_falta` reescribe el hash de quien acaba de entrar si el
   guardado se quedó por debajo de la política de hoy. La regla es **una sola
   frase, y tiene que seguir siéndolo**: *nunca se reescribe un hash que cueste
   MÁS trabajo (`m × t`) que el de hoy; se reescribe si cuesta menos, o si cuesta
   lo mismo y además es de una variante o versión anticuada.* Es el único momento
   en que se puede hacer, porque reescribir un hash exige la contraseña en claro.

   **La primera versión preguntaba cinco cosas por separado —algoritmo, versión,
   `m`, `t`, `p`— unidas por un OR, y eso DESTRUÍA hashes buenos**: un
   `Argon2id m=2GiB t=1 p=4` —la primera opción recomendada por el RFC 9106— tiene
   `t=1 < 2`, así que salía «por debajo» y se rebajaba a `m=19MiB t=2`: **53 veces
   menos trabajo, sin vuelta atrás y sin que nada se notara**, porque la cuenta
   seguía entrando y el log decía «puesto al día». Lo cazó la auditoría del propio
   cambio (2026-08-03) y es la directiva 23 en estado puro: el fallo estaba en la
   REGLA, y la prueba que decía vigilarla usaba un caso más fuerte en TODOS los
   ejes, así que nunca ejercía el mixto. Si alguien vuelve a tocar esto: el caso de
   prueba que importa es **más fuerte en conjunto, más flojo en un eje**. `p=` no
   entra en la cuenta —reparte el mismo trabajo, no lo aumenta— y la variante sola
   no justifica reescribir: `Argon2id` es mejor que `argon2i`, pero no vale dos
   órdenes de magnitud de coste. Quien quiera mejorar la variante de todo el padrón
   tiene el camino honesto: **subir `Params::DEFAULT`**, no rebajar a los demás
   para igualarlos.

   Tres cosas más que se rompen solas si alguien las "simplifica":
   - **el `UPDATE` va condicionado al hash viejo** (`AND clave_hash = ?`). Entre
     verificar y reescribir pasan un turno de semáforo y un Argon2id, y en ese
     rato la persona puede cambiar su contraseña desde otra pestaña: sin la
     condición se escribiría el hash de la contraseña VIEJA encima de la nueva, la
     nueva dejaría de servir y la vieja volvería a valer. Mismo patrón que el
     contador del TOTP y `usado_en IS NULL`;
   - **no toca `token_version`**, porque no es un cambio de contraseña sino la
     misma contraseña escrita de otra forma. Subirlo echaría a la persona de sus
     otras sesiones al entrar, sin que nada se lo explique — y contradiría la
     regla 2 solo en apariencia;
   - **no puede impedir entrar NI hacer esperar**: el acierto ya ocurrió. Con el
     Argon2 fallando o el `UPDATE` a cero filas, se anota y se sigue (modo
     degradado, #71). Y el turno de cripto se pide con `try_acquire`, **sin
     esperar** — es el único sitio del sistema que lo hace así: `con_turno` aguarda
     30 s porque en sus otros usos no hay respuesta que dar sin el Argon2, pero
     aquí la sesión ya está ganada y esperar dejaría medio minuto en blanco a quien
     ya tiene derecho a entrar, justo cuando hay más carga. Si no hay sitio, no se
     hace y el próximo acceso lo reintenta. Por eso no devuelve `Result`: un `?`
     convertiría una mejora silenciosa en un fallo de acceso.

   El caso que lo destapó fue el volcado (#231), que acepta cualquier Argon2
   válido, pero **el que de verdad importa es el general**: el día que se suban
   los parámetros del KDF, esto es lo único que pone al día las cuentas que ya
   existían — sin él, subir la política solo protegería a quien se dé de alta
   después.

El hash sale del **mismo** Argon2 que verifica el acceso, con turno del semáforo
de cripto (`ESPERA_CRIPTO`), como el login y el descifrado en reposo.

**Cuidado con las dos cifras de memoria, que no son la misma** (corregido el
2026-08-03, y estaba mal en siete comentarios): el Argon2 de **contraseñas**
—login, alta, `clave_correcta`, los códigos de recuperación— es
`Params::DEFAULT`, o sea **m=19456 (19 MiB), t=2, p=1, ~35 ms medidos en
release**. Los **64 MiB** son otra cosa: el Argon2id de `guaca::reposo`, que
descifra el archivo de una entrega o el secreto TOTP. Quien dimensione el
semáforo de cripto con 64 MiB para el login se equivoca por más de tres veces.

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
| `cuenta_creada`, `cuenta_actualizada`, `clave_restablecida`, `clave_cambiada`, `clave_propia_rechazada`, `2fa_retirado_por_admin` | administración de cuentas |
| `2fa_enrolamiento_iniciado`, `2fa_activado`, `2fa_activacion_rechazada`, `2fa_desactivado`, `2fa_desactivacion_rechazada`, `2fa_codigos_renovados`, `2fa_codigos_rechazados` | el segundo factor, desde la cuenta propia |
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
exige sesión, porque es el segundo paso del login. El sexto extremo no vive aquí
sino en `cuentas.rs`: `DELETE /api/cuentas/{id}/2fa`, el rescate del que perdió
el teléfono y los códigos (regla 5 de la administración de cuentas, arriba).

`codigos` renueva los diez de recuperación **sin tocar el secreto**, y exige la
contraseña por lo mismo que `desactivar`: entrega diez credenciales que entran en
lugar del segundo factor. Existe porque sin él la única forma de reponerlos era
desactivar y volver a enrolar —QR nuevo en todos los teléfonos y, sobre todo, un
rato con la cuenta protegida por la contraseña sola, que es el estado del que el
segundo factor viene a sacarla—. Quedarse sin códigos es lo normal: se gastan de
uno en uno, y cada uno se usa el día que el teléfono no está.

Siete cosas que se rompen solas si alguien las "simplifica":

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
   quema. Y se quema en el **mismo `UPDATE`** que comprueba que nadie lo quemó
   antes (`... AND (totp_ultimo_contador IS NULL OR totp_ultimo_contador < ?)`,
   mirando `rows_affected`), igual que `usado_en IS NULL` en los códigos de
   recuperación: entre leer el contador y escribirlo pasan el turno del semáforo
   y un Argon2id de 64 MiB descifrando el secreto, así que en dos sentencias dos
   peticiones simultáneas con el mismo código abrían las dos. Lo sujeta
   `el_mismo_codigo_a_la_vez_abre_una_sola_sesion`, y solo esa: sin la condición
   en el SQL, las otras trece pruebas del módulo siguen verdes.
4. **Activar exige teclear un código correcto Y la contraseña actual.** El código,
   porque activar al generar el QR dejaría fuera de su cuenta a quien tenga el
   reloj mal, y sin poder entrar a arreglarlo. La contraseña, porque encender el
   factor no quita protección pero **decide de quién es el teléfono que a partir
   de ahora manda**: sin ella, una sesión robada ataba el autenticador del ladrón
   a la cuenta ajena. No le daba acceso —el primer paso del login le sigue
   pidiendo la contraseña— pero dejaba al dueño fuera de la suya, con la única
   salida del rescate de la regla 5 de la administración de cuentas. Era la
   asimetría del revés: `desactivar` la pedía y `activar` no (hallazgo de la
   auditoría del 2026-08-03; se cerró el mismo día). Las dos cosas prueban cosas
   distintas y hacen falta las dos — el código, que el autenticador que se ata
   está delante de quien lo ata; la contraseña, que quien lo ata es el dueño—, y
   la primera se cumple igual de bien con el teléfono del ladrón. El orden es
   conflictos → contraseña → código: los conflictos son gratis y no dicen nada
   que la propia sesión no vea ya en `GET /api/2fa`, y así no se paga un Argon2id
   de 19 MiB por una petición que iba a ser rechazada igual.
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

## Licencia offline (`licencia.rs`)

Archivo firmado por el licenciante, verificado **en local** y sin red, con la
misma criptografía que sella las entregas. Los campos son los diez del Anexo A
del EULA, ni uno más: si el Anexo cambia, el módulo cambia con él.

**No bloquea nada. Nunca** — ni ausente, ni con el mantenimiento vencido, ni
alterada, ni con exceso de usuarios (invariante 2). Da visibilidad y evidencia;
el remedio ante el exceso es contractual. `veredicto` devuelve `Estado`, no
`Result`: no hay forma de que un llamante la convierta en un camino de error.

`GET /api/licencia` reparte en dos: el **aviso** (situación, mensaje, si es
grave) lo ve cualquiera con sesión —es la visibilidad que justifica el módulo—,
y los **datos del Anexo A** más el exceso de usuarios solo `Admin` o
`Supervisor`, porque llevan NIT, alcance contratado y el recuento exacto de
usuarios activos, que `/api/usuarios` ya le niega a un `Tercero`.

Tres detalles que parecen descuidos y no lo son:

- **`mantenimiento_hasta` NO es la vigencia.** El derecho de uso es perpetuo
  (3.1, 3.2, 7.6). Pasada la fecha se dice «mantenimiento vencido», nunca
  «licencia vencida»; y no lleva color de alarma, que solo merece `Invalida`.
- **El veredicto NO valida contra las listas del Anexo A; la emisión sí.** Un
  binario viejo que rechazara una edición nueva pintaría de rojo una licencia
  legítima y bien firmada. Quien dice si el documento es auténtico es la firma.
- **El reloj es UTC, y la zona horaria del cliente vive en `DIAS_DE_GRACIA`.**
  `mantenimiento_hasta` no es un instante: es una fecha de un contrato firmado en
  Colombia, y comparar con UTC a secas le quitaba al cliente las últimas cinco
  horas del día que pagó. La primera solución fue `chrono::Local`, y era peor:
  cuando el sistema no puede resolver su zona, `Local` **degrada a UTC en
  silencio** — la directiva 20 al revés. Así que `licencia::hoy` es
  `Utc::now().date_naive()` y la tolerancia es un día de gracia en la
  comparación, con dos aserciones en tiempo de compilación que lo acotan (cubre
  las 14 horas de desfase máximo y no pasa de dos días). Lo que se guarda firmado
  sigue siendo la fecha exacta del contrato.
  **Y la suma va `checked`**: `NaiveDate + Duration` entra en pánico al desbordar,
  y `%Y-%m-%d` acepta `+262142-12-31` —que `emitir` firma sin queja—, con lo que
  el handler moría y el aviso desaparecía en silencio. Al desbordar se toma «no
  vencido», que es la dirección que nunca empeora la situación del cliente.

El par de claves del licenciante es DISTINTO del de las entregas
(`generar-claves-licencia`), y `INFORMES_LICENCIA_SK` no se acepta por argumento
—quedaría en el historial del intérprete y a la vista de `ps`—. Despliegue y
emisión, en `DESPLIEGUE.md`.

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

- **Volcado: el hash de contraseña (#231)** — igual, escrito y sin mergear, en
  `fix/volcado-comprueba-el-hash` (PR #16). Éste sí entra limpio: medido el
  2026-08-03 contra el `main` de después de #19, sin un solo conflicto.

**Antes de dar por pendiente cualquier cosa de esta sección, mira las ramas y los
PR abiertos.** El 2026-08-03 entraron en `main` la administración de cuentas, el
segundo factor, el hash del volcado (#231) y **esta licencia**: si un documento
los da por pendientes, está viejo.

Es lo **único** pendiente, y `rust/README.md` dice lo mismo. Si encuentras otro
documento que dé por pendiente la inferencia, la firma, el planificador, la
subida o la importación, está desactualizado: las cinco están hechas.

## Documentos de negocio (no se editan sin pedirlo)

`EULA_BORRADOR.md`, `LICENCIAMIENTO.md`, `MODELO_LICENCIA_INFORMES.md`,
`CONTRATO_SERVICIOS_DESARROLLO.md`, `PRECIOS_PORTAFOLIO.md`, `ANCLAJE_PRECIOS.md`,
los análisis de minutas: trabajo jurídico de Juan. `verificar_clausulado.py`
comprueba que EULA y modelo de licencia no se contradigan. Despliegue en
`DESPLIEGUE.md`.
