# Despliegue — un binario, SQLite, y el panel dentro

Tras el cutover a Rust, `informes` (SIGER) es **un solo binario** (`siger-api`,
axum) con **el panel compilado dentro** —`maud` + htmx, que `rustc` compila junto
al resto del programa— y **SQLite** como base. No hay contenedor obligatorio, ni PostgreSQL, ni dependencias del
sistema salvo SQLite. Se copia el binario, se le da su configuración y corre.

La configuración vive en variables de entorno; el binario **no arranca si falta
un secreto** (a propósito). La plantilla está en `.env.example`.

---

## Construir

```bash
./rust/construir.sh              # produce rust/target/release/siger-api
```

**`cargo build --release` a secas YA BASTA para el binario** (2026-08-06). El
panel es código: el HTML lo compilan las macros de `maud` y los cuatro activos
que se sirven tal cual —htmx, `boi.js`, `comportamientos.js` y `panel.css`—
entran por `include_str!` desde `rust/siger-api/estaticos/`, que está
versionado. Si un activo cambia, `rustc` recompila; si falta, no compila. El
binario **no puede** salir desincronizado de su interfaz.

Desapareció por tanto todo el aparato que existía para impedirlo: el paso de
`npm ci && ng build`, la copia a `estaticos/`, y `build.rs`, que rompía el
release cuando esa carpeta estaba vacía. Con él se fue `SIGER_SIN_INTERFAZ`, que
ya no significa nada.

Lo que `construir.sh` sigue añadiendo, y por lo que se usa igual, es el paso 2:
**dibujar la pantalla en un navegador**. Que el HTML compile no dice si se abre
—los `hx-swap` los ejecuta JavaScript, y un fragmento correcto insertado en el
sitio equivocado devuelve el mismo 200 con el mismo cuerpo—.

## Configurar y arrancar

```bash
export SIGER_SECRETO_SESION=$(target/release/siger-api generar-secreto)
export DATABASE_URL=sqlite://siger.db     # por defecto ya es este
./target/release/siger-api                # sirve en http://localhost:8003
```

Subcomandos del binario:

| Subcomando | Para qué |
|---|---|
| `generar-secreto` | Genera el secreto de sesión (>= 32 bytes). |
| `generar-claves-firma` | Genera el par de firma post-cuántica (opcional). |
| `hashear` | Hash Argon2 de una contraseña, con los MISMOS parámetros que verifica el acceso. |
| `importar <origen> <destino>` | Vuelca una base del sistema Python anterior al esquema nuevo. |
| `alertas` | Corre una pasada de alertas por correo (para un cron/timer). |
| `respaldar <carpeta>` | Copia la base **en caliente** y los archivos de las entregas, y **verifica la copia** antes de darla por buena. |
| `restaurar <carpeta>` | Devuelve un respaldo a su sitio. Pare el servicio antes; `--forzar` para pisar una base con datos. |

### Si va detrás de un proxy inverso: `SIGER_TRAS_PROXY=1`

El acceso lleva un **freno al ensayo de contraseñas** que cuenta por pareja
(correo, origen). El origen es, por defecto, la dirección desde la que llegó la
conexión — que el cliente no elige.

Detrás de un proxy, esa dirección es siempre la del proxy y todos los clientes se
confundirían en uno. Con `SIGER_TRAS_PROXY=1` se toma la cabecera `X-Real-IP`.

**Solo se enciende si el proxy la REESCRIBE.** El sitio de nginx de este
repositorio (`despliegue/informes.xiliux.com.nginx`) pone
`proxy_set_header X-Real-IP $remote_addr`, que pisa lo que traiga el cliente. Con
un proxy que se limite a pasarla, encenderlo permite escribir un origen distinto
en cada petición, no llegar nunca al umbral y dejar el freno en adorno.

Si se enciende y la cabecera no llega, **el acceso falla con un 500 y lo dice en
el registro**: no se rellena con la dirección del proxy, porque eso degradaría el
freno a contar solo por cuenta y dejaría que un desconocido frenase al dueño de
la cuenta.

#### Y arrastra dónde escucha: `SIGER_ESCUCHA`

| `SIGER_ESCUCHA` | `SIGER_TRAS_PROXY` | dirección |
|---|---|---|
| puesta | cualquiera | lo que diga usted |
| ausente | `1` | `127.0.0.1` |
| ausente | sin poner | `0.0.0.0` |

Declarar el proxy y seguir escuchando en todas las interfaces deja una puerta
detrás del proxy: quien alcance el puerto directamente **elige su propio
`X-Real-IP`**, y entonces el freno del login no engancha nunca porque cada
intento cae en un cubo distinto. Por eso la declaración arrastra el defecto.

**Si el proxy vive en otra máquina** —o en otra red que la de bucle—, póngala a
mano: `SIGER_ESCUCHA=0.0.0.0`, o la dirección de la interfaz que corresponda. Se
cierra el descuido, no el caso. Una dirección ilegible **no arranca**; no se
sustituye por el defecto.

El freno **no bloquea**: tras cinco fallos seguidos el siguiente intento espera,
la espera se dobla y se detiene en cinco minutos. Nadie tiene que desbloquear a
nadie. Un acierto lo borra todo.

### Cuánta memoria puede comerse el Argon2: `SIGER_CRIPTO_PARALELA`

Hay dos sitios donde el servidor hace trabajo criptográfico caro, y comparten un
solo cupo porque comparten la memoria de la máquina:

| operación | coste | quién la dispara |
|---|---|---|
| descifrar el archivo de una entrega | 162 ms · 64 MiB | usuario con sesión |
| comprobar la contraseña en el login | 45 ms · 19 MiB | **cualquiera, sin sesión** |

El del login lo paga **todo** intento, también el de un correo que no existe: es a
propósito, porque si el inexistente saliera gratis el cronómetro diría qué cuentas
son de verdad.

El del descifrado sorprende: **da igual el tamaño del archivo**: 8 KiB y 4 MiB
cuestan lo mismo, porque el gasto es derivar la clave, no cifrar. Cada archivo
lleva su propia sal, así que la clave no se puede reutilizar entre archivos. Solo
aplica si `SIGER_CLAVE_REPOSO` está puesta.

Con 64 MiB por descifrado, «cuántos a la vez» **es** la memoria del servidor:

| descifrados a la vez | memoria |
|---|---|
| 1 | 66 MiB |
| 4 | 260 MiB |
| 16 | 1 031 MiB |
| 32 | 1 931 MiB |

| `SIGER_CRIPTO_PARALELA` | resultado |
|---|---|
| puesta | lo que diga usted (mínimo 1) |
| ausente | tantos como núcleos |

**El defecto sirve para casi todos.** Súbalo si le sobra memoria y quiere más
descargas simultáneas; **bájelo a 1 o 2 en un VPS de 1 GB**, donde el defecto de
un servidor de cuatro núcleos pediría 260 MiB solo para esto. Un valor que no sea
un entero de 1 en adelante **no arranca**: caer a un defecto le dejaría creyendo
que acotó la memoria mientras el servidor hace otra cosa.

Cuando todos los turnos están ocupados se espera hasta 30 s y luego se responde
**503** («reinténtelo»), nunca algo que culpe al usuario:

- una **descarga** da 503 y no 500: el archivo está perfectamente;
- **`/verificar`** dice *no se puede saber*, nunca «la entrega fue alterada» — un
  servidor con tráfico no es una entrega sospechosa;
- el **login** da 503 y no «credenciales incorrectas»: decirle a alguien que su
  contraseña está mal porque el servidor va lleno es mentirle, y además dejaría al
  freno anotando fallos que nadie cometió. Que el código sea distinto no delata
  ninguna cuenta: habla del servidor, no del correo.

### Cerrar todas las sesiones de un usuario

Cada usuario lleva un `token_version` que viaja firmado dentro de la cookie y se
comprueba en cada petición. Subirlo invalida de golpe todas sus sesiones abiertas
(cambio de contraseña, teléfono perdido, «cerrar sesión en todos los
dispositivos»):

```sql
UPDATE usuarios SET token_version = token_version + 1 WHERE email = 'quien@sea';
```

Quien vuelva a autenticarse entra con normalidad: revoca lo emitido, no la cuenta.

### Respaldos

```bash
# El servicio puede seguir corriendo: la copia se toma bajo una transacción.
siger-api respaldar /var/respaldos/siger-$(date +%F)
```

Tres cosas que este comando hace y que un `cp` no:

1. **No copia el archivo vivo.** La base va en modo WAL: las escrituras
   confirmadas viven en `siger.db-wal` hasta que alguien las integra, así que una
   copia del `.db` se abre, pasa por buena y le faltan las últimas transacciones.
   Se usa `VACUUM INTO`, que toma la instantánea bajo una transacción de lectura.
2. **Se lleva los archivos de las entregas**, que viven fuera de la base. Un
   respaldo con solo la base restaura un sistema donde cada entrega cumplida dice
   tener un archivo que ya no existe.
3. **Verifica la copia y falla si no cuadra**: la abre, le pasa
   `PRAGMA integrity_check`, compara los recuentos de las seis tablas con el
   origen y comprueba que cada archivo que la base nombra llegó. «Hay un cron» no
   es tener respaldos; lo que no se restauró no está probado.

Para automatizarlo, `despliegue/siger-respaldo.service` y su `.timer` diario.

## Servicio (systemd)

Modelo del OPRF: un `EnvironmentFile` con los secretos (permisos `600`), el
binario bajo `systemd`, y un `nginx` delante con TLS.

```ini
# /etc/systemd/system/siger.service
[Unit]
Description=SIGER (informes)
After=network.target

[Service]
EnvironmentFile=/etc/siger/siger.env
ExecStart=/opt/siger/siger-api
Restart=on-failure
WorkingDirectory=/opt/siger

[Install]
WantedBy=multi-user.target
```

`nginx` hace de proxy inverso al puerto `8003` y termina el TLS (certbot). Las
alertas, si se activan, se disparan con un `systemd timer` diario que llama a
`siger-api alertas` (no un hilo dentro del proceso: una pasada que puede fallar
sin tumbar el servidor — es el modo degradado, #71).

## Demostración (datos ficticios)

Para VER el sistema funcionando sin instalar nada de verdad:

```bash
python rust/sembrar_demo.py           # crea rust/demo/siger.db con datos ficticios
cd rust/demo && SIGER_SECRETO_SESION=$(../target/release/siger-api generar-secreto) \
  ../target/release/siger-api
```

**Arráncalo DESDE la carpeta de la demostración.** El servidor abre `siger.db`
relativo a su directorio de trabajo: desde la raíz del repositorio crea una base
vacía sin usuarios, y el acceso responde «credenciales incorrectas» —correcto
para no filtrar si el correo existe, e indistinguible de «esta base no tiene a
nadie»—.

La semilla siembra **un informe por cada modo de recurrencia** (específico
mensual y trimestral, cíclico, semanal, horario, fechas y a solicitud) y deja que
el **planificador del binario** genere las entregas con su calendario de
festivos. Si algún modo de calendario no produjera ninguna, el guion falla en vez
de dejar una demostración vacía: una demostración que no depende del motor no
demuestra el motor.

Dos cosas que el guion **no** hace, por si alguien apunta el `--ruta` a otro
sitio: no siembra sobre una base cuyos usuarios no sean los de la demostración
(hay que pedirlo con `--forzar`), y `--limpiar` borra la base y el almacén, nunca
la carpeta —en el servidor ahí viven también el binario y el propio guion—.

## La demostración pública (informes.xiliux.com)

Todo lo que hace falta está en `despliegue/`: la unidad de systemd, el timer que
regenera los datos cada noche, el sitio de nginx y `desplegar-demo.sh`, que
construye, comprueba que el binario lleva la interfaz DENTRO, copia y arranca.

```bash
./despliegue/desplegar-demo.sh debian@192.99.42.108
```

El primer argumento es el destino de `ssh` **de verdad**, no un marcador: en el
VPS de OVH con imagen Debian el usuario es `debian`. El guion **comprueba el
acceso antes de construir nada** — el `ssh` es lo primero que puede fallar por
algo ajeno al código (usuario equivocado, clave todavía sin autorizar, servidor
apagado), y descubrirlo tras medio minuto de compilación hace pensar que falló el
despliegue cuando lo que falla es la puerta. Si su clave no está autorizada aún:
`ssh-copy-id -i ~/.ssh/id_ed25519.pub debian@192.99.42.108`, una sola vez.

**La demostración YA ESTÁ PUBLICADA** — `https://informes.xiliux.com/` responde
200 y el registro DNS existe (medido el 2026-08-05). Hasta ese día este documento
decía lo contrario: *«el binario queda escuchando solo en el bucle local: hasta
publicarlo, nada es alcanzable desde fuera. Falta el registro DNS»*. Era la misma
frase falsa que el guion imprimía y que se corrigió ahí; vivía aquí también, y un
documento que dice «no es alcanzable» sobre un sitio que anuncia sus credenciales
en internet es peor que uno que no dice nada.

Consecuencia que hay que tener presente al desplegar: **lo que se sirve ahí es
público, credenciales incluidas**, y eso es a propósito. Se mide como lo mide el
mundo, nunca con un `is-active`:

```bash
curl -s -o /dev/null -w '%{http_code}\n' --max-time 15 https://informes.xiliux.com/
curl -s https://informes.xiliux.com/api/demostracion
dig +short informes.xiliux.com
```

Para una instalación NUEVA que todavía no resuelva, el registro es:

```
Registro DNS   A   <dominio>  ->  192.99.42.108     (panel de OVH)
```

y cuando resuelva, publicar es un comando que además lo comprueba **desde fuera
del servidor**, que es lo único que prueba el DNS, el certificado, el proxy y el
binario a la vez:

```bash
./despliegue/publicar-demo.sh debian@192.99.42.108
```

**Por qué esto es un guion y no tres líneas aquí.** Lo obvio —enlazar el sitio,
`nginx -t`, certbot— **no funciona en ese orden**, y falla con el sitio a medio
publicar:

- `informes.xiliux.com.nginx` trae `listen 443 ssl` con los `ssl_certificate`
  comentados, porque los escribe certbot. Habilitarlo antes de tener certificado
  hace fallar `nginx -t`.
- Pero certbot necesita que alguien conteste en el puerto 80 por ese nombre para
  el reto HTTP-01, y el sitio por defecto de este servidor responde `444` —cierra
  sin responder— a todo host desconocido. Sin sitio habilitado, el reto no llega.

No se puede habilitar sin certificado ni obtener certificado sin habilitar.
`publicar-demo.sh` rompe el círculo con un sitio temporal de solo-HTTP que sirve
únicamente el reto, pide el certificado con `certonly --webroot` —y no con
`--nginx`, que reescribiría el archivo del sitio y se llevaría por delante las
cabeceras, el `limit_req` y el proxy afinados a mano—, y solo entonces instala el
definitivo. Comprueba el DNS **antes** de tocar nada, y contra la IP del servidor:
si apunta a otra máquina, para y no deja nada a medias.

Las contraseñas del seed están escritas en el guion (es para mirar, no para
producción) y las fechas se calculan desde HOY, para que la columna de urgencia
enseñe su trabajo sin envejecer. Las contraseñas las cifra el propio binario
(`siger-api hashear`), con el mismo Argon2 que verifica el acceso.

### Y el visitante tiene que SABER con qué entrar

Quien llega por un enlace cae en la pantalla de acceso sin conocer ninguna
contraseña. Una demostración en la que nadie puede entrar no demuestra nada, así
que esa pantalla las **anuncia**, con un botón «Entrar como…» por cuenta.

Lo que anuncia sale de `SIGER_DEMO_CUENTAS`, y **en una instalación de cliente esa
variable no existe**: `GET /api/demostracion` devuelve una lista vacía y la
pantalla es la de siempre. No hay ningún `SIGER_DEMO=1` que encienda un juego de
cuentas conocidas — dentro del binario no hay ninguna lista que activar por
descuido, y lo que no se escribe en el entorno no se publica.

El valor lo escribe `desplegar-demo.sh` en cada despliegue, a partir de
`sembrar_demo.py --anunciar`: la **misma** fuente que siembra las cuentas. Si
fueran dos listas, el día que cambie una contraseña el cartel diría la vieja — y
un cartel que miente es peor que ninguno, porque el visitante concluye que el
producto está roto y se va sin escribir. `humo.sh` lo comprueba por socket como
lo comprobaría él: lee lo anunciado y **entra con eso**.

---

## Notas

- **Alertas**: dormidas por defecto. Se encienden con `ALERTAS_ACTIVAS=true` y
  exigen `SMTP_HOST`/`SMTP_DESDE` (o el binario no arranca). Sin ellas, las
  notificaciones internas de la aplicación siguen funcionando.
- **Firma de entregas**: opcional. Sin `QUIPU_SIGNING_KEY` la entrega se registra
  igual, sin firma — la ausencia de firma nunca impide cumplir.
- **Backups**: la base es SQLite; respaldar EN CALIENTE con la API de respaldo de
  `sqlite3`, no copiando el archivo vivo (una copia de una base en uso puede
  capturar una transacción a medias y producir un respaldo que parece bueno y no
  restaura).
- **Licencia offline**: portada a Rust (`licencia.rs`, #175). Un archivo firmado
  por el licenciante que la aplicación verifica **en local**, sin red, con la
  misma criptografía post-cuántica que sella las entregas. La licencia **nunca
  bloquea nada** —ni ausente, ni con el mantenimiento vencido, ni alterada, ni
  con exceso de usuarios—: solo da visibilidad (`GET /api/licencia`) y evidencia.
  Ese endpoint reparte en dos: el aviso lo ve cualquiera con sesión; los datos
  del Anexo A y el exceso de usuarios, solo `Admin` y `Supervisor`.
  El remedio ante el exceso es contractual. Si una licencia vencida impidiera
  registrar una entrega, Xiliux pasaría a ser la causa del incumplimiento del
  cliente.

  En el cliente, dos variables, ninguna obligatoria:

  | Variable | Qué es |
  |---|---|
  | `INFORMES_LICENCIA` | ruta del archivo. Por defecto `licencia.quipu`, junto al binario. Sin archivo: «ausente», que es normal en evaluación. |
  | `INFORMES_LICENCIA_PUBKEY` | clave **pública** del licenciante, en hex. Sin ella el veredicto es «sin verificador», nunca «inválida»: no se acusa a una licencia que no se ha podido mirar. |

  En casa del licenciante, y **solo** ahí:

  ```bash
  siger-api generar-claves-licencia     # una vez; la SK no viaja al cliente

  # La clave se TECLEA, no se escribe en la línea de órdenes.
  read -rs -p 'SK del licenciante: ' INFORMES_LICENCIA_SK && export INFORMES_LICENCIA_SK
  siger-api emitir-licencia \
      --licenciatario "ACME S.A.S." --nit 900123456-1 \
      --edicion profesional --perfil onprem --modalidad asistida \
      --version 4.0 --usuarios 25 --sedes 3 \
      --mantenimiento-hasta 2027-07-31 --salida licencia.quipu
  unset INFORMES_LICENCIA_SK
  ```

  **Por qué `read -rs` y no `INFORMES_LICENCIA_SK=<hex> siger-api …`**, que es lo
  que decía aquí hasta el 2026-08-03: el binario se niega a aceptar la clave como
  argumento precisamente para que no acabe en el historial del intérprete ni a la
  vista de `ps`. El prefijo de entorno la saca de `ps`, pero **la deja escrita en
  el historial igual**, así que la mitad del control se perdía en la propia
  documentación que lo explicaba. Con `read -rs` no se teclea en pantalla ni se
  guarda, y el `unset` cierra el rato en que está en el entorno. Lo señaló la
  revisión de seguridad de la rama.

  `--mantenimiento-hasta` es el fin del **Período de Mantenimiento**, no de la
  licencia: el derecho de uso es perpetuo (Cláusulas 3.1, 3.2 y 7.6 del EULA).
  Pasada esa fecha la interfaz dice «mantenimiento vencido» y **nunca** «licencia
  vencida», que sería falso y empujaría al cliente a pagar por lo que ya tiene.
