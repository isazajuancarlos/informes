from nicegui import ui
from app.core.database import get_db_session
from app.models.meta_tables import EntregaEsperada, Usuario
from app.ui.onboarding import panel_primeros_pasos
from sqlalchemy import func
from datetime import datetime

def page_dashboard():
    # Guía de primeros pasos: se muestra mientras la configuración esté
    # incompleta, para no recibir al admin nuevo con un tablero vacío.
    panel_primeros_pasos()

    def obtener_metricas():
        with get_db_session() as db:
            total = db.query(EntregaEsperada).count()
            cumplidos = db.query(EntregaEsperada).filter_by(estado='CUMPLIDO').count()
            extemp = db.query(EntregaEsperada).filter_by(estado='EXTEMPORANEO').count()

            # Calcular INCUMPLIDO: PENDIENTE cuya fecha_limite ya pasó
            ahora = datetime.now()
            pendientes_vencidos = db.query(EntregaEsperada).filter(
                EntregaEsperada.estado == 'PENDIENTE',
                EntregaEsperada.fecha_limite_teorica < ahora
            ).count()
            pendientes_vigentes = db.query(EntregaEsperada).filter(
                EntregaEsperada.estado == 'PENDIENTE',
                EntregaEsperada.fecha_limite_teorica >= ahora
            ).count()

            top_users = db.query(Usuario.nombre, func.count(EntregaEsperada.id)) \
                .join(EntregaEsperada, Usuario.id == EntregaEsperada.usuario_id) \
                .filter(EntregaEsperada.estado.in_(['PENDIENTE', 'EXTEMPORANEO'])) \
                .group_by(Usuario.id) \
                .limit(5).all()

            u_labels = [u[0].split(' ')[0] for u in top_users]
            u_vals = [u[1] for u in top_users]
            return total, cumplidos, extemp, pendientes_vigentes, pendientes_vencidos, u_labels, u_vals

    total, ok, warn, pendientes, incumplidos, labels, vals = obtener_metricas()
    data_pie = [
        {'value': ok, 'name': 'A Tiempo', 'itemStyle': {'color': '#22c55e'}},
        {'value': warn, 'name': 'Tarde', 'itemStyle': {'color': '#eab308'}},
        {'value': pendientes, 'name': 'Pendiente', 'itemStyle': {'color': '#f97316'}},
        {'value': incumplidos, 'name': 'Incumplido', 'itemStyle': {'color': '#ef4444'}}
    ]

    ui.label('Tablero de Control').classes('text-2xl font-bold text-slate-800 mb-6')

    with ui.row().classes('w-full gap-4 mb-8'):
        def card(t, v, c, i):
            with ui.card().classes(f'flex-1 p-4 border-l-4 {c} shadow-sm flex justify-between items-center'):
                with ui.column().classes('gap-0'):
                    ui.label(t).classes('text-xs font-bold text-gray-400 uppercase')
                    ui.label(str(v)).classes('text-2xl font-bold text-slate-800')
                ui.icon(i).classes(f'text-3xl {c.replace("border-", "text-")}')

        card('Total', total, 'border-blue-500', 'analytics')
        card('Cumplidos', ok, 'border-green-500', 'check_circle')
        card('Tarde', warn, 'border-yellow-500', 'history')
        card('Pendientes', pendientes, 'border-orange-500', 'schedule')
        card('Incumplido', incumplidos, 'border-red-500', 'warning')

    with ui.row().classes('w-full gap-6 h-96'):
        with ui.card().classes('w-1/2 h-full p-4'):
            ui.label('Estado Global').classes('font-bold text-lg mb-2')
            ui.echart({
                'tooltip': {'trigger': 'item'},
                'legend': {'bottom': '5%'},
                'series': [{'type': 'pie', 'radius': ['40%', '70%'], 'data': data_pie}]
            }).classes('w-full h-full')

        with ui.card().classes('w-1/2 h-full p-4'):
            ui.label('Top Pendientes por Usuario').classes('font-bold text-lg mb-2')
            ui.echart({
                'tooltip': {'trigger': 'axis'},
                'xAxis': {'type': 'category', 'data': labels},
                'yAxis': {'type': 'value'},
                'series': [{'type': 'bar', 'data': vals, 'itemStyle': {'color': '#6366f1'}}]
            }).classes('w-full h-full')
