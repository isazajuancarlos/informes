# Despliegue — un código, tres perfiles

El mismo código y la misma imagen Docker sirven a tres topologías. El
comportamiento lo decide una variable, `DEPLOYMENT_PROFILE`.

| Perfil | Para quién | Base de datos | Internet |
|---|---|---|---|
| `cloud` | SaaS que hospedamos nosotros | PostgreSQL | sí |
| `onprem` | Entidad/empresa con servidor propio | su PostgreSQL | no (LAN) |
| `standalone` | Empresa que no quiere administrar BD | SQLite | no |

`DATABASE_URL` explícito siempre gana. Si no se define y el perfil es
`standalone`, la app usa SQLite en `storage/informes.db` sola.

---

## `standalone` — una sola máquina, sin BD que administrar

Lo más simple. SQLite dentro del contenedor, offline.

```bash
export STORAGE_SECRET=$(openssl rand -hex 32)   # opcional pero recomendado
docker compose -f docker-compose.standalone.yml up -d
# abre http://localhost:8003
```

El volumen `data` persiste **todo** el estado (la base `informes.db` y los
archivos subidos). Respaldar = copiar ese volumen.

Sin Docker, directamente:

```bash
pip install -r requirements.txt
DEPLOYMENT_PROFILE=standalone python main.py
```

---

## `onprem` — el cliente lo corre en su servidor

App + PostgreSQL juntos, en su red, sin salir a internet.

```bash
cp .env.example .env
# edita .env: DB_PASSWORD y STORAGE_SECRET (usa valores propios)
docker compose up -d
# abre http://<servidor>:8003
```

Los datos viven en el volumen `db_data`, en la máquina del cliente. Nadie fuera
de su red los ve — un argumento de venta, no una limitación.

---

## `onprem` air-gapped — sin internet ni para bajar la imagen

Entidades que no permiten salida a la red. Se construye la imagen donde sí hay
internet, se exporta a un archivo, se traslada (USB) y se carga en destino.

**Donde hay internet:**

```bash
docker compose build
docker save informes-app postgres:16-alpine -o informes-offline.tar
# lleva informes-offline.tar + docker-compose.yml + .env al servidor destino
```

**En el servidor air-gapped:**

```bash
docker load -i informes-offline.tar
docker compose up -d
```

---

## Primer arranque

En cualquier perfil, al arrancar la app:

1. Crea el esquema si falta (`init_db`, seguro: solo tablas que no existan).
2. Inicia el scheduler en segundo plano.

Para datos iniciales (usuarios de prueba, variables), dentro del contenedor:

```bash
docker compose exec app python seed_data.py
```

En el log verás el perfil detectado:

```
🚀 Perfil de despliegue: onprem (PostgreSQL, offline)
✅ Background Scheduler iniciado
```

---

## Notas y pendientes

- **Email**: en `onprem`/`standalone` (offline) no se espera SMTP; las
  notificaciones in-app siguen funcionando. Si el cliente tiene un servidor de
  correo en su red, configura `SMTP_HOST`/`SENDER_EMAIL`/`SENDER_PASSWORD` y el
  email se reactiva solo.
- **Backups**: en PostgreSQL usa `pg_dump`; en SQLite (standalone) usa la API de
  respaldo EN CALIENTE de `sqlite3`, no una copia del archivo. La diferencia
  importa: copiar una base viva puede capturar una transacción a medias y
  producir un respaldo que parece bueno y no restaura. Al restaurar se guarda
  antes una copia de la base anterior (`informes.db.previa-*`), porque restaurar
  es justo cuando alguien está nervioso y puede equivocarse de archivo.
- **Licencia offline**: un archivo `licencia.quipu` firmado por EL LICENCIANTE,
  que la aplicación verifica en local — sin red, apto para air-gapped. Se emite
  con `python emitir_licencia.py` y sus campos son los del **Anexo A** del EULA.
  La ruta se configura con `INFORMES_LICENCIA` y la clave pública con
  `INFORMES_LICENCIA_PUBKEY`.

  **La licencia NUNCA bloquea nada**: ni ausente, ni vencida, ni manipulada. Da
  visibilidad —un aviso permanente en la cabecera— y evidencia; el remedio ante
  el exceso es contractual. Es la aplicación del principio del modo degradado:
  la ausencia de una capa opcional no puede ser la causa de que el cliente
  incumpla ante un tercero. Si una licencia vencida impidiera registrar una
  entrega, Xiliux pasaría a ser la causa del incumplimiento del cliente.
- **Firma de entregas y componente Quipu**: la copia de `quipu-crypto` que se
  entrega **no se instala desde PyPI**. Se empaqueta en `vendor/` con
  `python preparar_distribucion.py` y `requirements-firma.txt` la instala con
  `--no-index`. Obtenerla del índice público la entregaría bajo AGPL-3.0 y
  activaría el artículo 13 sobre el servicio en red del cliente; la Cláusula 9.2
  del EULA la entrega bajo licencia comercial. Si `vendor/` está vacío, la
  instalación FALLA en vez de caer a PyPI, que es lo correcto.
- **Actualizaciones**: on-premise no se hotfixea remotamente. La ruta de upgrade
  es reconstruir/recargar la imagen; las migraciones (`migrate_v4.py`) son
  idempotentes.
