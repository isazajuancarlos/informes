from nicegui import ui, app
from app.services.notification_service import NotificationService
from app.core.tiempo import formatear_utc
from app.services.license_service import estado as licencia_estado
from app.core.config import NOMBRE_PRODUCTO


def _render_notification_bell(usuario_id):
    """Renders the notification bell with badge and dropdown."""
    if not usuario_id:
        return

    no_leidas = NotificationService.obtener_no_leidas(usuario_id)
    count = len(no_leidas)

    def abrir_notificaciones():
        recientes = NotificationService.obtener_recientes(usuario_id, limit=10)

        with ui.dialog() as d, ui.card().classes('min-w-[400px] max-w-[500px] p-0'):
            with ui.row().classes('w-full items-center justify-between p-4 border-b'):
                ui.label('Notificaciones').classes('font-bold text-lg')
                ui.button(icon='close', on_click=d.close).props('flat round size=sm')

            with ui.scroll_area().classes('w-full max-h-[400px]'):
                if not recientes:
                    with ui.column().classes('w-full p-8 items-center'):
                        ui.icon('notifications_none', size='xl').classes('text-slate-300')
                        ui.label('Sin notificaciones').classes('text-slate-400')
                else:
                    tipo_icon = {'INFO': ('info', 'blue'), 'ALERTA': ('warning', 'orange'), 'URGENTE': ('error', 'red')}
                    for n in recientes:
                        icon_name, color = tipo_icon.get(n.tipo, ('circle', 'grey'))
                        bg = '' if n.leida else 'bg-blue-50'
                        with ui.row().classes(f'w-full items-start gap-3 p-3 border-b hover:bg-slate-50 cursor-pointer {bg}'):
                            ui.icon(icon_name, color=color).classes('mt-1 shrink-0')
                            with ui.column().classes('gap-0 flex-grow'):
                                ui.label(n.titulo).classes('font-medium text-sm')
                                ui.label(n.mensaje[:100]).classes('text-xs text-slate-500')
                                ui.label(formatear_utc(n.fecha_creacion, '%d/%m/%Y %H:%M')).classes('text-xs text-slate-400')

            with ui.row().classes('w-full justify-between p-3 border-t'):
                ui.button('Marcar todas leídas', on_click=lambda: _marcar_todo_y_cerrar(usuario_id, d)) \
                    .props('flat size=sm')
                ui.button('Ver todas', on_click=lambda: ui.navigate.to('/mis-pendientes')) \
                    .props('flat size=sm color=primary')

        d.open()

    with ui.button(icon='notifications', on_click=abrir_notificaciones).props('flat round color=white'):
        if count > 0:
            ui.badge(str(count), color='red').props('floating')


def _marcar_todo_y_cerrar(usuario_id, dialog):
    NotificationService.marcar_todas_leidas(usuario_id)
    dialog.close()
    ui.navigate.reload()


def _aviso_de_licencia():
    """Aviso de licencia en la cabecera. **Nunca bloquea nada.**

    Es la aplicación del principio del modo degradado: la ausencia de una capa
    opcional no puede impedirle al cliente cumplir una obligación con un
    tercero. Si una licencia vencida impidiera registrar una entrega, Xiliux
    pasaría a ser la causa del incumplimiento del cliente ante Supersalud.

    Lo que aporta es VISIBILIDAD; el remedio ante el exceso es contractual.
    Por eso distingue «no hay licencia» —normal en evaluación— de «la licencia
    no cuadra», que es lo que sí merece color de alarma: si ambas se vieran
    igual, el aviso de verdad se acabaría ignorando.
    """
    est = licencia_estado()
    if not est.hay_que_avisar:
        return
    color, icono = ('bg-red-600', 'gpp_bad') if est.es_grave else ('bg-amber-600', 'info')
    with ui.element('div').classes(f'{color} px-3 py-1 rounded flex items-center gap-2'):
        ui.icon(icono).classes('text-base')
        ui.label(est.mensaje).classes('text-xs')


def menu_principal():
    usuario_id = app.storage.user.get('user_id')

    with ui.header().classes('bg-slate-900 text-white shadow-lg items-center justify-between h-16 px-4'):
        with ui.row().classes('items-center gap-3'):
            ui.icon('analytics').classes('text-3xl text-blue-400')
            ui.label(NOMBRE_PRODUCTO).classes('text-lg font-bold')
            _aviso_de_licencia()

        with ui.row().classes('items-center gap-3'):
            # Language selector
            with ui.button_group().props('flat'):
                ui.button('ES', on_click=lambda: _cambiar_idioma('es')) \
                    .props('flat size=sm color=white').classes('text-xs opacity-70 hover:opacity-100')
                ui.button('EN', on_click=lambda: _cambiar_idioma('en')) \
                    .props('flat size=sm color=white').classes('text-xs opacity-70 hover:opacity-100')

            # Notification bell
            _render_notification_bell(usuario_id)

            nom = app.storage.user.get('nombre', 'User')
            rol = "Admin" if app.storage.user.get('es_admin') else app.storage.user.get('rol', 'User')
            with ui.column().classes('gap-0 items-end'):
                ui.label(nom).classes('text-sm font-bold')
                ui.label(rol).classes('text-[10px] uppercase text-slate-300')
            ui.avatar(icon='person', color='white', text_color='slate-900')

    with ui.left_drawer(value=True).classes('bg-slate-50 border-r border-slate-200 column justify-between w-64'):
        with ui.scroll_area().classes('w-full flex-grow'):
            with ui.column().classes('w-full gap-1 p-2'):

                if app.storage.user.get('es_admin', False):
                    ui.label('ESTRATEGIA').classes('text-[10px] font-bold text-slate-400 mt-4 ml-3 mb-1')
                    ui.button('Dashboard', on_click=lambda: ui.navigate.to('/dashboard')).props('flat align=left icon=dashboard').classes('w-full text-slate-700')
                    ui.button('Auditoría', on_click=lambda: ui.navigate.to('/auditoria')).props('flat align=left icon=manage_search').classes('w-full text-slate-700')
                    ui.button('Calendario', on_click=lambda: ui.navigate.to('/calendario')).props('flat align=left icon=calendar_month').classes('w-full text-slate-700')

                    ui.separator().classes('my-2 opacity-50')
                    ui.label('CONFIGURACIÓN').classes('text-[10px] font-bold text-slate-400 ml-3 mb-1')
                    ui.button('Gestor Informes', on_click=lambda: ui.navigate.to('/gestor-informes')).props('flat align=left icon=inventory_2').classes('w-full text-slate-700')
                    ui.button('Gestor Usuarios', on_click=lambda: ui.navigate.to('/gestor-usuarios')).props('flat align=left icon=group').classes('w-full text-slate-700')
                    ui.button('Terceros', on_click=lambda: ui.navigate.to('/terceros')).props('flat align=left icon=business').classes('w-full text-slate-700')
                    ui.button('Solicitudes', on_click=lambda: ui.navigate.to('/solicitudes')).props('flat align=left icon=request_quote').classes('w-full text-slate-700')
                    ui.button('Conjuntos de Datos', on_click=lambda: ui.navigate.to('/conjuntos-datos')).props('flat align=left icon=dataset').classes('w-full text-slate-700')
                    ui.button('Entregas Manuales', on_click=lambda: ui.navigate.to('/entregas-manuales')).props('flat align=left icon=add_task').classes('w-full text-slate-700')
                    ui.button('Unidades Org.', on_click=lambda: ui.navigate.to('/unidades-organizacionales')).props('flat align=left icon=account_tree').classes('w-full text-slate-700')
                    ui.button('Delegaciones', on_click=lambda: ui.navigate.to('/delegaciones')).props('flat align=left icon=swap_horiz').classes('w-full text-slate-700')

                    with ui.expansion('Técnico', icon='settings').classes('w-full text-slate-700'):
                        ui.button('Variables', on_click=lambda: ui.navigate.to('/variables')).props('flat align=left icon=input size=sm').classes('w-full pl-8 text-slate-600')
                        ui.button('Reglas', on_click=lambda: ui.navigate.to('/reglas')).props('flat align=left icon=psychology size=sm').classes('w-full pl-8 text-slate-600')
                        ui.button('Configuración', on_click=lambda: ui.navigate.to('/configuracion-sistema')).props('flat align=left icon=tune size=sm').classes('w-full pl-8 text-slate-600')

                    ui.separator().classes('my-2 opacity-50')
                    ui.button('Carga Masiva', on_click=lambda: ui.navigate.to('/importar-csv')).props('flat align=left icon=file_upload').classes('w-full text-slate-700')
                    ui.button('Respaldos', on_click=lambda: ui.navigate.to('/respaldos')).props('flat align=left icon=save').classes('w-full text-slate-700')

                rol = app.storage.user.get('rol')
                if rol == 'TERCERO':
                    ui.label('CONTROL').classes('text-[10px] font-bold text-slate-400 ml-3 mb-1')
                    ui.button('Portal Terceros', on_click=lambda: ui.navigate.to('/portal-terceros')).props('flat align=left icon=visibility').classes('w-full text-purple-800 font-bold bg-purple-50')

                # ZONA SUPERVISOR
                if app.storage.user.get('rol') == 'SUPERVISOR':
                    ui.label('INTELIGENCIA').classes('text-[10px] font-bold text-slate-400 mt-4 ml-3 mb-1')
                    ui.button('Analytics / BI', on_click=lambda: ui.navigate.to('/analytics')) \
                        .props('flat align=left icon=insights').classes('w-full text-teal-800 font-bold bg-teal-50')
                    ui.button('Calendario', on_click=lambda: ui.navigate.to('/calendario')) \
                        .props('flat align=left icon=calendar_month').classes('w-full text-slate-600')
                    ui.button('Auditoría (Lectura)', on_click=lambda: ui.navigate.to('/auditoria')) \
                        .props('flat align=left icon=manage_search').classes('w-full text-slate-600')

                if rol == 'OPERADOR' or app.storage.user.get('es_admin'):
                    ui.separator().classes('my-2 opacity-50')
                    ui.label('OPERACIÓN').classes('text-[10px] font-bold text-slate-400 ml-3 mb-1')
                    ui.button('Mis Pendientes', on_click=lambda: ui.navigate.to('/mis-pendientes')).props('flat align=left icon=assignment_late').classes('w-full text-blue-800 font-bold bg-blue-50')
                    ui.button('Simulador', on_click=lambda: ui.navigate.to('/simulador')).props('flat align=left icon=science').classes('w-full text-slate-500')
                    if rol == 'OPERADOR':
                        ui.button('Calendario', on_click=lambda: ui.navigate.to('/calendario')).props('flat align=left icon=calendar_month').classes('w-full text-slate-500')
                        ui.button('Delegaciones', on_click=lambda: ui.navigate.to('/delegaciones')).props('flat align=left icon=swap_horiz').classes('w-full text-slate-500')

        with ui.column().classes('w-full p-4 border-t border-slate-200 bg-white gap-1'):
            ui.button('Política de Datos', on_click=lambda: ui.navigate.to('/terminos')) \
                .props('flat align=left icon=privacy_tip size=sm') \
                .classes('w-full text-slate-400 text-xs')
            ui.button('Salir', on_click=lambda: ui.navigate.to('/logout')).props('flat align=left icon=logout text-color=red').classes('w-full border border-red-100 rounded-md font-bold')


def _cambiar_idioma(lang: str):
    app.storage.user['idioma'] = lang
    ui.navigate.reload()
