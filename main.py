import sys
import os
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

from nicegui import ui, app
from app.init_db import init_db

# Imports
from app.ui.layout import menu_principal
from app.ui.login import page_login
from app.ui.variables import page_variables
from app.ui.rules import page_rules
from app.ui.reports_manager import page_reports_manager
from app.ui.simulator import page_simulator
from app.ui.my_tasks import page_my_tasks
from app.ui.dashboard import page_dashboard
from app.ui.audit import page_audit
from app.ui.users_manager import page_users_manager
from app.ui.stakeholders_manager import page_stakeholders
from app.ui.stakeholder_portal import page_stakeholder_portal
from app.ui.csv_importer import page_csv_importer
from app.ui.backup_manager import page_backup_manager
from app.ui.reset_password import page_reset_password
from app.ui.supervisor_analytics import page_supervisor
from app.ui.requests_manager import page_requests_manager
from app.ui.conjuntos_datos import page_conjuntos_datos  # NUEVO v3.0
from app.ui.entregas_manuales import page_entregas_manuales  # NUEVO v3.0
from app.ui.terminos import page_terminos
from app.ui.system_settings import page_system_settings  # NUEVO v4.0
from app.ui.org_units import page_org_units  # NUEVO v4.0
from app.ui.delegation import page_delegation  # NUEVO v4.0
from app.ui.calendar_view import page_calendar_view  # NUEVO v4.0
from app.services.background_scheduler import init_scheduler

init_db()

# Iniciar scheduler automático en segundo plano
init_scheduler()

# --- SEGURIDAD ---
from app.services.session_manager import SessionManager, SecurityAudit

def authenticated_user():
    if not app.storage.user.get('authenticated', False):
        ui.navigate.to('/login'); return False
    if SessionManager.is_session_expired():
        email = app.storage.user.get('email', 'unknown')
        SecurityAudit.log_session_timeout(email)
        SessionManager.invalidate_session()
        ui.notify('Sesión expirada. Inicie sesión nuevamente.', type='warning')
        ui.navigate.to('/login'); return False
    SessionManager.update_activity()
    return True

def admin_only():
    if not authenticated_user(): return False
    if not app.storage.user.get('es_admin', False):
        email = app.storage.user.get('email', 'unknown')
        SecurityAudit.log_permission_denied(email, 'admin_area')
        ui.notify('Acceso Restringido', type='negative'); ui.navigate.to('/'); return False
    return True

# --- RUTAS ---
@ui.page('/login')
def route_login():
    if app.storage.user.get('authenticated', False): ui.navigate.to('/'); return
    page_login()

@ui.page('/logout')
def route_logout():
    from app.services.session_manager import SessionManager, SecurityAudit
    email = app.storage.user.get('email', 'unknown')
    SecurityAudit.log_logout(email)
    SessionManager.invalidate_session()
    ui.navigate.to('/login')
@ui.page('/reset-password')
def route_reset(token: str=None): page_reset_password(token)

# Admin
@ui.page('/dashboard')
def r_dash(): 
    if admin_only(): menu_principal(); page_dashboard()
@ui.page('/variables')
def r_vars(): 
    if admin_only(): menu_principal(); page_variables()
@ui.page('/reglas')
def r_rules(): 
    if admin_only(): menu_principal(); page_rules()
@ui.page('/gestor-informes')
def r_rep(): 
    if admin_only(): menu_principal(); page_reports_manager()
@ui.page('/gestor-usuarios')
def r_users(): 
    if admin_only(): menu_principal(); page_users_manager()
@ui.page('/terceros')
def r_stake(): 
    if admin_only(): menu_principal(); page_stakeholders()
@ui.page('/auditoria')
def r_audit(): 
    if admin_only(): menu_principal(); page_audit()
@ui.page('/importar-csv')
def r_csv(): 
    if admin_only(): menu_principal(); page_csv_importer()
@ui.page('/respaldos')
def r_back():
    if admin_only(): menu_principal(); page_backup_manager()
@ui.page('/solicitudes')
def r_requests():
    if admin_only(): menu_principal(); page_requests_manager()
@ui.page('/conjuntos-datos')
def r_conjuntos():
    if admin_only(): menu_principal(); page_conjuntos_datos()
@ui.page('/entregas-manuales')
def r_entregas_manuales():
    if admin_only(): menu_principal(); page_entregas_manuales()

# Política de datos y términos (acceso público, sin autenticación)
@ui.page('/terminos')
def r_terminos():
    page_terminos()

# NUEVO v4.0
@ui.page('/configuracion-sistema')
def r_settings():
    if admin_only(): menu_principal(); page_system_settings()

@ui.page('/unidades-organizacionales')
def r_org():
    if admin_only(): menu_principal(); page_org_units()

@ui.page('/delegaciones')
def r_deleg():
    if authenticated_user(): menu_principal(); page_delegation()

@ui.page('/calendario')
def r_cal():
    if authenticated_user(): menu_principal(); page_calendar_view()

# Operativo
@ui.page('/mis-pendientes')
def r_tasks(): 
    if authenticated_user(): menu_principal(); page_my_tasks()
@ui.page('/simulador')
def r_sim(): 
    if authenticated_user(): menu_principal(); page_simulator()

# Terceros
@ui.page('/portal-terceros')
def r_portal(): 
    if authenticated_user(): menu_principal(); page_stakeholder_portal()

# Supervisores
@ui.page('/analytics')
def r_analytics():
    if authenticated_user(): # Supervisor no es admin, pero está autenticado
        menu_principal()
        page_supervisor()

# Home Routing
@ui.page('/')
def index():
    if not app.storage.user.get('authenticated', False): ui.navigate.to('/login'); return
    
    rol = app.storage.user.get('rol', 'OPERADOR')
    
    if rol == 'ADMIN': ui.navigate.to('/dashboard')
    elif rol == 'SUPERVISOR': ui.navigate.to('/analytics')
    elif rol == 'TERCERO': ui.navigate.to('/portal-terceros')
    else: ui.navigate.to('/mis-pendientes')

# Health check endpoint para Docker/Kubernetes
@app.get('/health')
def health_check():
    """Endpoint de salud para el health check del contenedor."""
    from app.core.database import engine
    from sqlalchemy import text
    try:
        with engine.connect() as conn:
            conn.execute(text("SELECT 1"))
        db_status = "ok"
    except Exception:
        db_status = "error"
    return {"status": "healthy" if db_status == "ok" else "degraded", "db": db_status, "service": "informes"}

if __name__ in {"__main__", "__mp_main__"}:
    from app.core.config import NOMBRE_PRODUCTO, STORAGE_SECRET
    # reload por defecto en dev; se apaga en contenedores (RELOAD=false).
    _reload = os.getenv("RELOAD", "true").strip().lower() == "true"
    # El puerto estaba fijo en 8003. Configurarlo permite levantar una segunda
    # instancia sin pelearse con la de desarrollo — que es lo que necesitan las
    # pruebas visuales para medir la pantalla en un navegador de verdad— y hace
    # falta igualmente para desplegar detrás de un proxy.
    _puerto = int(os.getenv("PORT", "8003"))
    ui.run(title=NOMBRE_PRODUCTO, language="es", favicon="📊", host="0.0.0.0", port=_puerto, storage_secret=STORAGE_SECRET, reload=_reload, show=False)