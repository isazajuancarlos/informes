# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a **Report Management System v4.0** ("Sistema Gestión de Informes") built with NiceGUI, FastAPI, SQLAlchemy, and PostgreSQL. It manages scheduled reports with dynamic variables, business rules validation, task assignment, stakeholder notifications, in-app notifications, organizational units, task delegation, and multi-language support (ES/EN). The system supports multiple user roles (ADMIN, OPERADOR, SUPERVISOR, TERCERO) with role-based access control.

## Development Commands

### First-Time Setup
```bash
pip install -r requirements.txt
cp .env.example .env
# Edit .env with your credentials
python reset_total.py   # Creates all tables
python seed_data.py     # Populates test data
```

### Running the Application
```bash
python main.py
```
Runs on port **8003**. Access at `http://localhost:8003`. Hot-reload is enabled (`reload=True`).

On startup, expect: `✅ Background Scheduler iniciado`

### v4.0 Migration (run once after upgrading from v3)
```bash
pip install -r requirements.txt   # installs openpyxl
python migrate_v4.py              # adds new tables/columns safely
```

### Schema migrations
```bash
python migrar_esquema.py   # Adds missing columns. Idempotent, verified, both dialects.
```

Run it after every upgrade, before starting the app. It adds `firma`,
`fecha_vencimiento_nominal` and `tipo_ajuste` to `entregas_esperadas`.

On a **fresh** install the tables do not exist yet, so create the schema first
(`python -m app.init_db`) and then migrate. The migration says so if you get the
order wrong.

The app listens on **port 8003** (`main.py`), not 8080.

**Do not use `migrate_firma.py`**: it uses PostgreSQL-only syntax
(`ADD COLUMN IF NOT EXISTS`, `information_schema`) and silently did nothing on
SQLite, which is the engine for the `standalone` and `cloud` profiles. It now
refuses to run.

### Database Operations
```bash
python -m app.init_db    # Safe: creates missing tables only
python seed_data.py      # Seed users/variables (admin@co.co/123456, operador@co.co/123456)
python reset_total.py    # WARNING: drops ALL tables and recreates them
```

### Demo Data (Fictional)
```bash
python importar_datos_demo.py --dry-run   # Test import of 397 synthetic reports
python importar_datos_demo.py             # Import demo data
```

### Environment Variables
All credentials live in `.env` (never commit it). Key variables:
- `DATABASE_URL` — PostgreSQL connection (default: `postgresql://postgres:admin@localhost/informes`)
- `SENDER_EMAIL`, `SENDER_PASSWORD` — SMTP credentials; `EMAIL_ENABLED` is auto-set if both present
- `SMTP_HOST`, `SMTP_PORT` — SMTP server (default: `smtp.gmail.com:587`)
- `STORAGE_SECRET` — Session encryption key
- `SESSION_TIMEOUT_MINUTES` — Session expiration (default: 60)
- `MAX_RETROACTIVE_DAYS` — Task retroactivity limit (default: 730)
- `ENABLE_AUTO_SCHEDULER` — Enable background automation (default: true)
- `APP_BASE_URL` — Used in email links (default: `http://localhost:8003`)
- `PAIS_ISO` — Country for holiday calendar (default: `CO`); unsupported countries fall back gracefully
- `TIMEZONE` — IANA timezone (default: `America/Bogota`)
- `IDIOMA_DEFECTO` — Default language `es` or `en` (default: `es`)
- `MAX_FILE_SIZE_MB` — Upload size limit (default: 50)
- `ALLOWED_EXTENSIONS` — Comma-separated (default: `.pdf,.xlsx,.docx,.zip`)

## Architecture

### Core System Components

**1. Meta-Programming System**
Reports are defined dynamically rather than hardcoded:
- `MetaVariable`: Catalog of reusable input fields (TEXTO, NUMERO, FECHA, MONEDA, HORA_MILITAR, TEXTO_ALFANUM, BOOLEANO, LISTA)
- `MetaRegla`: Business rules that trigger consequences based on variable values
- `ConfiguracionInformeVariable`: Links variables to specific reports with custom parameters

**2. Causality Engine** (`app/services/causality_engine.py`)
Evaluates business rules against user inputs. Returns a "plan" with alerts, required files, and roles to notify. Operators: EQUALS, GREATER_THAN, LESS_THAN, GREATER_THAN_OR_EQUAL, LESS_THAN_OR_EQUAL, CONTAINS, IN_LIST. Consequences: FILE_REQ, DATE_CHANGE, NOTIFY_ROLE, BLOCK.

**3. Task Scheduler** (`app/services/scheduler.py`)
Generates `EntregaEsperada` (expected deliveries) from report recurrence configs. Modes: CICLICO (every N days), ESPECIFICO (specific days of month), SEMANAL (weekly), HOURLY. Handles Colombian business day adjustments via `MotorReglasDinamicas`.

**4. Background Scheduler** (`app/services/background_scheduler.py`)
APScheduler jobs running in-process:

| Task | Schedule | Purpose |
|------|----------|---------|
| Task Generation | Daily (midnight) | Creates EntregaEsperada from schedules |
| Alert Processing | Every 6 hours | Emails for deadlines/breaches |
| Auto Backup | Daily (2 AM) | PostgreSQL dump to `storage/backups/` |
| SLA Report | Weekly (Mon 8 AM) | Metrics email to supervisors |

**5. Session Management** (`app/services/session_manager.py`)
Configurable timeout, activity tracking, auto-logout. Security events (login attempts, logouts, permission denials) are persisted to the `audit_logs` DB table (`AuditLog` model) and also printed to console.

**6. Calendar Engine** (`app/services/calendar_engine.py`)
Holiday calendar via `holidays` library using `PAIS_ISO` config. Falls back gracefully for unsupported countries. Provides weekend detection and business day adjustments.

**7. Role-Based Access Control** (enforced in `main.py`)
- `ADMIN`: Full access — dashboard, users, reports, variables, rules, backups
- `OPERADOR`: Task execution — `/mis-pendientes`, `/simulador`
- `SUPERVISOR`: Read-only analytics — `/analytics`
- `TERCERO`: Stakeholder portal — `/portal-terceros`

Route guards: `authenticated_user()` and `admin_only()` in `main.py`.

### Key Models (`app/models/meta_tables.py`)

**Report Lifecycle:**
1. `Informe` (`cat_informes`): Report definition — schedule, assignments, v3.0 fields: `tipo_flujo` (ENVIO_INTERNO/ENVIO_EXTERNO/RECEPCION_EXTERNA/BIDIRECCIONAL), `subsistema`, `conjunto_datos_id`, `tiene_acuerdo_confidencialidad`, `url_plantilla`, `ruta_plantilla`, `justificacion_normatividad`, `aplicativo_origen`, `ruta_envio`, `ruta_respaldo`
2. `EntregaEsperada` (`entregas_esperadas`): Task instances — `estado` (PENDIENTE/CUMPLIDO/INCUMPLIDO), `es_manual` flag for on-demand deliveries, `creador_manual_id`, `notas_admin`
3. `Comentario` (`comentarios_entrega`): Chat messages per delivery

**Users & Stakeholders:**
- `Usuario`: Internal users; `es_admin` + `rol_sistema` (ADMIN/OPERADOR/SUPERVISOR/TERCERO)
- `Stakeholder`: External parties; can link to a `Usuario` for portal access
- `ReporteAnalitico`: Saved chart configurations created by Supervisors (stores query config, not data)
- Many-to-many: reports ↔ stakeholders via `solicitantes`/`destinatarios` junction tables

**Assignment Table (`asignaciones_informe_usuario`):**
Associates users to reports with extra columns: `tipo_responsabilidad` (EJECUTA/ENVIA/RECIBE/SUPERVISA), `es_principal` (bool), `orden` (int).

**Security & Audit:**
- `AuditLog` (`audit_logs`): Persistent security event log (events: login_attempt, logout, session_timeout, permission_denied). Complies with Ley 1581/2012 (Colombian data protection).
- `PasswordReset`: Time-limited single-use password reset tokens (1 hour expiry)

**Data Organization (v3.0):**
- `ConjuntoDatos` (`conjuntos_datos`): Data sets catalog (RIPS, RUAF, etc.) with optional `responsable_id`
- `SolicitudNuevoInforme` (`solicitudes_terceros`): Stakeholder requests for new reports, approved/denied by admin

**v4.0 Models:**
- `UnidadOrganizacional` (`unidades_organizacionales`): Hierarchical org structure; `padre_id` self-FK, `nivel`, `codigo`
- `Delegacion` (`delegaciones`): Temporary task delegation between users; `delegante_id`, `delegado_id`, date range
- `NotificacionApp` (`notificaciones_app`): In-app notifications; `tipo` (INFO/ALERTA/URGENTE), `leida`, `url_destino`
- `ConfiguracionSistema` (`configuracion_sistema`): Key-value system settings stored in DB (runtime overrides for env vars)
- FK additions: `usuarios.unidad_id`, `cat_informes.unidad_id` → `unidades_organizacionales`

**v4.0 Services:**
- `app/services/i18n_service.py`: `t(key, **kwargs)` translation function; reads `app/i18n/es.json` and `en.json`; language from `app.storage.user['idioma']`
- `app/services/notification_service.py`: `NotificationService` — `crear()`, `marcar_leida()`, `marcar_todas_leidas()`, `obtener_no_leidas()`, `obtener_recientes()`

### Database Session Pattern

`get_db_session()` does **not** auto-commit — callers must call `db.commit()` explicitly. When returning ORM objects that will be used after the session closes, call `db.expunge_all()` first.

```python
from app.core.database import get_db_session

with get_db_session() as db:
    users = db.query(Usuario).all()
    db.commit()
```

Always create in-app notifications **after** the main `db.commit()` to avoid nested session conflicts.

### Session Storage Keys (`app.storage.user`)

| Key | Type | Description |
|-----|------|-------------|
| `authenticated` | bool | Whether user is logged in |
| `email` | str | User email |
| `user_id` | int | `Usuario.id` |
| `nombre` | str | Display name |
| `rol` | str | ADMIN/OPERADOR/SUPERVISOR/TERCERO |
| `es_admin` | bool | Shortcut for `rol == 'ADMIN'` |
| `idioma` | str | `es` or `en` for i18n |

### Recurrence Configuration

`Informe.recurrencia_json` structure:
```json
{
  "modo": "CICLICO|ESPECIFICO|SEMANAL|HOURLY",
  "fecha_inicio": "2025-01-01",
  "intervalo": 7,
  "mover_habil": true,
  "dias_semana": [0, 1, 2, 3, 4],
  "meses": [1, 6, 12],
  "dia_corte": 15,
  "es_ultimo_dia": false
}
```

### Routes (registered in `main.py`)

| Route | Access | Page |
|-------|--------|------|
| `/login`, `/logout`, `/reset-password` | Public | Auth |
| `/terminos` | Public | Privacy policy (Ley 1581/2012) |
| `/dashboard`, `/variables`, `/reglas`, `/gestor-informes`, `/gestor-usuarios`, `/terceros`, `/auditoria`, `/importar-csv`, `/respaldos`, `/solicitudes`, `/conjuntos-datos`, `/entregas-manuales`, `/unidades-organizacionales`, `/configuracion-sistema` | Admin only | Admin management |
| `/mis-pendientes`, `/simulador`, `/calendario`, `/delegaciones` | Authenticated | Operator tasks |
| `/portal-terceros` | Authenticated | Stakeholder view |
| `/analytics` | Authenticated | Supervisor BI |
| `/health` | Public (GET) | Docker health check |

### Storage Locations

- `storage/uploads/` — User-uploaded files (organized by delivery ID)
- `storage/backups/` — SQL dumps
- `.nicegui/` — NiceGUI session persistence

### Docker

The `Dockerfile` expects source at `informes/` relative to build context (part of a multi-service repo). The health check hits `/health` every 30s. Exposes port 8003.

## Important Patterns

### Creating a New Page
1. Create module in `app/ui/` with a `page_*()` function
2. Register route in `main.py` with appropriate guard
3. Add navigation button in `app/ui/layout.py`

### Adding a New Variable Type
1. Add to `TipoDatoEnum` in `app/schemas/dynamic_core.py`
2. Update input rendering in `app/ui/variables.py` and `app/ui/simulator.py`
3. Update validation in `MotorCausalidad._evaluar()` if needed

### Adding a New Operator / Consequence Type
- Operators: add to `OperadorLogicoEnum`, implement in `MotorCausalidad._evaluar()`
- Consequences: add to `TipoConsecuenciaEnum`, handle in `MotorCausalidad._aplicar()`, update `app/ui/rules.py`

## Known Constraints

- No automated tests — manual testing via `/simulador` and `seed_data.py`
- Single PostgreSQL database, local filesystem storage (no cloud/S3)
- NiceGUI uses websockets — requires sticky sessions for load balancing
- All models are in one file: `app/models/meta_tables.py`
- `app/routers/` directory exists but is legacy/unused
