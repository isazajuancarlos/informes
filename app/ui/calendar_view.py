"""
Vista Calendario — muestra entregas esperadas en grilla mensual.
"""
from nicegui import ui, app
from datetime import date, timedelta, datetime
import calendar as cal_mod
from app.core.database import get_db_session
from app.models.meta_tables import EntregaEsperada, Informe, Usuario


_MESES_ES = [
    '', 'Enero', 'Febrero', 'Marzo', 'Abril', 'Mayo', 'Junio',
    'Julio', 'Agosto', 'Septiembre', 'Octubre', 'Noviembre', 'Diciembre'
]
_DIAS_ES = ['Lun', 'Mar', 'Mié', 'Jue', 'Vie', 'Sáb', 'Dom']


def page_calendar_view():
    es_admin = app.storage.user.get('es_admin', False)
    es_supervisor = app.storage.user.get('rol') == 'SUPERVISOR'
    uid = app.storage.user.get('user_id')

    hoy = date.today()
    nav = {'year': hoy.year, 'month': hoy.month}
    detalle_container = ui.column().classes('w-full')
    cal_container = ui.column().classes('w-full')

    def cargar_entregas_mes(year, month):
        primer_dia = date(year, month, 1)
        ultimo_dia = date(year, month, cal_mod.monthrange(year, month)[1])

        with get_db_session() as db:
            q = db.query(EntregaEsperada).filter(
                EntregaEsperada.fecha_limite_teorica >= datetime.combine(primer_dia, datetime.min.time()),
                EntregaEsperada.fecha_limite_teorica <= datetime.combine(ultimo_dia, datetime.max.time())
            )
            if not (es_admin or es_supervisor):
                q = q.filter(EntregaEsperada.usuario_id == uid)

            entregas = q.all()
            # Group by day
            por_dia: dict[int, list] = {}
            for e in entregas:
                d = e.fecha_limite_teorica.day
                por_dia.setdefault(d, []).append({
                    'estado': e.estado,
                    'nombre': e.informe.nombre if e.informe else '?',
                    'usuario': e.usuario.nombre if e.usuario else '?',
                    'id': e.id,
                    'fecha_str': e.fecha_limite_teorica.strftime('%d/%m/%Y %H:%M'),
                })
        return por_dia

    def color_estado(estado):
        return {'PENDIENTE': 'orange', 'CUMPLIDO': 'green', 'INCUMPLIDO': 'red',
                'EXTEMPORANEO': 'amber'}.get(estado, 'grey')

    def mostrar_detalle(dia_entregas: list, dia: int):
        detalle_container.clear()
        with detalle_container:
            with ui.card().classes('w-full p-4'):
                ui.label(f'Entregas del día {dia}').classes('text-lg font-bold mb-3')
                if not dia_entregas:
                    ui.label('Sin entregas este día').classes('text-slate-400')
                    return
                for e in dia_entregas:
                    with ui.row().classes('items-center gap-2 w-full border-b py-2'):
                        ui.badge(e['estado'], color=color_estado(e['estado'])).classes('text-white')
                        with ui.column().classes('gap-0 flex-grow'):
                            ui.label(e['nombre']).classes('font-medium text-sm')
                            ui.label(f"{e['usuario']} — {e['fecha_str']}").classes('text-xs text-slate-400')

    def renderizar():
        cal_container.clear()
        year, month = nav['year'], nav['month']
        por_dia = cargar_entregas_mes(year, month)

        with cal_container:
            # Navigation header
            with ui.row().classes('w-full items-center justify-between mb-4'):
                ui.button(icon='chevron_left', on_click=lambda: cambiar_mes(-1)).props('flat round')
                ui.label(f'{_MESES_ES[month]} {year}').classes('text-xl font-bold')
                ui.button(icon='chevron_right', on_click=lambda: cambiar_mes(1)).props('flat round')
                ui.button('Hoy', on_click=lambda: ir_hoy()).props('flat size=sm')

            # Day headers
            with ui.grid(columns=7).classes('w-full gap-1'):
                for dia_label in _DIAS_ES:
                    ui.label(dia_label).classes('text-center text-xs font-bold text-slate-500 py-1')

                # Empty cells before first day
                primer_dia_semana = date(year, month, 1).weekday()  # 0=Monday
                for _ in range(primer_dia_semana):
                    ui.element('div').classes('min-h-[80px]')

                # Day cells
                num_dias = cal_mod.monthrange(year, month)[1]
                for dia in range(1, num_dias + 1):
                    entregas_dia = por_dia.get(dia, [])
                    es_hoy = (year == hoy.year and month == hoy.month and dia == hoy.day)

                    cell_cls = 'min-h-[80px] rounded-lg border p-1 cursor-pointer hover:bg-blue-50 transition-colors'
                    if es_hoy:
                        cell_cls += ' border-blue-400 bg-blue-50'
                    else:
                        cell_cls += ' border-slate-200'

                    dia_captured = dia
                    entregas_captured = list(entregas_dia)

                    with ui.element('div').classes(cell_cls).on('click', lambda _, d=dia_captured, e=entregas_captured: mostrar_detalle(e, d)):
                        num_cls = 'text-sm font-bold mb-1'
                        if es_hoy:
                            num_cls += ' text-blue-600'
                        ui.label(str(dia)).classes(num_cls)

                        # Show up to 3 badges
                        for ent in entregas_dia[:3]:
                            ui.badge('', color=color_estado(ent['estado'])).classes('w-2 h-2 rounded-full inline-block mr-1')
                        if len(entregas_dia) > 3:
                            ui.label(f'+{len(entregas_dia)-3}').classes('text-xs text-slate-400')

    def cambiar_mes(delta: int):
        m = nav['month'] + delta
        y = nav['year']
        if m > 12:
            m = 1
            y += 1
        elif m < 1:
            m = 12
            y -= 1
        nav['month'] = m
        nav['year'] = y
        renderizar()

    def ir_hoy():
        nav['year'] = hoy.year
        nav['month'] = hoy.month
        renderizar()

    ui.label('Calendario de Entregas').classes('text-3xl font-bold text-slate-800 mb-2')
    ui.label('Haz clic en un día para ver el detalle de las entregas').classes('text-slate-500 mb-6')

    renderizar()
    detalle_container
