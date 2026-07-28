# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## El nombre del producto

Se llama **SIGER** — *Sistema Integral de Gestión de Informes*. La carpeta se
llama `informes` y así se queda: el repositorio no es el producto.

El nombre vive en **un solo sitio**, `app/core/config.py`
(`NOMBRE_PRODUCTO` / `NOMBRE_LARGO`), y es configurable por entorno para que un
cliente que lo instale en su casa pueda poner el suyo. **No se escribe a mano en
ninguna pantalla**: llegó a estar en cinco sitios diciendo cinco cosas distintas
—«SIGER» al entrar, «SISTEMA INTEGRAL» en la cabecera, «Sistema Gestión» en la
pestaña del navegador, «Sistema de Gestión» en los correos y «Sistema de Gestión
de Informes» en el clausulado— y no lo vio nadie porque nunca estuvieron en la
misma frase. `tests/test_nombre_del_producto.py` impide que vuelvan a separarse.

## Project Overview

**SIGER** — NiceGUI + FastAPI + SQLAlchemy. Gestiona obligaciones de entrega de
informes: define cada obligación (quién la produce, quién la recibe, cada
cuánto), genera los vencimientos, avisa, recibe la entrega, la firma y deja
rastro auditable. Cuatro roles (ADMIN, OPERADOR, SUPERVISOR, TERCERO) con control
de acceso por ruta.

Dos invariantes atraviesan el código y explican casi todas las decisiones:

1. **El flujo es lo único que el producto modela con columnas.** Quién produce,
   quién recibe, quién consume. Todo lo demás —campos de la ficha, taxonomías,
   reglas— es configuración del cliente y vive como datos, no como esquema.
2. **Lo opcional nunca puede impedir cumplir.** La firma sin `quipu-crypto`, la
   licencia vencida, el correo sin SMTP: todo degrada con gracia. Un bloqueo
   convertiría a Xiliux en la causa del incumplimiento del cliente ante su ente
   de control. Ver `app/services/license_service.py`, que lo argumenta entero.

## Development Commands

### Instalación

El camino que recorre el cliente, y el que prueba el CI:

```bash
python instalar.py --perfil standalone --modo nativo --si   # o --modo docker
```

Sin banderas es interactivo. `--kit` prepara la distribución; `--revisar` solo
diagnostica. El CI (`instalacion`) lo ejecuta sobre una copia limpia
(`git archive`) y después comprueba que se puede entrar con las credenciales que
promete y que la aplicación responde: que devuelva 0 no bastaba —hubo una época
en que sembraba datos sin crear el esquema y anunciaba unas credenciales que no
existían—.

A mano:

```bash
pip install -r requirements.txt
cp .env.example .env
python -m app.init_db     # crea solo las tablas que faltan
python migrar_esquema.py  # añade columnas que faltan (idempotente)
python seed_data.py       # admin@co.co/123456, operador@co.co/123456
```

### Ejecución

```bash
python main.py
```

Puerto **8003** por defecto, configurable con `PORT` (las pruebas visuales
levantan una segunda instancia sin pelearse con la de desarrollo). Recarga en
caliente salvo `RELOAD=false`. Al arrancar imprime el perfil de despliegue y
`✅ Background Scheduler iniciado`.

### Pruebas

**629 pruebas.** No es un proyecto sin pruebas — si lees lo contrario en algún
documento, está desactualizado.

```bash
pytest tests/ -q                          # toda la suite
pytest tests/test_periodicidad.py -q      # un archivo
pytest tests/test_flujo.py::test_nombre   # una prueba
pytest -k "firma and not visual"          # por nombre
```

Lo que hay que saber antes de escribir una:

- `conftest.py` fija `DATABASE_URL=sqlite:///:memory:` y
  `DEPLOYMENT_PROFILE=standalone` **antes** de que se importe nada de `app`,
  porque `app.core.database` lee eso al importarse. En memoria a propósito: con
  fichero, la base sobrevive entre corridas y arrastra el esquema de ayer.
- `pytest.ini` convierte `LegacyAPIWarning` de SQLAlchemy en **error**. Un aviso
  mide qué se ejecutó, no cuánta deuda hay (`Query.get()` vivía en 28 sitios y
  la suite mostraba uno). Si alguna vez hace falta una API legada a propósito, se
  marca esa prueba con `@pytest.mark.filterwarnings(...)` y queda dicho por qué.
- Las pantallas se prueban de verdad con el simulador de cliente de NiceGUI
  (fixture `user`, plugin declarado en `conftest.py`): construye los elementos y
  los pulsa. Requiere `asyncio_mode = auto`; en modo estricto salen en ERROR de
  setup, que parece un problema de entorno y se acaba ignorando.
- `nicegui.testing` **no calcula CSS**: para él una columna con `display:none`
  existe igual. Por eso `tests/test_pantalla_visual.py` usa Playwright con
  Chromium de verdad (`pip install playwright && playwright install chromium`).
  Se omite solo si falta el navegador — opcional en desarrollo, obligatorio en
  el CI.
- Las pruebas de firma se **omiten** sin `quipu-crypto`. Con `EXIGIR_CRIPTO=1`
  fallan en vez de omitirse: es lo que usa el CI, porque un verificador que no
  puede ponerse rojo no informa de nada.

Verificadores fuera de pytest, que el CI también corre:

```bash
python herramientas/verificar_patrones.py herramientas/patrones_referencia.json
python verificar_clausulado.py   # EULA_BORRADOR.md vs MODELO_LICENCIA_INFORMES.md
```

### CI (`.github/workflows/ci.yml`)

Seis trabajos, cada uno guardando algo que ya falló una vez:

| Trabajo | Qué protege |
|---|---|
| `pruebas` | la suite, en SQLite/standalone y con la cripto exigida |
| `migracion` | `migrar_esquema.py` sobre una base **antigua**, dos veces (idempotencia), filas viejas **rellenadas** y no en NULL, y que `migrate_firma.py` siga negándose a correr |
| `instalacion` | el asistente instala en copia limpia, se entra con las credenciales prometidas (y solo con ellas) y la app responde |
| `visual` | las pantallas en Chromium: el CSS que `nicegui.testing` no ve |
| `patrones` | los 80 patrones de referencia contra el motor real |
| `clausulado` | que EULA y modelo de licencia digan lo mismo |

### Migraciones

```bash
python migrar_esquema.py        # columnas que faltan; idempotente, ambos dialectos
python migrar_campos_ficha.py   # columnas fijas de Informe -> MetaVariable(ámbito FICHA)
python migrate_v4.py            # tablas/columnas v4 (unidades, delegaciones, notificaciones)
```

En una instalación **nueva** las tablas no existen todavía: primero
`python -m app.init_db`, después migrar. La migración lo dice si te equivocas de
orden.

**No usar `migrate_firma.py`**: usa sintaxis exclusiva de PostgreSQL
(`ADD COLUMN IF NOT EXISTS`, `information_schema`) y en SQLite —el motor de
`standalone`— no hacía nada, en silencio, mientras su verificación informaba «NO
SE PUDO VERIFICAR». Ya se niega a correr, y el CI comprueba que se sigue negando.

### Base de datos y datos de prueba

```bash
python -m app.init_db             # seguro: solo crea lo que falta
python reset_total.py             # CUIDADO: borra TODAS las tablas y las recrea
python importar_datos_demo.py --dry-run   # 397 informes sintéticos
python demo_empresa_privada.py            # demo de empresa privada
python emitir_licencia.py                 # emite una licencia offline firmada
python preparar_distribucion.py           # puebla vendor/ con quipu-crypto
```

### Perfiles de despliegue

`DEPLOYMENT_PROFILE` decide motor de base y modo offline. El mismo código sirve
a los tres:

| Perfil | Base por defecto | Red |
|---|---|---|
| `cloud` (defecto) | PostgreSQL | online |
| `onprem` | PostgreSQL del cliente | offline |
| `standalone` | SQLite en `storage/informes.db` | offline |

`DATABASE_URL` explícito siempre gana. **La suite y el CI corren en SQLite**: es
la edición que se entrega instalada, y probar solo contra PostgreSQL ya dejó
pasar una migración rota.

### Variables de entorno

Todo en `.env` (nunca se commitea). Además de las evidentes (`DATABASE_URL`,
`SMTP_*`, `SENDER_EMAIL`/`SENDER_PASSWORD` → `EMAIL_ENABLED`, `STORAGE_SECRET`,
`SESSION_TIMEOUT_MINUTES`, `APP_BASE_URL`, `MAX_FILE_SIZE_MB`,
`ALLOWED_EXTENSIONS`, `BACKUP_PATH`, `MAX_RETROACTIVE_DAYS`,
`ENABLE_AUTO_SCHEDULER`, `SCHEDULER_HOUR`/`SCHEDULER_MINUTE`):

- `DEPLOYMENT_PROFILE` — `cloud` | `onprem` | `standalone`
- `OFFLINE` — fuerza el modo offline (por defecto lo deducen los perfiles)
- `NOMBRE_PRODUCTO` / `NOMBRE_LARGO` — marca del cliente
- `PORT`, `RELOAD` — puerto y recarga en caliente
- `PAIS_ISO` (`CO`), `TIMEZONE` (`America/Bogota`), `IDIOMA_DEFECTO` (`es`|`en`)
- `QUIPU_CUSTODIA` (`entorno`|`hsm`), `QUIPU_SIGNING_KEY`, `QUIPU_VERIFYING_KEY`
- `INFORMES_LICENCIA`, `INFORMES_LICENCIA_PUBKEY`
- `EXIGIR_CRIPTO=1` — solo pruebas: la ausencia de `quipu-crypto` es fallo

## Architecture

### El flujo: `Participacion` es la única fuente de verdad

Quién participa en un informe y con qué papel vive en **una sola tabla**,
`participacion_informe`. Antes vivía en cuatro sitios que podían contradecirse:
`asignaciones_informe_usuario`, `rel_informe_solicitantes`,
`rel_informe_destinatarios` y una columna `tipo_flujo` almacenada. Si lees
código o documentos que los mencionen, son de antes.

Cinco roles genéricos (`app/models/meta_tables.py`, arriba del todo):
`ORIGEN` (produce y envía), `DESTINO` (recibe), `USUARIO` (consume),
`SUPERVISA` (responde por el cumplimiento), `EJECUTA` (hace el trabajo).

Una parte es interna (`usuario_id`) **o** externa (`stakeholder_id`), nunca las
dos — lo garantiza un `CheckConstraint`, y la unicidad necesita **dos índices
parciales**, porque con una columna NULL un `UniqueConstraint` no restringe nada
(`NULL != NULL`).

`Informe.tipo_flujo`, `.solicitantes`, `.destinatarios`, `.responsables`,
`.supervisores` son **propiedades derivadas** de las participaciones, no
columnas. `tiene_quien_la_reciba` acepta DESTINO **o** USUARIO: un tablero que
un auditor consulta no se le envía a nadie y la obligación existe igual.

### Los campos del informe los define el administrador

`MetaVariable` tiene `ambito`: `FICHA` (metadatos del informe: subsistema, cargo
autorizador, plantilla…) o `CONTENIDO` (datos que se capturan al ejecutar).
`CONTENIDO` es el defecto para que una base anterior siga significando lo mismo
tras migrar. Las nueve columnas fijas que había estaban copiadas del Excel de un
cliente concreto: convertían el ejemplo en la especificación. Ahora son datos
(`migrar_campos_ficha.py` copia los valores viejos).

Tipos: TEXTO, NUMERO, FECHA, MONEDA, HORA_MILITAR, TEXTO_ALFANUM, BOOLEANO,
LISTA (con `opciones`, una por línea; vacío = campo libre).

### Motor de causalidad (`app/services/causality_engine.py`)

Evalúa `MetaRegla` contra lo que el usuario captura y devuelve un plan: alertas,
archivos exigidos, roles a notificar. Operadores: EQUALS, GREATER_THAN,
LESS_THAN, GREATER_THAN_OR_EQUAL, LESS_THAN_OR_EQUAL, CONTAINS, IN_LIST.
Consecuencias: FILE_REQ, DATE_CHANGE, NOTIFY_ROLE, BLOCK.

### Periodicidad: cinco módulos, y el orden importa

El motor (`app/services/scheduler.py`, `GeneradorTareas`) genera
`EntregaEsperada` desde `recurrencia_json`. Modos: `CICLICO` (cada N días),
`ESPECIFICO` (días del mes/meses), `SEMANAL`, `HOURLY` y `FECHAS` (lista
literal — la salida de emergencia: una obligación que no sigue ningún patrón se
enumera, y así nadie se queda fuera). Ajusta a día hábil con
`MotorReglasDinamicas` (`app/services/calendar_engine.py`), que a su vez usa la
librería `holidays` según `PAIS_ISO` y degrada con gracia si el país no está.

Alrededor, la capa que evita obligaciones que *parecen* creadas y no hacen nada:

- `periodicidad.py` — qué preguntar según la periodicidad, en vocabulario de
  usuario («trimestral»), no del motor. `CAMPOS_QUE_USA` está **medido**, no
  deducido leyendo el código.
- `asistente_regla.py` + `app/ui/asistente_cronograma.py` — se marcan fechas en
  un calendario y la lista deja de ser la pregunta para ser la respuesta: las
  lecturas posibles de lo marcado, cada una con las fechas que produciría. La
  lógica está en el servicio y se recorre entera sin navegador.
- `inferir_periodicidad.py` — del calendario a la regla. Cada patrón candidato
  se **ejecuta** y se descarta si no reproduce exactamente las fechas marcadas.
  Un patrón mal inferido es peor que ninguno: el usuario lo da por bueno.
- `validar_regla.py` — valida **ejecutando**: si la regla no produce ningún
  vencimiento, no se guarda, sea cual sea el motivo. Las comprobaciones
  concretas existen para *explicar*, no para detectar.
- `preview_calendario.py` — vista previa con el **motor real**
  (`obtener_deadlines_para_fecha`). Una copia de la lógica mostraría fechas que
  el sistema luego no genera.

### Las dos fechas de un vencimiento

En `EntregaEsperada`, y confundirlas es el defecto que el par existe para
impedir:

- `fecha_vencimiento_nominal` — el día que fija la norma o el contrato. No se
  mueve aunque sea festivo. Es la **identidad** de la entrega: la clave por la
  que el planificador decide si ya existe. NULL = anterior al cambio o creada a
  mano; no se rellena con la otra.
- `fecha_limite_teorica` — el día contra el que se mide el cumplimiento, ya
  resuelto el ajuste a hábil. Es lo que pinta la interfaz en rojo. El nombre es
  histórico: contiene el límite **efectivo**.
- `tipo_ajuste` — `NINGUNO` | `ANTICIPADO` | `GRACIA`.

### Relojes: UTC vs local (`app/core/tiempo.py`)

El modelo mezcla dos relojes y la mezcla es histórica. Seis columnas guardan
**UTC naive** (`ahora_utc()`, que sustituye a `datetime.utcnow()`); 45 sitios
usan `datetime.now()` local. La excepción es
`EntregaEsperada.fecha_programada`, que guarda **hora local** (`ahora_local()`):
los cinco sitios que crean entregas la fijan a mano en local.

Lo que rompía era **mostrar** un valor UTC como si fuera local: el registro de
auditoría —el que el modelo cita para la Ley 1581/2012— aparecía cinco horas en
el futuro. Se corrige **al presentar**, no al guardar, porque así quedan
arregladas también las filas antiguas y no hay discontinuidad. Si algún día se
migra a datetimes con zona, el sitio es `app/core/tiempo.py`.

### Firma y licencia (Quipu, opcionales)

- `signing_service.py` — firma post-cuántica (Ed25519 + ML-DSA-87) de los campos
  auditables de una entrega, **dentro de la misma transacción** que fija estado,
  fecha y archivo. Se verifica desde `/auditoria`. Sin `quipu-crypto` o sin
  claves, `firmar_entrega` devuelve None y la entrega se registra igual.
- `license_service.py` — licencia offline: archivo firmado, verificado en local,
  sin red. **No bloquea nada, nunca.** Da visibilidad (aviso en la interfaz) y
  evidencia (registro auditable); el remedio ante el exceso de uso es
  contractual, no técnico.
- `requirements-firma.txt` instala con `--no-index --find-links vendor/`. **No es
  un detalle de empaquetado**: desde PyPI, `quipu-crypto` viene bajo AGPL-3.0 y
  su uso en red activaría el artículo 13 contra el cliente. Las cláusulas 9.2 y
  9.3 del EULA entregan la copia comercial; estas dos banderas son su mitad
  técnica. Si `vendor/` está vacío, la instalación **falla** en vez de caer a
  PyPI.

### Planificador en segundo plano (`background_scheduler.py`)

APScheduler dentro del proceso:

| Trabajo | Cuándo | Para qué |
|---|---|---|
| Generación de tareas | diario (medianoche) | crea `EntregaEsperada` |
| Alertas | cada 6 h | correos de proximidad e incumplimiento |
| Respaldo | diario (2 AM) | volcado a `storage/backups/` |
| Informe de SLA | lunes 8 AM | métricas a supervisores |

Las alertas se marcan con `alerta_proximidad_en` / `alerta_incumplimiento_en`:
sin eso, la condición «vencida y pendiente» no deja de cumplirse nunca y el
mismo aviso salía cada 6 horas. Avisar de más entrena a la gente a ignorar el
sistema.

### Sesión, auditoría y acceso

`session_manager.py`: caducidad configurable, seguimiento de actividad, cierre
automático. Los eventos de seguridad (intento de acceso, salida, caducidad,
permiso denegado) se persisten en `audit_logs` (`AuditLog`), no solo en consola.

Guardas en `main.py`: `authenticated_user()` y `admin_only()`. `ADMIN` todo;
`OPERADOR` → `/mis-pendientes`, `/simulador`; `SUPERVISOR` → `/analytics`;
`TERCERO` → `/portal-terceros`. La ruta `/` redirige según el rol.

### Modelos (`app/models/meta_tables.py`, todos en un archivo)

- Ciclo: `Informe` (`cat_informes`) → `EntregaEsperada` (`entregas_esperadas`,
  estado PENDIENTE/CUMPLIDO/INCUMPLIDO, `es_manual`, `creador_manual_id`,
  `notas_admin`, `firma`) → `Comentario` (`comentarios_entrega`).
- Partes: `Usuario` (`es_admin` + `rol_sistema`), `Stakeholder` (externo, puede
  enlazar con un `Usuario` para el portal), `Participacion` (el flujo).
- Configuración: `MetaVariable`, `MetaRegla`, `ConfiguracionInformeVariable`,
  `ConjuntoDatos`, `SolicitudNuevoInforme`.
- v4: `UnidadOrganizacional` (jerárquica, `padre_id`), `Delegacion`,
  `NotificacionApp`, `ConfiguracionSistema` (ajustes en BD que sobreescriben el
  entorno). FKs `usuarios.unidad_id` y `cat_informes.unidad_id`.
- Seguridad: `AuditLog`, `PasswordReset` (un solo uso, 1 hora).
- Analítica: `ReporteAnalitico` (guarda la configuración de la consulta, no los
  datos).

### Patrón de sesión de BD

`get_db_session()` **no** hace commit automático — el llamante llama a
`db.commit()`. Si devuelves objetos ORM que se usarán tras cerrar la sesión,
`db.expunge_all()` primero. Las notificaciones en la aplicación se crean
**después** del commit principal, para no anidar sesiones.

```python
from app.core.database import get_db_session

with get_db_session() as db:
    users = db.query(Usuario).all()
    db.commit()
```

SQLAlchemy 2.0: `Session.get()`, nunca `Query.get()` (la suite lo rechaza).

### `app.storage.user`

| Clave | Tipo | Qué es |
|---|---|---|
| `authenticated` | bool | sesión iniciada |
| `email`, `nombre` | str | identidad visible |
| `user_id` | int | `Usuario.id` |
| `rol` | str | ADMIN/OPERADOR/SUPERVISOR/TERCERO |
| `es_admin` | bool | atajo de `rol == 'ADMIN'` |
| `idioma` | str | `es` o `en` |

### `recurrencia_json`

```json
{
  "modo": "CICLICO|ESPECIFICO|SEMANAL|HOURLY|FECHAS",
  "fecha_inicio": "2026-01-01",
  "intervalo": 7,
  "mover_habil": true,
  "dias_semana": [0, 1, 2, 3, 4],
  "meses": [1, 6, 12],
  "dia_corte": 15,
  "es_ultimo_dia": false,
  "fechas": ["2026-03-31", "2026-06-30"]
}
```

### Rutas (`main.py`)

| Ruta | Acceso |
|---|---|
| `/login`, `/logout`, `/reset-password`, `/terminos`, `/health` | público |
| `/dashboard`, `/variables`, `/reglas`, `/gestor-informes`, `/gestor-usuarios`, `/terceros`, `/auditoria`, `/importar-csv`, `/respaldos`, `/solicitudes`, `/conjuntos-datos`, `/entregas-manuales`, `/unidades-organizacionales`, `/configuracion-sistema` | admin |
| `/mis-pendientes`, `/simulador`, `/calendario`, `/delegaciones` | autenticado |
| `/portal-terceros`, `/analytics` | autenticado |

### Almacenamiento y Docker

`storage/uploads/` (por entrega), `storage/backups/`, `.nicegui/` (sesiones).
El `Dockerfile` espera el código en `informes/` respecto del contexto de
construcción; `docker-compose.yml` (PostgreSQL) y
`docker-compose.standalone.yml` (SQLite). Health check contra `/health` cada 30 s.

## Important Patterns

### Página nueva
1. Módulo en `app/ui/` con una función `page_*()`.
2. Ruta en `main.py` con su guarda.
3. Botón en `app/ui/layout.py`.
4. Prueba con la fixture `user` de NiceGUI — que importe no prueba que se dibuje.

### Tipo de variable nuevo
1. `TipoDatoEnum` en `app/schemas/dynamic_core.py`.
2. Render en `app/ui/variables.py` y `app/ui/simulator.py`.
3. Validación en `MotorCausalidad._evaluar()` si aplica.

### Operador o consecuencia nuevos
- Operadores: `OperadorLogicoEnum` + `MotorCausalidad._evaluar()`.
- Consecuencias: `TipoConsecuenciaEnum` + `MotorCausalidad._aplicar()` +
  `app/ui/rules.py`.

### Regla de periodicidad nueva
Toca el motor (`scheduler.py`) **y** `CAMPOS_QUE_USA` en `periodicidad.py`, y
añade el patrón a `herramientas/patrones_referencia.json` — el trabajo
`patrones` del CI comprueba que el motor produce lo que el patrón declara.

## Known Constraints

- Almacenamiento en el sistema de archivos local (sin S3).
- NiceGUI usa websockets: hace falta sesión pegajosa tras un balanceador.
- Todos los modelos en un archivo: `app/models/meta_tables.py`.
- `app/routers/` existe pero es legado y no se usa.
- Documentos de negocio (`EULA_BORRADOR.md`, `LICENCIAMIENTO.md`,
  `MODELO_LICENCIA_INFORMES.md`, los análisis de minutas) son trabajo jurídico de
  Juan: se leen, no se editan sin pedirlo. `verificar_clausulado.py` comprueba
  que los dos primeros no se contradigan.
