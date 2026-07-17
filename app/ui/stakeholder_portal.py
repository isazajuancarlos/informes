from nicegui import ui, app
from app.core.database import get_db_session
from app.models.meta_tables import EntregaEsperada, SolicitudNuevoInforme, Stakeholder
from datetime import datetime
import os

def page_stakeholder_portal():
    uid = app.storage.user.get('user_id')
    sol = {'tit': '', 'desc': ''}

    def cargar():
        with get_db_session() as db:
            me = db.query(Stakeholder).filter(Stakeholder.usuario_id == uid).first()
            if not me: ui.label('Usuario no vinculado a Entidad').classes('text-red'); return
            
            all_e = db.query(EntregaEsperada).all()
            rows = []
            for e in all_e:
                es_sol = any(s.id == me.id for s in e.informe.solicitantes)
                es_dst = any(d.id == me.id for d in e.informe.destinatarios)
                
                if es_sol or es_dst:
                    col = 'green' if e.estado=='CUMPLIDO' else ('red' if datetime.now() > e.fecha_limite_teorica else 'orange')
                    if e.estado == 'EXTEMPORANEO': col = 'yellow'
                    rows.append({
                        'inf': e.informe.nombre, 'resp': e.usuario.nombre if e.usuario else '-',
                        'fecha': e.fecha_programada.strftime('%Y-%m-%d'), 'st': 'INCUMPLIDO' if col=='red' else e.estado,
                        'col': col, 'url': e.ruta_archivo, 'rol': 'Solicitante' if es_sol else 'Destinatario'
                    })
            tabla.rows = rows
            
            kpi_ok.text = str(len([x for x in rows if x['st']=='CUMPLIDO']))
            kpi_bad.text = str(len([x for x in rows if x['col'] in ['red','orange']]))

    def manejar_descarga(fila):
        ruta = fila.get('url')
        if not ruta or not os.path.exists(ruta):
            ui.notify('Archivo no disponible', type='warning')
            return
        with open(ruta, 'rb') as f:
            contenido = f.read()
        ui.download(contenido, os.path.basename(ruta))

    def enviar_sol():
        if not sol['tit']: return
        with get_db_session() as db:
            me = db.query(Stakeholder).filter(Stakeholder.usuario_id == uid).first()
            if me:
                db.add(SolicitudNuevoInforme(stakeholder_id=me.id, titulo=sol['tit'], descripcion=sol['desc']))
                db.commit(); ui.notify('Solicitud enviada', type='positive'); d_sol.close()

    ui.label('Portal de Control').classes('text-2xl font-bold mb-4')
    with ui.row().classes('w-full gap-6 mb-6'):
        with ui.card().classes('w-1/3 p-4 border-l-4 border-green-500 flex justify-between items-center'):
            with ui.column().classes('gap-0'): ui.label('Recibidos').classes('text-xs text-gray-400 uppercase font-bold'); kpi_ok = ui.label('0').classes('text-3xl font-bold text-green-700')
            ui.icon('check_circle').classes('text-4xl text-green-200')
        with ui.card().classes('w-1/3 p-4 border-l-4 border-red-500 flex justify-between items-center'):
            with ui.column().classes('gap-0'): ui.label('Pendientes').classes('text-xs text-gray-400 uppercase font-bold'); kpi_bad = ui.label('0').classes('text-3xl font-bold text-red-700')
            ui.icon('warning').classes('text-4xl text-red-200')
        with ui.column().classes('w-1/4'):
            ui.button('Solicitar Informe', icon='add_comment', on_click=lambda: d_sol.open()).classes('w-full h-full bg-slate-800 text-white')

    cols = [{'name':'fecha','label':'Fecha','field':'fecha','align':'left'}, {'name':'inf','label':'Informe','field':'inf','align':'left'}, {'name':'resp','label':'Responsable','field':'resp','align':'left'}, {'name':'rol','label':'Mi Rol','field':'rol','align':'center'}, {'name':'st','label':'Estado','field':'st','align':'center'}, {'name':'act','label':'Evidencia','field':'act','align':'center'}]
    tabla = ui.table(columns=cols, rows=[]).classes('w-full shadow-sm')
    tabla.add_slot('body-cell-st', '<q-td :props="props"><q-badge :color="props.row.col">{{ props.row.st }}</q-badge></q-td>')
    tabla.add_slot('body-cell-act', '''<q-td :props="props">
        <q-btn v-if="props.row.url" icon="download" flat round color="primary" size="sm"
            @click="$emit(\'descargar\', props.row)"><q-tooltip>Descargar</q-tooltip></q-btn>
        <span v-else class="text-gray-300">-</span>
    </q-td>''')
    tabla.on('descargar', lambda e: manejar_descarga(e.args))

    with ui.dialog() as d_sol, ui.card().classes('min-w-[500px]'):
        ui.label('Solicitud').classes('text-lg font-bold')
        ui.input('Asunto').bind_value(sol, 'tit').classes('w-full')
        ui.textarea('Detalle').bind_value(sol, 'desc').classes('w-full')
        with ui.row().classes('w-full justify-end mt-4'):
            ui.button('Cancelar', on_click=d_sol.close).props('flat')
            ui.button('Enviar', on_click=enviar_sol).classes('bg-blue-600 text-white')
            
    cargar()