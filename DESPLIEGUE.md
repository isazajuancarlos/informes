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
cd rust
cargo build --release            # produce target/release/siger-api
```

La interfaz Angular se construye e incrusta como parte del build (rust-embed).
Para reconstruir la interfaz de cero:

```bash
cd angular && npm ci && npx ng build     # deja el bundle que el binario embebe
```

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
DATABASE_URL=sqlite://rust/demo/siger.db ./target/release/siger-api
```

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
