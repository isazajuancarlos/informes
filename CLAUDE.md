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
    participacion.rs informes.rs flujo.rs estadisticas.rs
    alertas.rs planificador.rs firma.rs importar.rs estaticos.rs respaldo.rs
    pruebas_cronograma.rs   # solo #[cfg(test)]: el cronograma de punta a punta
    tests/cli.rs            # los subcomandos ejercidos como proceso, no como función
    migrations/  # 0001_inicial … 0006_freno_y_revocacion (sqlx::migrate!)
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
fallos sin castigo, luego 15 s doblando con techo en 5 min, y un acierto lo borra.
**No bloquea la cuenta** — un bloqueo deja fuera al usuario legítimo, que es peor
que el ataque que evita. La REGLA vive en `guaca::freno`, compartida con medico;
aquí solo están las dos consultas. El origen es la dirección del socket salvo que
se declare `SIGER_TRAS_PROXY=1`, y si entonces no llega `X-Real-IP` **el acceso
falla**: rellenarlo con la dirección del proxy degradaría el freno a contar solo
por cuenta.

Las tres respuestas de rechazo —correo inexistente, cuenta desactivada, clave
mala— cuestan **lo mismo**: cuando no hay usuario se verifica contra un hash
señuelo. Sin eso, el mensaje es idéntico y el cronómetro dice qué correos existen.

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

Angular 22 + `@angular/material` + `material-icons` **empaquetado**, sin una
sola petición a un CDN — el producto se instala sin internet, y la CSP del
binario es `script-src 'self'`. PrimeNG está prohibido (ver el general).
Siete páginas en `src/app/paginas/`, el cliente HTTP y la sesión en
`src/app/nucleo/`.

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
