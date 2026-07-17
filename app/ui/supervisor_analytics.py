from nicegui import ui, app
from app.core.database import get_db_session
from app.models.meta_tables import EntregaEsperada, Informe, Usuario, ReporteAnalitico
from datetime import datetime, date
import json
import io

def page_supervisor():
    user_id = app.storage.user.get('user_id')
    hoy = date.today()

    # Estado del Constructor de Gráficos
    config_grafico = {
        'nombre_reporte': '',
        'tipo_grafico': 'PIE', # PIE, BAR, LINE
        'filtro_informe': None,
        'filtro_usuario': None,
        'filtro_ano': str(hoy.year),
        'desde': f'{hoy.year}-01-01',
        'hasta': hoy.isoformat(),
        'usar_rango': False,
    }
    # Datos usados en la última consulta (para exportar)
    _ultimo_resultado = {'datos': [], 'estados': {}}

    contenedor_grafico = ui.column().classes('w-full h-96 bg-white p-4 shadow-sm rounded border')

    # --- LÓGICA DE DATOS ---
    def generar_grafico():
        with get_db_session() as db:
            query = db.query(EntregaEsperada)

            # 1. Aplicar Filtros
            if config_grafico['filtro_informe']:
                query = query.filter(EntregaEsperada.informe_id == config_grafico['filtro_informe'])
            if config_grafico['filtro_usuario']:
                query = query.filter(EntregaEsperada.usuario_id == config_grafico['filtro_usuario'])

            # Filtro fecha: rango o año
            from sqlalchemy import extract
            if config_grafico.get('usar_rango') and config_grafico.get('desde') and config_grafico.get('hasta'):
                try:
                    desde_dt = datetime.strptime(config_grafico['desde'], '%Y-%m-%d')
                    hasta_dt = datetime.strptime(config_grafico['hasta'], '%Y-%m-%d').replace(hour=23, minute=59)
                    query = query.filter(
                        EntregaEsperada.fecha_programada >= desde_dt,
                        EntregaEsperada.fecha_programada <= hasta_dt
                    )
                except ValueError:
                    pass
            else:
                year_filter = int(config_grafico['filtro_ano'])
                query = query.filter(extract('year', EntregaEsperada.fecha_programada) == year_filter)

            datos = query.all()
            _ultimo_resultado['datos'] = [
                {
                    'informe': e.informe.nombre if e.informe else '?',
                    'usuario': e.usuario.nombre if e.usuario else '?',
                    'estado': e.estado,
                    'fecha': e.fecha_programada.strftime('%Y-%m-%d') if e.fecha_programada else '',
                }
                for e in datos
            ]
            
            # 2. Procesar Datos para ECharts
            if not datos:
                contenedor_grafico.clear()
                with contenedor_grafico:
                    ui.label('No hay datos con estos filtros').classes('text-gray-400 w-full text-center mt-20')
                return

            # Agrupación básica: Por Estado
            estados = {'CUMPLIDO': 0, 'EXTEMPORANEO': 0, 'PENDIENTE': 0, 'INCUMPLIDO': 0}

            for t in datos:
                st = t.estado
                # Recalcular semáforo en vivo
                if st == 'PENDIENTE' and t.fecha_limite_teorica and datetime.now() > t.fecha_limite_teorica:
                    st = 'INCUMPLIDO'
                if st in estados:
                    estados[st] += 1
            _ultimo_resultado['estados'] = estados
            
            # 3. Renderizar
            chart_option = {}
            
            if config_grafico['tipo_grafico'] == 'PIE':
                chart_option = {
                    'tooltip': {'trigger': 'item'},
                    'legend': {'bottom': '5%'},
                    'series': [{
                        'name': 'Estado', 'type': 'pie', 'radius': ['40%', '70%'],
                        'data': [
                            {'value': estados['CUMPLIDO'], 'name': 'A Tiempo', 'itemStyle': {'color': '#22c55e'}},
                            {'value': estados['EXTEMPORANEO'], 'name': 'Tarde', 'itemStyle': {'color': '#eab308'}},
                            {'value': estados['INCUMPLIDO'], 'name': 'Incumplido', 'itemStyle': {'color': '#ef4444'}},
                            {'value': estados['PENDIENTE'], 'name': 'Pendiente', 'itemStyle': {'color': '#f97316'}}
                        ]
                    }]
                }
            elif config_grafico['tipo_grafico'] == 'BAR':
                chart_option = {
                    'tooltip': {'trigger': 'axis'},
                    'xAxis': {'type': 'category', 'data': list(estados.keys())},
                    'yAxis': {'type': 'value'},
                    'series': [{'data': list(estados.values()), 'type': 'bar', 'itemStyle': {'color': '#3b82f6'}}]
                }
            elif config_grafico['tipo_grafico'] == 'LINE':
                chart_option = {
                    'tooltip': {'trigger': 'axis'},
                    'xAxis': {'type': 'category', 'data': list(estados.keys())},
                    'yAxis': {'type': 'value'},
                    'series': [{
                        'data': list(estados.values()), 'type': 'line', 'smooth': True,
                        'areaStyle': {'opacity': 0.3},
                        'itemStyle': {'color': '#8b5cf6'}
                    }]
                }

            contenedor_grafico.clear()
            with contenedor_grafico:
                ui.echart(chart_option).classes('w-full h-full')

    def guardar_analisis():
        if not config_grafico['nombre_reporte']:
            ui.notify('Asigne un nombre a este reporte', type='warning'); return
        
        with get_db_session() as db:
            rep = ReporteAnalitico(
                usuario_id=user_id,
                nombre=config_grafico['nombre_reporte'],
                configuracion=config_grafico # Guardamos el JSON completo
            )
            db.add(rep); db.commit()
        
        ui.notify('Análisis Guardado', type='positive')
        cargar_guardados()

    def cargar_guardados():
        listado_guardados.clear()
        with get_db_session() as db:
            mis_reportes = db.query(ReporteAnalitico).filter_by(usuario_id=user_id).all()
            with listado_guardados:
                if not mis_reportes: ui.label('Sin reportes guardados').classes('text-xs italic text-gray-400')
                for r in mis_reportes:
                    with ui.row().classes('w-full items-center justify-between border-b p-1'):
                        ui.label(r.nombre).classes('text-sm font-bold text-slate-700')
                        ui.button(icon='play_arrow', on_click=lambda c=r.configuracion: cargar_config(c)).props('flat round dense color=green')

    def cargar_config(cfg):
        config_grafico.update(cfg)
        generar_grafico()
        ui.notify(f'Configuración cargada: {cfg["nombre_reporte"]}')

    # --- UI ---
    # Cargar opciones
    with get_db_session() as db:
        infs = {i.id: i.nombre for i in db.query(Informe).all()}
        usrs = {u.id: u.nombre for u in db.query(Usuario).all()}

    ui.label('Inteligencia de Negocios (Supervisor)').classes('text-2xl font-bold text-slate-800 mb-4')

    with ui.row().classes('w-full gap-6 items-start'):
        
        # PANEL IZQUIERDO: CONSTRUCTOR
        with ui.card().classes('w-1/3 p-4'):
            ui.label('1. Configurar Análisis').classes('text-lg font-bold text-blue-800 mb-2')
            
            ui.select(infs, label='Filtrar por Informe', clearable=True).bind_value(config_grafico, 'filtro_informe').classes('w-full')
            ui.select(usrs, label='Filtrar por Usuario', clearable=True).bind_value(config_grafico, 'filtro_usuario').classes('w-full')

            ui.separator().classes('my-2')
            ui.label('Filtro de fecha').classes('text-xs font-bold text-slate-500')
            ui.checkbox('Usar rango de fechas').bind_value(config_grafico, 'usar_rango')
            with ui.row().classes('w-full gap-2'):
                ui.input('Desde').props('type=date').bind_value(config_grafico, 'desde').classes('flex-grow')
                ui.input('Hasta').props('type=date').bind_value(config_grafico, 'hasta').classes('flex-grow')
            ui.select(['2024', '2025', '2026', '2027'], label='Año (si no usa rango)').bind_value(config_grafico, 'filtro_ano').classes('w-full')

            ui.separator().classes('my-4')
            ui.label('Tipo de Visualización:').classes('text-xs font-bold')
            with ui.row():
                ui.radio(['PIE', 'BAR', 'LINE'], value='PIE').bind_value(config_grafico, 'tipo_grafico').props('inline')

            ui.button('Generar Gráfico', icon='query_stats', on_click=generar_grafico).classes('w-full mt-4 bg-slate-900 text-white')

            def exportar_excel():
                datos = _ultimo_resultado.get('datos', [])
                if not datos:
                    ui.notify('Genera primero el gráfico', type='warning')
                    return
                try:
                    import openpyxl
                    wb = openpyxl.Workbook()
                    ws = wb.active
                    ws.title = 'Analytics'
                    ws.append(['Informe', 'Usuario', 'Estado', 'Fecha'])
                    for row in datos:
                        ws.append([row['informe'], row['usuario'], row['estado'], row['fecha']])
                    buf = io.BytesIO()
                    wb.save(buf)
                    ui.download(buf.getvalue(), 'analytics.xlsx')
                except ImportError:
                    ui.notify('openpyxl no instalado. Ejecuta: pip install openpyxl', type='negative')

            ui.button('Exportar Excel', icon='download', on_click=exportar_excel).props('flat color=green').classes('w-full')
            
            ui.separator().classes('my-6')
            
            ui.label('2. Guardar para después').classes('text-lg font-bold text-green-800 mb-2')
            ui.input('Nombre del Reporte').bind_value(config_grafico, 'nombre_reporte').classes('w-full')
            ui.button('Guardar en "Mis Reportes"', icon='save', on_click=guardar_analisis).props('flat color=green').classes('w-full')

            ui.separator().classes('my-4')
            ui.label('Mis Reportes Guardados').classes('font-bold text-xs text-gray-500')
            listado_guardados = ui.column().classes('w-full')
            cargar_guardados()

        # PANEL DERECHO: VISUALIZACIÓN
        with ui.column().classes('w-2/3'):
            contenedor_grafico

    generar_grafico() # Init