"""
System Settings — Admin configures global parameters stored in ConfiguracionSistema table.
"""
from nicegui import ui, app
from app.core.database import get_db_session
from app.models.meta_tables import ConfiguracionSistema


_DEFAULT_KEYS = {
    'PAIS_ISO': ('CO', 'Código ISO del país (ej: CO, US, MX)'),
    'TIMEZONE': ('America/Bogota', 'Zona horaria IANA (ej: America/Bogota)'),
    'IDIOMA_DEFECTO': ('es', 'Idioma predeterminado del sistema (es | en)'),
    'SESSION_TIMEOUT_MINUTES': ('60', 'Minutos de inactividad antes de cerrar sesión'),
    'MAX_FILE_SIZE_MB': ('50', 'Tamaño máximo de archivo en MB'),
}


def _load_config(db) -> dict:
    rows = db.query(ConfiguracionSistema).all()
    result = {k: v for k, v in {r.clave: r.valor for r in rows}.items()}
    # Fill defaults for missing keys
    for k, (default, _) in _DEFAULT_KEYS.items():
        result.setdefault(k, default)
    return result


def _save_key(db, clave, valor):
    row = db.query(ConfiguracionSistema).filter_by(clave=clave).first()
    desc = _DEFAULT_KEYS.get(clave, (None, ''))[1]
    if row:
        row.valor = valor
    else:
        db.add(ConfiguracionSistema(clave=clave, valor=valor, descripcion=desc))


def page_system_settings():
    ui.label('Configuración del Sistema').classes('text-3xl font-bold text-slate-800 mb-2')
    ui.label('Parámetros globales persistidos en base de datos').classes('text-slate-500 mb-6')

    form = {}

    with get_db_session() as db:
        config = _load_config(db)

    for k, val in config.items():
        form[k] = val

    def guardar():
        with get_db_session() as db:
            for clave, valor in form.items():
                _save_key(db, clave, str(valor) if valor is not None else '')
            db.commit()

        # Propagate language change to current user session
        if 'IDIOMA_DEFECTO' in form:
            app.storage.user['idioma'] = form['IDIOMA_DEFECTO']

        from app.services.i18n_service import invalidar_cache
        invalidar_cache()

        ui.notify('Configuración guardada', type='positive')

    def enviar_correo_prueba():
        from app.core.config import EMAIL_ENABLED
        if not EMAIL_ENABLED:
            ui.notify('Email no configurado en .env', type='warning')
            return
        admin_email = app.storage.user.get('email', '')
        if not admin_email:
            ui.notify('No se encontró email del administrador', type='warning')
            return
        import asyncio
        from app.services.email_service import EmailService
        asyncio.create_task(EmailService.enviar_credenciales(admin_email, 'Admin', '(correo de prueba)'))
        ui.notify('Correo de prueba enviado', type='positive')

    with ui.tabs().classes('w-full') as tabs:
        tab_general = ui.tab('General', icon='settings')
        tab_security = ui.tab('Seguridad', icon='lock')
        tab_email = ui.tab('Correo', icon='email')

    with ui.tab_panels(tabs, value=tab_general).classes('w-full'):
        with ui.tab_panel(tab_general):
            with ui.card().classes('w-full max-w-xl p-6'):
                ui.label('Configuración General').classes('text-lg font-bold mb-4')
                ui.input('País (código ISO)', value=form.get('PAIS_ISO', 'CO')) \
                    .bind_value(form, 'PAIS_ISO').classes('w-full').props('outlined')
                ui.input('Zona Horaria (IANA)', value=form.get('TIMEZONE', 'America/Bogota')) \
                    .bind_value(form, 'TIMEZONE').classes('w-full mt-3').props('outlined')
                ui.select(
                    {'es': 'Español', 'en': 'English'},
                    label='Idioma Predeterminado',
                    value=form.get('IDIOMA_DEFECTO', 'es')
                ).bind_value(form, 'IDIOMA_DEFECTO').classes('w-full mt-3')

        with ui.tab_panel(tab_security):
            with ui.card().classes('w-full max-w-xl p-6'):
                ui.label('Seguridad de Sesión').classes('text-lg font-bold mb-4')
                ui.number('Timeout de sesión (minutos)', value=int(form.get('SESSION_TIMEOUT_MINUTES', 60)),
                          min=5, max=480) \
                    .bind_value(form, 'SESSION_TIMEOUT_MINUTES').classes('w-full').props('outlined')
                ui.number('Tamaño máx. archivo (MB)', value=int(form.get('MAX_FILE_SIZE_MB', 50)),
                          min=1, max=500) \
                    .bind_value(form, 'MAX_FILE_SIZE_MB').classes('w-full mt-3').props('outlined')

        with ui.tab_panel(tab_email):
            with ui.card().classes('w-full max-w-xl p-6'):
                ui.label('Configuración de Correo').classes('text-lg font-bold mb-4')
                ui.label('Las credenciales SMTP se configuran en el archivo .env').classes('text-slate-500 text-sm mb-4')
                ui.button('Enviar correo de prueba', icon='send', on_click=enviar_correo_prueba) \
                    .classes('bg-blue-600 text-white')

    with ui.row().classes('w-full justify-end mt-6'):
        ui.button('Guardar cambios', icon='save', on_click=guardar).classes('bg-green-600 text-white')
