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

El freno **no bloquea**: tras cinco fallos seguidos el siguiente intento espera,
la espera se dobla y se detiene en cinco minutos. Nadie tiene que desbloquear a
nadie. Un acierto lo borra todo.

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
./despliegue/desplegar-demo.sh usuario@servidor
```

Tres pasos quedan fuera a propósito, porque no son reversibles desde el guion: el
registro DNS de `informes.xiliux.com`, enlazar el sitio en `sites-enabled/` y
pedir el certificado con `certbot --nginx -d informes.xiliux.com` (que exige el
DNS ya resolviendo). El guion los recuerda al terminar.

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
