# Despliegue — un binario, SQLite, la interfaz embebida

Tras el cutover a Rust, `informes` (SIGER) es **un solo binario** (`siger-api`,
axum) con la interfaz **Angular embebida** dentro del ejecutable y **SQLite**
como base. No hay contenedor obligatorio, ni PostgreSQL, ni dependencias del
sistema salvo SQLite. Se copia el binario, se le da su configuración y corre.

La configuración vive en variables de entorno; el binario **no arranca si falta
un secreto** (a propósito). La plantilla está en `.env.example`.

---

## Construir

```bash
./rust/construir.sh              # produce rust/target/release/siger-api
```

**Un solo comando, y es el único correcto.** `cargo build --release` a secas NO
construye la interfaz: `rust-embed` empotra lo que haya en
`rust/siger-api/estaticos/`, cuyo contenido está en `.gitignore`. En un clon
limpio esa carpeta está vacía, y el binario saldría sirviendo la API con un 404
en la raíz. `construir.sh` compila Angular primero y lo copia; el orden importa,
porque al revés el binario viaja con la interfaz de la vez anterior.

Ya no se puede equivocar uno en silencio: `rust/siger-api/build.rs` **rompe la
compilación de release** si falta `estaticos/index.html`. Si de verdad se quiere
un binario sin interfaz, hay que pedirlo:
`SIGER_SIN_INTERFAZ=1 cargo build --release`.

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

El binario queda **escuchando solo en el bucle local**: hasta publicarlo, nada es
alcanzable desde fuera. Falta una sola cosa que no depende del guion:

```
Registro DNS   A   informes.xiliux.com  ->  192.99.42.108     (panel de OVH)
```

Cuando resuelva, publicar es un comando que además lo comprueba desde fuera:

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
- **Licencia offline — PENDIENTE de portar a Rust.** En el sistema Python existía
  una licencia firmada por el licenciante que la app verificaba en local
  (`INFORMES_LICENCIA`). El port a Rust todavía NO la trae; está registrada como
  tarea. Cuando exista, mantendrá el principio del modo degradado: la licencia
  **nunca bloquea nada** —ni ausente, ni vencida, ni alterada—, solo da
  visibilidad; el remedio ante el exceso es contractual. Si una licencia vencida
  impidiera registrar una entrega, Xiliux pasaría a ser la causa del
  incumplimiento del cliente.
