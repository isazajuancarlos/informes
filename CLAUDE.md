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
en la historia de git. Hoy es **un solo binario** con **el panel compilado dentro**
—`maud` + htmx desde el 2026-08-06, antes Angular— y **SQLite** como base. No queda `app/`,
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
  siger-api/    # binario axum + panel en maud/htmx + SQLite (SQLx)
    main.rs rutas.rs db.rs config.rs sesion.rs autz.rs
    acceso.rs origen.rs  # freno del login: las dos consultas y de dónde viene quien pide
    segundofactor.rs  # 2FA: las columnas. La REGLA es `guaca::dosfactores`
    cuentas.rs participacion.rs informes.rs flujo.rs estadisticas.rs
    alertas.rs planificador.rs firma.rs importar.rs respaldo.rs
    vista.rs     # EL PANEL: 35 rutas de `maud`, ~2.200 líneas
    pruebas_cronograma.rs   # solo #[cfg(test)]: el cronograma de punta a punta
    tests/cli.rs            # los subcomandos ejercidos como proceso, no como función
    migrations/  # 0001_inicial … 0007_segundo_factor (sqlx::migrate!)
    estaticos/   # los 5 activos VENDORIZADOS y versionados: htmx.min.js,
                 # boi.js, panel.css y DOS comportamientos de boi —
                 # comportamientos/{confirmar-doble,calendario-multiple}.js,
                 # vendorizados sin tocar. Entran por include_str!
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
cargo build --release            # produce target/release/siger-api, panel incluido
cargo test  --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings   # el CI usa -D warnings


./rust/construir.sh              # el ARTEFACTO: binario + la pantalla DIBUJADA
./rust/humo.sh                   # y probarlo por HTTP, como lo prueba el mundo
python rust/sembrar_demo.py      # base de demostración en rust/demo/siger.db
```

**Correr UNA prueba** (el nombre es un filtro por subcadena, no una ruta):

```bash
cd rust
cargo test -p siger-api --bin siger-api -- --list        # los 98 nombres, para elegir
cargo test -p siger-api rutas::pruebas_acceso           # el freno del login entero
cargo test -p siger-api --bin siger-api vista::         # un módulo
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
El CI (`.github/workflows/ci.yml`) tiene **tres** trabajos desde el
2026-08-06: `rust` (test + clippy), `cargo-audit (RustSec)` y `cargo-deny
(supply chain)`. El cuarto, `frontend (ng build)`, se fue con Angular — y con él
**Node desaparece del build entero**.

**Ese `ci.yml` YA NO LO EJECUTA NADIE, y conviene saberlo antes de tocarlo**
(medido el 2026-08-11). Bajo el protocolo «solo local» —permanente desde el
2026-08-10— Actions está apagado, el repositorio de GitHub está vaciado y
`origin` es `/home/juan/espejos/informes.git`. El archivo se conserva porque es
la **fuente de la que `verde_local.py` lee los pasos**: los ejecuta con el mismo
`bash -eo pipefail` que usaba GitHub, así que no envejecen por estar copiados.
El verde de este repositorio es:

```bash
python3 /mnt/data/conocimiento/cupo/verde_local.py informes
```

que corre los 4 pasos y **dice siempre lo que NO reprodujo** —los `uses:` del
runner y el `apt-get` de Ubuntu—, para que «verde» no se confunda con
«completo».

### El banco de navegador alcanza las pantallas CON sesión (2026-08-08)

Hoy es la librería **`mirilla`** (MIT, pública, fijada por `rev`) — un cliente
**CDP por tubería** que abre Chromium con `--remote-debugging-pipe` y le habla
por los descriptores 3 y 4. Nació aquí como `tests/comun/mod.rs`, copiado de
`medico` sin generalizar a propósito, y se extrajo el 2026-08-10 con el segundo
consumidor delante.

Hasta ese día el banco solo llegaba a las pantallas **sin sesión**, y estaba
escrito por qué: `chromium --dump-dom` no admite mandar una cookie, y la única
forma habría sido abrir una ruta que autentique por GET —un agujero real a cambio
de una prueba—. Con CDP la sesión no se inyecta: se **ENTRA por el formulario**,
como entra una persona, así que el acceso queda medido de paso.

Lo que se gana, y no es poco: `data-boi="listo"` **dentro del panel**. Los dos
botones de confirmación doble —«Quitar 2FA» y «Desactivar»— dependen de que ese
módulo corra, y si no corriera actuarían a la PRIMERA pulsación devolviendo
exactamente el mismo 200 con el mismo cuerpo. Tres pruebas nuevas, con sus dos
parejas: sin sesión acaba en la entrada, y una clave equivocada no entra.

**Viene de `medico`, copiado SIN generalizar a propósito** (es la tarea #9 de
aquel repositorio): extraer una pieza compartida antes de que exista el segundo
consumidor es adivinar la costura. Con el segundo delante ya se ve dónde está, y
el dato es mejor de lo que la ficha suponía: **de sus ~309 líneas, UNA es de este
producto** —la ruta de la entrada, `/entrar` aquí y `/panel/entrada` allá—. Ni el
arranque del binario ni la siembra entraron ahí; se quedaron en `pantalla.rs`,
donde ya estaban. O sea que la pieza compartida no hay que partirla en
«arrancar / sembrar / entrar»: hay que partir **«hablar CDP»** de **«dónde está
la entrada»**.

Lo que sí duele y hay que decidir antes de extraer: **la licencia**. medico es
AGPL y este repositorio es comercial. Los dos son de Isaza Arenas, así que
relicenciar es del autor y no hay nada que pedir a nadie — pero la pieza común
necesita su licencia decidida, como la necesitó `boi`.

**«El CI no dibuja la pantalla» fue FALSO entre el 2026-08-06 y el 2026-08-10, y
salió caro.** Las 7 pruebas de `tests/pantalla.rs` no llevaban `#[ignore]` y el
runner `ubuntu-latest` trae `google-chrome-stable`, así que el CI **sí** las
corría; y `dibujar()` lanzaba el navegador con `.output()` **sin plazo de
proceso**, de modo que un headless que no sale nunca colgaba la corrida —**49
minutos medidos**, y fue uno de los culpables de agotar el cupo el 2026-08-07—.

**Cerrado el 2026-08-10 (#582), en cuatro piezas y con su pareja**: `#[ignore]`
en las 7, `--include-ignored` en `construir.sh` —que pasa a ser el ÚNICO sitio
donde el banco se ejecuta—, `timeout-minutes: 20` en los tres trabajos y un
plazo de proceso de 60 s en `dibujar()`, con hilo centinela y sin dependencias
nuevas. La guarda que ya tenía `construir.sh` —exigir `[1-9]* passed`— es la que
impide que quitar la bandera deje el paso 2/2 verde sin dibujar nada.
*(La coletilla «no se empuja hasta el 2026-09-01» que llevaba esta línea
caducó: no había cupo que esperar, y la rama entró en `main` el 2026-08-11.)*

**Lo que el CI no comprueba, y por eso existen los dos guiones de
arriba:** DIBUJAR la pantalla. Que el HTML compile no dice si se abre — los `hx-swap` los
ejecuta JavaScript en el navegador, y un fragmento correcto insertado en el sitio
equivocado devuelve el mismo 200 con el mismo cuerpo. `construir.sh` corre el
banco de navegador sobre el binario de release (paso 2/2, y exige que se haya
EJECUTADO, no solo que salga 0), y `humo.sh` arranca ese binario contra una base
temporal y le habla por socket: la raíz redirigiendo al panel, los tres activos
—htmx, `boi.js`, `panel.css`— viajando DENTRO, las cabeceras en
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

**`cargo build --release` a secas YA BASTA** (2026-08-06). El panel es código:
el HTML lo compilan las macros de `maud` y los cuatro activos entran por
`include_str!` desde `estaticos/`, que está versionado. Si un activo cambia,
`rustc` recompila; si falta, **no compila**. El binario no puede salir
desincronizado de su interfaz, así que se fueron `build.rs`, `rust-embed`, el
paso de npm y la bandera `SIGER_SIN_INTERFAZ`.

**Y ese `include_str!` destapó un defecto de verdad, que conviene no repetir.**
El `.gitignore` traía `rust/siger-api/estaticos/*` —escrito cuando ahí caía la
salida de `ng build`, donde era correcto— y se tragó dos activos del panel:
`comportamientos.js` y `panel.css` **no estaban en git**. Medido extrayendo el
árbol con `git archive` a otro directorio y compilándolo:
`couldn't read .../comportamientos.js`. En local todo salía verde porque los
archivos existen en disco, y el CI no lo vio porque solo corre en PR contra
`main` y este trabajo no tenía PR abierto. Se arregló **quitando la regla**, no
añadiendo excepciones: esa carpeta ya no guarda artefactos sino fuente
vendorizada, y con ella versionada un archivo que falte sale en `git status` el
mismo día.

Subcomandos del binario: `importar <origen> <destino>` (vuelca una base del
Python anterior), `importar-secop <csv> <db_destino> [--periodicidad mensual]`
(siembra el calendario de obligaciones desde una FOTO de contratos de SECOP II —
un CSV descargado a mano, no una integración viva; #608), `alertas` (una pasada, para un timer),
`respaldar <carpeta>` / `restaurar <carpeta> [--forzar]` (base en caliente vía
`VACUUM INTO` + los archivos de entrega, y la copia **se verifica** antes de
darse por buena), `generar-secreto`, `generar-claves-firma`, `hashear` (Argon2,
mismos parámetros que el acceso), `revisar-hashes` (cuántas cuentas están por
debajo de la política del KDF y por qué eje). Todos leen del entorno **dónde** está la base:
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

   **Degradar con gracia no es degradar en silencio, y hasta el 2026-08-11 aquí
   era lo segundo** (#604). Con el correo caído, `siger-api alertas` imprimía
   «120 fallidas» y salía con **código 0**, así que un temporizador quedaba en
   verde para siempre mientras nadie recibía el aviso que el producto vende; y
   con `SMTP_DESDE` ilegible era peor, porque la pasada devolvía un resumen de
   **ceros idéntico** al de un día tranquilo. Ahora `Resumen::incompleta()` es la
   regla en una frase, toda pasada deja su línea en la bitácora y el subcomando
   sale con fallo. **Nada de eso bloquea**: informa. Las entregas siguen sin
   marcar y el siguiente pase las reintenta, que es lo que exige esta invariante.
3. **Ante la ausencia de un dato, se falla ruidosamente; nunca se sustituye por un
   defecto.** `config.rs` no trae valores por defecto para secretos.

## Administración de cuentas (`cuentas.rs`)

`GET/POST /api/cuentas`, `PUT /api/cuentas/{id}`, `POST /api/cuentas/{id}/clave`
(restablecer la de otro), `DELETE /api/cuentas/{id}/2fa` (quitarle el segundo
factor a otro) y `POST /api/mi-clave` (la propia, cualquier rol).
Pantallas del panel: `/cuentas` y `/mi-clave` (`vista.rs`).

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

   **El caso que no se puede arreglar se CUENTA, no se avisa en cada acceso.** Un
   hash con menos memoria que la política pero más trabajo total no lo domina hoy:
   reescribirlo pierde memoria y dejarlo lo deja por debajo, así que no se toca. Eso
   se anotaba con `warn!` en cada entrada de las mismas cuentas y **nada cierra ese
   estado**, así que la advertencia sonaba para siempre — una alarma que no se puede
   resolver es una que a la tercera semana nadie mira, aunque sea cierta
   (directiva 23). Ahora va en `debug!`, y el recuento del padrón —que es la
   pregunta de verdad: «¿puedo subir `Params::DEFAULT` sin dejar a nadie atrás?»—
   vive en el subcomando **`revisar-hashes`**, que se consulta cuando se decide y
   cubre **los dos** cuadrantes no dominados. El otro, «más memoria y menos trabajo»,
   no aparecía en ninguna parte: su déficit está acotado por debajo de 2× mientras
   la memoria está por encima, así que no es más débil en conjunto — pero «no sale
   en ningún sitio» y «no pasa nada» no son lo mismo. `revisar-hashes` sale distinto
   de cero **solo** por los hashes ilegibles, que sí son una avería: ésos no pueden
   entrar y el login les responde lo mismo que a una clave mala.

   El caso que lo destapó fue el volcado (#231), que acepta cualquier Argon2
   válido, pero **el que de verdad importa es el general**: el día que se suban
   los parámetros del KDF, esto es lo único que pone al día las cuentas que ya
   existían — sin él, subir la política solo protegería a quien se dé de alta
   después.

El hash sale del **mismo** Argon2 que verifica el acceso, con turno del semáforo
de cripto (`ESPERA_CRIPTO`), como el login y el descifrado en reposo. Desde el
2026-08-04 «el mismo» es literal: quien calcula y quien verifica es
**`guaca::claves`** —una función, no seis copias—, y lo que pone SIGER encima es
solo lo que guaca no puede saber, el turno y el `spawn_blocking`. Antes eran
**seis** sitios con las mismas tres líneas: `cuentas::{hashear, clave_correcta,
renovar_hash_si_hace_falta}`, el subcomando `hashear` de `main.rs`, **la
verificación del login en `rutas.rs`** y **el hash señuelo** — coincidían por
casualidad de seis literales iguales, que es el modo de fallo del que nació
guaca. De `argon2` aquí solo se LEE: `PasswordHash`, para analizar el PHC
guardado (la política, y el paso previo del login).

Los dos últimos no los vio quien escribió el cambio, los cazó `security-review`
al auditarlo: el grep que dio por completa la lista salió truncado por un
`head -40` y el negativo pareció una medición (directiva 30). El señuelo importa
especialmente porque su único trabajo es **costar lo mismo** que un hash real, y
la alineación estaba escrita como «los dos llaman a `Argon2::default()`» — dos
literales, no un mecanismo.

**Y el porqué que este archivo y tres comentarios daban estaba al revés.** Decían
que un hash con otros parámetros «deja a alguien fuera del sistema». No deja a
nadie fuera: un PHC de Argon2 lleva su `m`, su `t` y su `p` dentro, y el
verificador los lee de ahí, así que un hash `m=8 t=1` **entra igual de bien**.
Medido por mutación el 2026-08-04 en el subcomando `hashear`: las 11 pruebas del
CLI siguieron verdes, la de «sale un `$argon2`» incluida. El daño es el
contrario y no tiene síntoma —el primer administrador del cliente protegido por
un hash 2400 veces más barato, y nada lo delata—, y lo caza ahora
`el_hash_del_cli_abre_la_puerta_y_cuesta_lo_que_debe` (`tests/cli.rs`), que
compara los parámetros del hash impreso contra `Params::DEFAULT`. Lo que sí deja
a alguien fuera, y es ruidoso, es cambiar de **algoritmo o de formato**: eso lo
sujeta la otra mitad de la misma prueba.

Los otros dos sitios sí tenían quien los cazara, comprobado por mutación:
invertir los argumentos de `verificar` en el login pone **10** pruebas de
`pruebas_acceso` en rojo, y abaratar el señuelo a `m=8 t=1` cae
`un_correo_inexistente_cuesta_lo_mismo_que_uno_existente` —y solo ella—, que es
exactamente lo que ese hash existe para impedir.

**`sesion.rs` YA SE DEDUJO (2026-08-31, rama `sesion-doc-veraz`)**: el módulo pasó
a ser la capa de cookies axum sobre `guaca::sesion`. El relato viejo —«la copia
débil es la de la librería, guaca emite `id|caducidad` sin `token_version`»— era
cierto hasta guaca 0.6 y **falso desde la 0.7**: guaca subió `version` y el dominio
de firma DESDE estas apps y encima los endureció (prefijo de longitud del dominio
en `firmar_dominio`, que esta copia no tenía). Hoy guaca es un superconjunto
estricto, así que `emitir`/`verificar`/`{emitir,verificar}_pendiente` delegan en
`guaca::sesion::{emitir,verificar}` y `{emitir,verificar}_en` con el dominio
`"pendiente-de-segundo-factor|"`. La revocación (#194) y la separación de dominio
del 2FA se conservan —las da guaca—.

**AVISO DE DESPLIEGUE: este cambio cierra la sesión de TODOS, una vez.** El prefijo
de longitud cambia la firma también con dominio vacío —guaca firma
`HMAC([8 ceros]‖carga)`, la copia firmaba `HMAC(carga)`—, así que las cookies vivas
no verifican y todos re-entran (medido con el vector fijo real de guaca; caso rojo
`una_cookie_del_esquema_viejo_no_entra`). No es regresión (guaca es más estricto):
es el coste elegido del logout único frente a una ventana de doble verificación
(D1/D37). Se despliega AGENDADO y ANUNCIADO. Medico sigue con su copia (parser
distinto: su compatibilidad se mide aparte).

**guaca se fija por `rev`, y el `rev` NO es el del tag `v0.4.0`.** Lo que se
vendoriza al construir es un ÁRBOL, no un número de versión, y el árbol de ese
tag no lleva `LICENSE` en la raíz: el binario acababa incorporando una copia de
guaca que no declara bajo qué licencia se puede usar (la declaración del
`Cargo.toml` y el NOTICE no sustituyen al archivo). El 2026-08-04 el pin pasó a
`d786d1c`, un commit después del tag, que solo añade el LICENSE y toca el CI de
guaca —ni una línea de `src/`, comprobado con `diff -r` sobre los dos árboles
descargados—.

**Hoy el pin es `161a6876` —guaca `v0.6.0`— y va con `quipu = "0.11"` DIRECTO.**
*(Medido el 2026-08-12 en `rust/siger-api/Cargo.toml:91`. Lo movió `b25028a` ese
mismo día —«guaca sube a v0.6.0: un solo árbol para los tres»—, y lo movió **otra
instancia**: la de medico, que subió el segundo factor a guaca y no podía dejar a
informes en el `rev` viejo sin partir la regla compartida en dos versiones. Este
párrafo se quedó con el `23395f3` de antes. Directiva 36 en estado puro: lo que
otra sesión escribe no llega sola a la ficha de ésta.)*

**MOVER ESE `rev` EXIGE CORRER EL BANCO DE GUACA Y DECIR EL RESULTADO**, con
`bash rust/pruebas-de-guaca.sh` — que lee el `rev` del `Cargo.toml`, saca ese
árbol del espejo y corre su banco entero (78 pruebas hoy; **la orden tarda ~1
minuto**, porque compila guaca desde cero — los 2 s son solo la ejecución). No es
ceremonia: desde que se podaron `dosfactores` y `qr`, **la corrección del
codificador de QR ya no se mide aquí, ni la regla de la ventana del TOTP a nivel
UNITARIO** —esta última sí se mide por el extremo HTTP, con
`un_codigo_de_fuera_de_la_ventana_no_abre_sesion`, que caza un ensanche de la
ventana a 2; lo que no se mide aquí es el resto del algoritmo—. Y un
QR mal generado no falla — sale un dibujo plausible que ningún teléfono lee, así
que pasaría los dos `grep` estructurales de `humo.sh` y se vería perfecto en
pantalla. Un `rev` que rompiera el codificador entraría con las 246 pruebas de
este repositorio en verde. «Compila y todo pasa» NO sustituye a esto.

El pin y la versión de quipu se mueven en el mismo commit, y por eso: guaca ya
pide quipu 0.11, así que subir solo uno de los dos dejaría **dos entradas de
quipu** en el `Cargo.lock` y dos pilas cripto dentro del binario. El control es
`grep -c '^name = "quipu"$' rust/Cargo.lock`, tiene que dar **1**, y desde el
2026-08-12 **ya no depende de que alguien lo recuerde**: lo exige `deny.toml` con
`deny-multiple-versions` sobre `quipu`, que corre en el paso `cadena-suministro`
del verde local. Es el mismo árbol de guaca que fija tunjo. Al salir un tag que
incluya el LICENSE, volver a fijar el tag.

**Y desde el 2026-08-11 las dos dependencias `git` —`guaca` y `mirilla`— salen
del ESPEJO LOCAL, no de GitHub**: `git = "file:///home/juan/espejos/<repo>.git"`,
con el mismo `rev` de siempre. No es cosmética. El protocolo «solo local» movió
el origen de verdad a `/home/juan/espejos` y **vació** los repositorios de
GitHub, así que los `rev` fijados allí son **commits huérfanos**: no los alcanza
ninguna rama y solo se descargaban porque GitHub sirve un SHA arbitrario
mientras el objeto exista. El día que su recolector se los llevara, esto dejaba
de compilar sin aviso previo y en la fecha que decidiera otro. Probado, no
razonado: retirada la caché de git de cargo, el build se bajó de `file://` y
terminó.

**El precio, dicho entero: el build queda atado a esta máquina.** Para SIGER
—privado, comercial, compilado solo aquí— es coherente; quien clone este
repositorio en otra máquina no podrá construirlo sin los espejos. `mirilla`
además es `dev-dependency`, así que **no entra en el binario de producción**.

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
| `entrega_ajena_solicitada` | alguien pidió una entrega que EXISTE y no le corresponde. **SEIS OPERACIONES reciben un id de entrega —diez entradas de ruta, porque cinco tienen gemela en el panel—; CINCO anotan y una está exenta con motivo** (revisado el 2026-08-16). Anotan: descarga, verificación, justificación, **subida** y **el hilo de comentarios** —éste por sus dos extremos, `GET /api/entregas/{id}/comentarios` y la pantalla `GET /entregas/{id}`—. La exenta es `revisar_de`: su guardia es de ROL y es constante respecto del id, y quien la pasa tiene `Alcance::Todas`, para quien «ajena» no existe. **La cuenta importa**: este archivo llegó a afirmar «las CUATRO puertas» el mismo día, y era falso — un veredicto de completitud escrito aquí es lo que hace que nadie vuelva a mirar. Dos detalles que no se simplifican: lo ajeno en la subida es la ACCIÓN y no la ENTREGA —un tercero que sí ve la entrega no puede subirla, y anotárselo le fabrica un sondeo que no cometió—; y el hilo se anota en los MANEJADORES y no en `comentarios_de`, porque `entrega_subir` redibuja la pantalla tras un fallo y la misma petición dejaría dos líneas. Casos rojos: `subir_a_una_entrega_ajena_tambien_se_anota`, `al_tercero_que_si_ve_la_entrega_no_se_le_anota_por_intentar_subirla` y `leer_el_hilo_de_una_entrega_ajena_tambien_se_anota` |
| `cuentas_listadas` | se leyó el padrón completo de personas, con cuántas filas y sin ids |
| `acceso`, `acceso_fallido`, `acceso_frenado`, `salida`, `salida_total` | la puerta |
| `cuenta_creada`, `cuenta_actualizada`, `clave_restablecida`, `clave_cambiada`, `clave_propia_rechazada`, `2fa_retirado_por_admin` | administración de cuentas |
| `2fa_enrolamiento_iniciado`, `2fa_activado`, `2fa_activacion_rechazada`, `2fa_desactivado`, `2fa_desactivacion_rechazada`, `2fa_codigos_renovados`, `2fa_codigos_rechazados` | el segundo factor, desde la cuenta propia |
| `acceso_falta_segundo_factor`, `2fa_fallido`, `2fa_codigo_reutilizado`, `2fa_recuperacion_usada`, `2fa_frenado`, `2fa_secreto_ilegible` | el segundo paso de la puerta |
| `informe_creado`, `informe_actualizado`, `participacion_actualizada`, `justificacion_*`, `entrega_registrada`, `entrega_contenido_no_concuerda` | el flujo |
| `alertas_pasada`, `alertas_incompletas` | **cada** pasada de alertas, fuera bien o mal (ver abajo) |
| `planificador_fallido` | la generación de entregas falló — es lo que CREA las obligaciones, así que su fallo pesa más que el de una alerta |
| `justificacion_sin_avisar` | la justificación se guardó pero no se pudo avisar a supervisión — y ése es el ÚNICO canal por el que se entera |

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
3. **La aritmética de fechas del planificador y de las alertas va `checked`, y
   `ALERTAS_DIAS_PROXIMIDAD` está acotado en el arranque.** No es endurecimiento:
   `arrancar_reloj` es un `tokio::spawn` sin supervisor, así que un pánico ahí
   dentro **mata el reloj para siempre sin tumbar el servidor** — y un pánico es
   la única forma de fallo que la bitácora no puede registrar, ni siquiera con la
   fila diaria del punto de abajo. Los dos caminos eran alcanzables: `+262142-12-31`
   es `NaiveDate::MAX` y atraviesa `importar::solo_fecha` porque es un punto fijo
   de parsear y volver a formatear, y `numero()` no acotaba nada. La lección ya
   estaba escrita en este archivo para `licencia.rs` y no se había aplicado aquí.
4. **La pasada de alertas se anota SIEMPRE, también cuando va bien**, y no
   contradice al punto 3: aquello era una fila por cada pintado de pantalla y
   esto es **una fila al día**. Sin la línea buena, una tabla sin eventos de
   alertas significa a la vez «no había a quién avisar» y «el reloj lleva tres
   semanas muerto» (directiva 30). `alertas_incompletas` cubre los dos modos de
   fallo —envíos que fallaron, y la pasada que **no llegó a intentarlo** porque
   la configuración del correo es ilegible—; `sin_destinatario` NO enciende esa
   alarma, porque es un dato que le falta al cliente, nada lo cierra, y una
   alarma que no se puede apagar deja de mirarse (directiva 23).
5. **`listar_entregas`, `/api/usuarios` y `/api/stakeholders` NO se anotan.** Los
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

## El segundo factor (`guaca::dosfactores`, `segundofactor.rs`, #212)

TOTP (RFC 6238), 6 dígitos, paso de 30 s, **una** ventana de tolerancia a cada
lado. El reparto es el mismo que el del freno: la REGLA en **`guaca::dosfactores`**
—sin `sqlx`, sin `Config`, sin ningún tipo de SIGER— y las columnas en
`segundofactor.rs`.

**La regla SALIÓ de aquí y volvió como dependencia** (2026-08-12). Nació el
2026-08-02 escrita para mudarse, subió a guaca el día que medico necesitó lo
mismo —ni antes: la directiva 9 dice que sube lo que YA se repitió en un segundo
proyecto, nunca por previsión— y ese mismo día se podó la copia local: eran
**381 líneas idénticas** a las de guaca salvo comentarios y un literal de prueba.
Con las dos vivas, el estado era el que la #212 nombraba como peligro —«escrito
dos veces, dentro de un año una tendrá el antirreplay y la otra no»—, y no es
hipotético: la revisión de seguridad de este repositorio encontró que el contador
antirreplay se quemaba en una sentencia aparte y tres peticiones simultáneas con
el mismo código abrían tres sesiones.

Las **10 pruebas** de la regla se fueron con ella y viven en guaca, idénticas. En
el conteo de aquí eso se ve como 262 → **252**, y la resta es exactamente ésa: no
dejó de correr nada más. Lo que sigue midiendo el TOTP por el camino real son las
pruebas de `segundofactor.rs`, que lo ejercen contra la base y contra los
extremos HTTP —`el_mismo_codigo_a_la_vez_abre_una_sola_sesion` entre ellas—.

**`qr.rs` se podó a continuación, el mismo día**, con la misma medición: 945
líneas idénticas a `guaca::qr`, misma API pública (`MARGEN`, `NoCabe`, `Codigo`,
`oscuro`, `a_svg`, `codificar`) y sus **7 pruebas** presentes en guaca. Con ella
se fueron `tests/oraculo_qr.json` y `tests/generar_oraculo_qr.py`, que sin el
código que miden solo aparentarían cobertura. El conteo bajó 252 → **245**, y la
resta es exactamente esas 7.

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

Ocho cosas que se rompen solas si alguien las "simplifica":

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
7. **El secreto ilegible NO tumba los códigos de recuperación**, y el orden es
   toda la regla. `verificar` devolvía 503 antes de llegar a
   `gastar_codigo_recuperacion`, así que una frase de reposo rotada dejaba sin
   servir los diez códigos del papel **justo el día que hacían falta**: la avería
   que obliga a rescatar tumbaba también el mecanismo de rescate, y la única
   salida quedaba en manos de un administrador. Un código de recuperación es un
   hash Argon2 en **otra tabla** y no necesita el secreto para nada. Los tres
   caminos por los que el secreto puede faltar —sin blob, sin frase o blob que no
   descifra, y secreto que no es base32— acaban en el mismo `SecretoIlegible`,
   porque para este paso significan lo mismo.

   **Y lo que decide si el fallo se cobra es el FORMATO**
   (`dosfactores::parece_codigo_de_recuperacion`): con forma de código de
   recuperación —ocho, guion, ocho— se comprueba de verdad contra la tabla, así
   que fallar cuenta para el freno; cualquier otra cosa no se pudo comprobar y no
   se cobra. Sin esa distinción el estado de avería sería **un carril para ensayar
   códigos de recuperación sin freno**, y con ella un TOTP de seis dígitos sigue
   recibiendo 503 sin que se le cobre nada a quien no tiene culpa.

`Veredicto` tiene **cuatro** estados por la misma razón que `Documento`:
`SecretoIlegible` es una avería del despliegue y no se le cobra al usuario **por
lo que no se pudo comprobar** —un TOTP no cuenta como fallo para el freno ni se le
dice que su código está mal—, pero sí por lo que sí: ver el punto 7. Y
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

En el panel son **dos pantallas**: `/dos-pasos` (estado, enrolamiento con QR,
códigos de recuperación y baja con contraseña) y `/entrar/codigo`, el segundo
paso del acceso. La entrada está en el menú de la cuenta, junto a «Cambiar mi
contraseña». El segundo paso es una PÁGINA aparte y no un formulario que aparece
—el servidor devuelve un 303 a `/entrar/codigo`—, así que las credenciales no
siguen en ninguna parte: lo que el navegador lleva en ese momento es la cookie
pendiente y nada más.

### El QR se dibuja en el servidor (`guaca::qr`)

Un codificador propio —modo byte, nivel M, versiones 1..40, máscara por
penalización— que devuelve **SVG**, y el enrolamiento lo manda ya dibujado en
`qr_svg`. **Vive en guaca desde el 2026-08-12**, con el mismo argumento que
`dosfactores`: aquí quedaban 945 líneas idénticas a las suyas, y una copia de un
codificador es peor que una copia de cualquier otra cosa —un QR mal generado no
falla, sale un dibujo plausible que ningún teléfono lee, así que el día que las
dos copias se separen nadie lo va a notar mirando la pantalla—. No hay librería de QR ni en npm ni en Rust: la *feature* `qr` de
`totp-rs` arrastra `image` a un binario offline, y una de npm son ~20 KB en el
paquete inicial para una pantalla que cada cuenta abre una vez.

**Cómo se sabe que está bien, que es lo único difícil de esto**: un QR mal
generado no falla, sale un dibujo plausible que ningún teléfono lee. Se mide
contra `qrencode` (libqrencode) con un oráculo dorado —hoy en guaca:
`tests/oraculo_qr.json` y su generador se fueron con el módulo, porque un oráculo
sin el código que mide es un archivo huérfano que aparenta cobertura— comparando **módulo a módulo las
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

**La fecha de trabajo está acotada a 1900..=9999, y es una guardia, no un
adorno** (`siger_core::fechas::en_rango_de_trabajo`, desde el 2026-08-11).
`chrono` acepta hasta `+262142-12-31` —`NaiveDate::MAX`—, que **parsea y es punto
fijo de ida y vuelta**, así que atraviesa igual una petición HTTP y el validador
del volcado. Cerca de ese extremo el motor entra en PÁNICO por tres sitios:
`dias_del_mes` cae a su `unwrap_or(28)` y el `position().unwrap()` de
`ordinal_en_el_mes` revienta; `Calendario::de_pais(_, año-1, año+1)` construye el
año 262143, que no existe; y `vencimientos_entre` desborda al incrementar.

Lo que se veía desde fuera era peor que un 500: hyper se lleva la tarea de
conexión, así que el cliente recibe **respuesta vacía**, y en el panel htmx no
tiene nada que intercambiar — la pantalla se queda como estaba y no aparece
ningún error. El proceso sobrevive, o sea que tampoco se cae el servicio.

**La guardia va en la ENTRADA** y no en cada `unwrap`: así se cierran los sitios
de una vez en vez de repartir `checked_*` por el motor. Y rechaza lo IMPOSIBLE,
no lo inusual: un vencimiento en el año 262142 no es un cliente raro, es un dato
corrupto.

**Pero «la entrada» son DOS, y la primera versión solo cerró una.** Las fechas
sueltas —`lecturas_de` con los días marcados, `previsualizar_de` con `desde` y
`hasta`— y las que viajan **dentro de la regla**: `fecha_inicio` y `fechas[]`.
Ésas últimas entran por cuatro puertas —crear un informe, editarlo,
previsualizar y volcar— y se acotan en el **embudo único** por el que se
convierten en `NaiveDate`: `compat.rs::TryFrom<&ReglaJson> for Regla`.

**Y la fecha peligrosa no es la que parece.** `+262142-12-31` sale por el portón
de `vencimientos_para` (`fecha < vigencia`) sin tocar nada; la que revienta es
**`-262143-01-01`**, cuatro días por encima de `NaiveDate::MIN`, con una regla
semanal de cada N semanas: el ancla resta días para buscar el lunes de esa
semana y desborda por abajo. Guardada, esa regla **mataba el reloj del
planificador en cada arranque** mientras `/api/salud` seguía respondiendo 200.

Una fecha **ilegible** sigue tratándose como vigencia ausente, y una **imposible**
es `Err`: son dos cosas distintas y tratarlas igual convertiría «desde el año
-262143» en «sin vigencia», que produce un calendario de entregas distinto y en
silencio.

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

## El panel (`vista.rs` + `estaticos/`)

**`maud` + htmx, servido por el mismo binario** (directiva 26, desde el
2026-08-06). Sustituye al SPA de Angular 22 + Material, que se retiró entero ese
mismo día: `angular/` (316 MB, 32 archivos en git), `build.rs`, `estaticos.rs`,
`rust-embed`, el job `frontend (ng build)` del CI y el paso de npm de
`construir.sh`.

**Lo que compra el cambio es que el build deja de necesitar Node.** El HTML lo
compila `rustc` con los tipos del dominio dentro, así que un error de plantilla
es un error de compilación y no una pantalla en blanco a las tres semanas. El
recuento de dependencias de frontal del producto entero es **una** —htmx, MIT—
más código nuestro.

**35 rutas de panel** en `rutas.rs`, todas apuntando a `vista.rs` (~2.200
líneas), frente a 34 de `/api`. La raíz `/` **redirige a `/entregas`**, y basta
una regla para los dos casos: quien no tiene sesión rebota de ahí a `/entrar`,
porque de eso ya se encarga el extractor `ActorDePanel`. Decidir en `/` entre las
dos habría sido una segunda copia de la regla de sesión.

**Ya no hay `fallback`.** El comodín del SPA devolvía `index.html` con **200** a
cualquier errata de URL; hoy lo que no case es un 404 de verdad. Lo comprueba
`humo.sh` con su pareja: `/no-existe-esta-pagina` da 404 y `/cronograma` sin
sesión da 303.

**Los cinco activos van VENDORIZADOS, versionados y dentro del binario**
(`include_str!`): `htmx.min.js`, `boi.js` —la micro-librería común—, `panel.css`
y **dos comportamientos de boi**, `confirmar-doble.js` (desde el 2026-08-08) y
`calendario-multiple.js` (desde el 2026-08-10), servidos bajo
`/comportamientos/`. Los dos son el archivo de `/mnt/data/boi` **sin tocar** —se
comprueba con `md5sum`—, y por eso importan `from "../boi.js"`: si alguien los
"arreglara" a `./boi.js` para servirlos en la raíz, dejarían de ser el mismo
archivo. El calendario es el que devuelve el gesto que se perdió al retirar
Angular, y nació en boi y no aquí porque un selector de fechas múltiple lo van a
querer los cinco frontales. Jamás por CDN: el producto se instala sin
internet y la CSP es `script-src 'self'`. **No hay framework de CSS**: lo que
Material daba gratis se hace propio, y ese es el coste real del cambio.

**Y una pantalla hay que DIBUJARLA.** Es tentador dar por probada una pantalla
con un `assert` sobre el HTML que devuelve axum —es texto, está a mano y con
`maud` sale limpio—, pero eso mide el códec y no el producto (directiva 28): los
`hx-swap` los ejecuta JavaScript, y un fragmento correcto insertado en el sitio
equivocado devuelve el mismo 200 con el mismo cuerpo. El banco es
`tests/pantalla.rs`, arranca el binario y lo mira con Chromium. **Cazó dos
defectos en su primera corrida.** El límite de `--dump-dom` —no puede llevar
cookie, así que solo alcanzaba lo anterior a la sesión— **se levantó el
2026-08-08** con el cliente CDP, hoy la librería `mirilla`: son **11** pruebas y
cuatro de ellas ENTRAN por el formulario.

**Y desde el 2026-08-11 el banco llega al camino crítico del producto**, que era
el punto 3 de la #443: se sube una entrega DESDE el navegador —`DataTransfer`
para poblar el `input[type=file]`, `submit` normal porque ese formulario no lleva
htmx— y se comprueba en la LISTA que quedó registrada. Entrar lo hace cualquier
aplicación; lo que distingue a SIGER de una hoja de cálculo es esto. La pareja
va dentro: la misma pantalla, la misma sesión, antes y después, y lo único que
cambia es que se subió el archivo.

Dos cosas que la prueba fijó y no estaban escritas en ningún sitio: **el archivo
se guarda como `entrega-<id>-<marca>.<ext>`, nunca con el nombre que manda el
cliente** —dar por bueno ese nombre fue el agujero del portal anterior—, y `.txt`
no está entre las extensiones admitidas. Las dos salieron de una prueba en rojo,
no de leer el código. Ver la sección del banco de navegador,
arriba.

### Validación de CONTENIDO al recibir (`archivo_valido`, #165 + XBRL)

`subir_de` califica el contenido del archivo con `verifica_contenido`, y el
resultado (`archivo_valido`) es un **FACTOR, no una compuerta**: nunca rechaza la
entrega (invariante 2), solo la marca para revisión (evento
`entrega_contenido_no_concuerda`). Tres valores: `Some(true)` concuerda,
`Some(false)` no concuerda / es sospechoso, `None` **sin opinión** —y `NULL` NO
es «inválido», es «no se pudo afirmar ni negar», no se inventa un veredicto
(D20)—.

Para la mayoría de extensiones eso son los **magic bytes** de `infer` (#165). Pero
un **XBRL/XML es texto y no tiene firma binaria**, así que para `xml`/`xbrl` manda
el **evaluador propio** `siger_core::contenido` —NO `infer`— y esto es la lección
del concilio del 2026-11-02: `infer::is_xml` clasifica como «xml» cualquier cosa
que empiece con `<?xml`, lo que (a) no valida nada de la estructura y (b) marcaba
como desajuste TODO XBRL legítimo, además de dejar pasar un `.xml` con XXE. El
evaluador es un **lector XML endurecido anti-XXE** (rechaza el **subconjunto
interno** del DOCTYPE —donde viven las declaraciones de entidad— y toda PI; tolera
el `<!DOCTYPE html>` desnudo y el `PUBLIC` cuyo system-id esté en la **allowlist de
DTD XHTML del W3C**, marcando `SYSTEM`/URL desconocida; admite entidades
predefinidas, numéricas y del conjunto **XHTML**; topes de profundidad/tamaño/nodos) + un
**validador de instancia XBRL Y de iXBRL** (hechos/contextos/unidades + integridad
referencial), cero dependencias nuevas (técnica reimplementada de crabrl, AGPL,
D9/D38). **iXBRL (XBRL embebido en XHTML, lo de EDGAR/ESEF) se RECONOCE desde
2026-11-10**: una raíz `html` que declara el NS inline (1.1 2013 o 1.0 2008) se mide
con el MISMO evaluador —el lector ya recoge `ix:nonFraction`/`nonNumeric` y
`xbrli:context`/`unit` por su nombre local (D1/D9)—. **El enrutamiento del
`.xhtml`/`.htm`/`.html` al evaluador se HIZO el 2026-11-10** (rama
`feat/enruta-xhtml-ixbrl`, cierre por concilio): el lector ahora **tolera el
`<!DOCTYPE html>` desnudo y el `<!DOCTYPE html PUBLIC "…" "<dtd-xhtml-del-w3c>">`**
(solo los system-id (http y https) de la allowlist `DTDS_XHTML_W3C`; `SYSTEM` y cualquier otra
URL se **marcan** —diferencial de parser: un consumidor aguas abajo que trajera el
id externo haría XXE/SSRF, hallazgo del review 2026-11-10—) y **admite las entidades
con nombre del conjunto XHTML** (`&nbsp;`, `&copy;`) **SOLO si el documento declaró el
DTD XHTML de la allowlist que las define** (el `PUBLIC` hacia el W3C): un `<!DOCTYPE
html>` desnudo o ausente NO las habilita —un parser XML estricto rechaza ahí un
`&nbsp;` como entidad no declarada, así que darle veredicto limpio sería el mismo
diferencial de parser; se **marca** (acople conf-7 del concilio, cerrado 2026-11-12,
`Lector::dtd_define_entidades`)—. **Excepción `standalone="yes"`**: aun con el DTD
PUBLIC declarado, un `&nbsp;` bajo `<?xml … standalone="yes"?>` se **marca**, porque
XML 1.0 §4.1 obliga a TODO procesador conforme a rechazar una entidad que solo declara
el subconjunto externo (verificado contra expat; hallazgo del concilio Fable 2026-11-12,
`Lector::standalone_yes`). El modelo de consumidor que cubre el acople es el que RESUELVE
el DTD declarado; contra uno que rehúsa todo DTD queda un diferencial residual que NO se
cierra, para no marcar los filings XHTML-1.0 legítimos de EDGAR/ESEF (D18). Un DTD externo
permitido tampoco habilita las SUYAS (solo el conjunto XHTML fijo). Se mantiene CERRADO el
**subconjunto interno** `[ … ]` (XXE,
*billion laughs*) y toda entidad fuera de la lista. El DOCTYPE vale UNA vez y antes
de la raíz. Un `.xhtml`/`.htm` que **NO** es
iXBRL se **marca** (decisión de producto de Juan 2026-11-10: la extensión declara un
filing, como un `.xbrl` que no es XBRL; un `.xml` que no es XBRL **no** se marca).
**`.xhtml`/`.htm` NO está en `SIGER_EXTENSIONES` por defecto**: enrutar solo surte
efecto si el despliegue lo habilita. *(Condición 1 del concilio —contrastar contra un
filing ESEF/EDGAR REAL— parcial: no se vendorizó uno genuino porque SEC bloquea el UA
genérico en `/Archives` y usar el contacto de Juan viola la regla de privacidad; su
página de bloqueo, XHTML 1.0 real, sí reveló el `<!DOCTYPE … PUBLIC …>` que motivó
tolerar el id externo. Residuo conocido: las PI `<?xml-stylesheet?>` que llevan
algunos filings aún se marcan.)* Un contenido **no-UTF-8** legítimo (ISO-8859-1/UTF-16) da `None` —no se
marca a un cliente que cumplió—, salvo que `infer` lo delate como binario
conocido disfrazado, que sí se marca. El archivo se evalúa en claro, antes de
cifrarlo en reposo; el veredicto no toca el estado de cumplimiento, que lo decide
la fecha.

**Diferido con nota** (medido, aún sin construir): el parser corre inline en el
hilo tokio (hasta ~2,5 s por 50 MB — candidato a `spawn_blocking` como
`leer_en_claro`); el factor colapsa seis veredictos en un bool con un solo texto
(una razón más rica en la bitácora ayudaría a distinguir XXE de codificación
rara); y `Limites::max_bytes` (64 MiB) no está atado a `SIGER_MAX_MB`. Alcance del
validador: instancia XBRL **e iXBRL**, NO taxonomía/linkbases.

## Pendiente conocido

**HECHO el 2026-08-22: el espejo quedó con SOLO `origin/main`.** Se midieron las
**21** ramas que había fuera de `main` (no cuatro: la tabla de abajo se había
quedado corta, justo lo que ella misma advertía) y **las 21 eran borrables** —17
ancestros literales de `main` (historia entera dentro, `--is-ancestor` = true) y
4 con commits propios pero absorbidos/caducos, verificados archivo por archivo
por el método de abajo—. Ninguna viva. Borradas del espejo (`git push origin
--delete`), `HEAD` del bare repuntado a `main`, `fsck` limpio. La tabla de cuatro
filas que seguía aquí queda como **registro del método**, no como pendiente.

**Medido el 2026-08-12 contra el ESPEJO, que es el `origin` de verdad desde el
protocolo solo-local** (`git branch -r --no-merged origin/main`): **cuatro ramas
fuera de `main`, y las cuatro son sedimento**. `gh pr list` ya no significa nada
aquí — el repositorio de GitHub está vaciado.

| Rama | Veredicto |
|---|---|
| `fix/despliegue-mide-si-esta-publicado` | **absorbida**: lo único que tiene y `main` no son las líneas de la era Angular (`<!doctype html`, `rust-embed`, `build.rs`) que `main` reemplazó por las de maud |
| `fix/un-200-no-prueba-que-sea-siger` | **absorbida**: el saneado del `<title>` y el «un 200 no prueba que sea SIGER» están en `main`. **Y esta fila estaba escrita al revés hasta el 2026-08-12**: decía que a `main` «le falta el ancla `^SIGER$`», y era `main` quien LA TENÍA — un resto de la era Angular, cuando la pantalla se titulaba exactamente «SIGER». Con `maud` el título es «Entrar · SIGER», así que ese ancla habría rechazado una publicación correcta. Hoy los dos guiones usan `SIGER$`, que acepta lo real y sigue rechazando «Welcome to nginx!» |
| `fix/politica-hash-mira-el-digest` | **superada**: la política del hash está en `main` y MEJOR — la rama llama a `Argon2::default()` a pelo y trae el `mod estaticos` de Angular, y su comentario repite el porqué que este archivo corrigió el 2026-08-04 («deja a esa persona fuera del sistema», que es falso) |
| `pruebas/banco-de-pantalla` | **caduca**: es Playwright sobre `angular/e2e/`, y `angular/` no existe. Su heredero en `main` es `tests/pantalla.rs` con `mirilla` |

**Lo que cambió respecto de la medición del 2026-08-10, y es el argumento de
volver a medir**: `deps/boi-confirmar-doble-compartido` **está en `main`** (se
mergeó el 2026-08-11, commit `ac9c582`) y `fix/deps-fast-uri-y-hono` ya no
existe; en cambio aparecieron dos ramas que aquella tabla no nombraba. Una tabla
de cuatro filas acertó dos y falló dos en 48 horas.

**Borrarlas es decisión de Juan** —`git push origin --delete <rama>`, una por
una—; aquí solo se mide.

`git cherry` NO sirve para decidir esto, y **medido el 2026-08-12 da 3 `+` y
1 `-`**: las tres primeras salen con `+` —«no aplicada»— porque `main` reescribió
los mismos archivos en la migración a maud y el *patch-id* ya no coincide aunque
la sustancia esté dentro; solo `fix/politica-hash-mira-el-digest` sale con `-`.
O sea que acierta en una de cuatro, y no se puede saber en cuál sin comprobarlo
por otra vía. Lo que discrimina es
`diff <(git show origin/<rama>:<archivo>) <(git show origin/main:<archivo>)` y
mirar **solo las líneas `<`**: si todas son de la era Angular, `main` es
superconjunto.

**Esta sección se mide, no se recuerda: corre esos dos comandos antes de dar
nada por pendiente.** Se lleva ya dos mentiras por no hacerlo: el volcado (#231)
figuró aquí como «escrito y sin mergear en PR #16» **hasta el 2026-08-04, un día
entero después de entrar en `main`** —el PR se mergeó el 2026-08-03 a las 13:06,
y tres párrafos más abajo este mismo archivo ya lo daba por dentro—, y antes le
había pasado igual a la administración de cuentas, el segundo factor y la
licencia. Una lista de pendientes escrita a mano envejece en silencio y hace
trabajar sobre algo que ya no existe.

Si encuentras otro documento que dé por pendiente la inferencia, la firma, el
planificador, la subida, la importación, el volcado (#231), el segundo factor
(#212), la administración de cuentas o la licencia offline, está desactualizado:
las diez están hechas y en `main`.

### Residuo de la revisión de seguridad del 2026-08-16 (bajo el listón)

Salió de auditar aquí las dos clases de fallo que ese día se encontraron en
`medico` —un id del cliente usado sin acotar, y un filtro que degrada a «todos»—.
**Ninguna de las dos vive en informes**, y eso quedó medido: los 29 manejadores
que reciben un id lo acotan por `Alcance` o exigen un rol que ya lo ve todo,
`Alcance` no es un `Option` y sus siete consumos son `match` exhaustivos sin brazo
`_`, y **no hay un solo extractor `Query` en el crate**.

Lo que sí salió, y no se arregló:

| Dónde | Qué |
|---|---|
| ~~`vista.rs` `pantalla_partes`~~ | **RESUELTO el 2026-08-22.** Los dos `unwrap_or_default()` pasaron a `match … Err ⇒ error_en_pantalla`, como `partes_de` arriba y la gemela `partes_fila`. El comportamiento no cambia (el error no puede ocurrir), pero ya no queda el par asimétrico |
| ~~`POST /api/periodicidad/previsualizar`~~ | **RESUELTO el 2026-08-22.** Las dos previsualizaciones (API y panel) y la **pantalla GET `/cronograma`** piden ahora `puede_escribir` (=Admin), como su gemela `lecturas_de`. No filtraba —opera sobre lo que el cliente manda— pero dos guardias distintas sobre la misma función se desincronizan. Se gateó también la pantalla GET para que no quede accesible con el botón en 403 (patrón del panel: nav-muestra / manejador-gatea, como `cuentas_pagina`); el nav sigue listando Cronograma para todos, igual que las demás secciones de Admin. Pareja: `previsualizar_el_cronograma_pide_administracion` (Operador→403, Admin→ok) |
| `auditar_entrega_ajena` | **REVISADO el 2026-08-22 — correcto por diseño, sin cambio.** La consulta de existencia va **sin alcance a propósito**, y su docstring ya lo justifica: acotarla haría que un id ajeno-que-existe devolviera `None` y **dejaría de anotar el sondeo**, que es justo la discriminación (ajena-que-existe vs inexistente) que la pieza existe para dar. No devuelve información —nadie lee la bitácora por HTTP—, y quien la lee por CLI es el dueño del servidor. El «cualquier actor puede provocar entradas» ES la detección de sondeo funcionando: quien pide una ajena que existe queda anotado, que es el objetivo. Estrecharlo sería la directiva 23 al revés. Confirmado por la revisión de seguridad de este lote |
| ~~`revisar_de`~~ | **RESUELTO el 2026-08-22.** El retorno temprano por `filas == 0` (la carrera en que la entrega se borró entre `entrega_contexto` y el `UPDATE`) hace ahora `tx.rollback().await?` EXPLÍCITO en vez de fiarlo al `Drop` de `sqlx`; un `commit` movido de sitio en una edición futura ya no lo rompe en silencio |
| ~~`tests/pantalla.rs`~~ | **RESUELTO el 2026-08-22.** El quinto caso está en `subir_a_una_entrega_ajena_tambien_se_anota` (carril unitario): el dueño (usuario 1) subiendo a SU entrega (9) no genera `entrega_ajena_solicitada`. La mitad positiva ya no vive solo en el carril de navegador `#[ignore]` |
| ~~Las ramas fuera de `main`~~ | **BORRADAS el 2026-08-22.** Eran 21 (no cuatro), las 21 absorbidas o caducas, medidas una a una (17 ancestros literales + 4 verificadas por archivo). El espejo quedó con solo `origin/main`, `fsck` limpio |

## Documentos de negocio (no se editan sin pedirlo)

`EULA_BORRADOR.md`, `LICENCIAMIENTO.md`, `MODELO_LICENCIA_INFORMES.md`,
`CONTRATO_SERVICIOS_DESARROLLO.md`, `PRECIOS_PORTAFOLIO.md`, `ANCLAJE_PRECIOS.md`,
los análisis de minutas: trabajo jurídico de Juan. `verificar_clausulado.py`
comprueba que EULA y modelo de licencia no se contradigan. Despliegue en
`DESPLIEGUE.md`.

**La vuelta atrás de la demo funciona HOY porque no hay migraciones nuevas, y esa
condición hay que comprobarla cada vez** (escrito el 2026-08-16, tras verlo
morder en medico). `sqlx::migrate!` valida al arrancar que el binario conozca
**toda** migración ya aplicada en la base —`ignore_missing` va en `false` y aquí
tampoco se cambia—, así que el día que entre una `000N` nueva, restaurar el
binario anterior y reiniciar deja el servicio en `failed` con `VersionMissing`,
no degradado: caído. Y da igual que la migración sea aditiva; la validación corre
antes de que el esquema importe.

Comprobación antes de dar por buena la vuelta atrás:

    git -C /mnt/data/informes log --since='<fecha del binario desplegado>' \
        --diff-filter=A --name-only --format='' main | grep '\.sql$'

Si imprime algo, hay que restaurar TAMBIÉN la base —con `siger-api restaurar`,
que borra `-wal`/`-shm`; un `cp` dejando el WAL al lado devuelve el estado que se
quería abandonar, con `integrity_check` en `ok`—. Si no imprime nada, basta el
binario.

**Y la demo NO se respalda a propósito**: `siger-demo-sembrar.timer` la regenera
cada noche (comprobado el 2026-08-16: enabled, corrió hace 19 h), así que
respaldarla sería guardar copias de datos ficticios. Eso vale para la DEMO y no
para una instalación de cliente, que sí lleva `siger-respaldo@.timer` — hoy
`not-found` en el VPS, y es correcto: ahí no hay instalación de cliente.
