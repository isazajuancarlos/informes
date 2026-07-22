from nicegui import ui, app
from app.core.database import get_db_session
from app.models.meta_tables import EntregaEsperada, Comentario, AuditLog
from app.services.signing_service import verificar_entrega
from app.core.tiempo import formatear_utc
from datetime import datetime
import io
import csv

def page_audit():
    user_id = app.storage.user.get('user_id')
    estado = {'fechas': []}
    _entregas_rows = []
    _seg_rows = []

    # --- Funciones de Entregas (existente) ---
    def abrir_chat(eid):
        def cargar():
            cont_msgs.clear()
            with get_db_session() as db:
                msgs = db.query(Comentario).filter_by(entrega_id=eid).order_by(Comentario.fecha).all()
                with cont_msgs:
                    if not msgs: ui.label('Sin comentarios').classes('text-gray-400 italic w-full text-center mt-4')
                    for m in msgs:
                        mio = m.autor_id == user_id
                        align = 'end' if mio else 'start'
                        bg = 'bg-blue-100' if mio else 'bg-gray-100'
                        with ui.row().classes(f'w-full justify-{align}'):
                            with ui.column().classes(f'{bg} p-2 rounded max-w-[80%] gap-0'):
                                ui.label(m.autor.nombre).classes('text-[10px] font-bold text-blue-800')
                                ui.label(m.texto).classes('text-sm')
                                ui.label(formatear_utc(m.fecha, '%d/%m %H:%M')).classes('text-[9px] text-gray-500 self-end')

        def enviar():
            if not txt_in.value: return
            with get_db_session() as db:
                db.add(Comentario(entrega_id=eid, autor_id=user_id, texto=txt_in.value)); db.commit()
            txt_in.value = ''; cargar()

        with ui.dialog() as d, ui.card().classes('w-[400px] h-[500px] column p-0'):
            with ui.row().classes('w-full bg-slate-900 text-white p-3 justify-between items-center'):
                ui.label(f'Chat Auditoría #{eid}').classes('font-bold')
                ui.button(icon='close', on_click=d.close).props('flat round dense text-color=white')
            cont_msgs = ui.column().classes('w-full flex-grow p-4 overflow-y-auto')
            with ui.row().classes('w-full bg-white border-t p-2 gap-2'):
                txt_in = ui.input(placeholder='Mensaje...').classes('flex-grow').on('keydown.enter', enviar)
                ui.button(icon='send', on_click=enviar).props('round dense')
            cargar()
        d.open()

    def buscar():
        if not estado['fechas']: return
        fechas = [datetime.strptime(f, '%Y-%m-%d').date() for f in estado['fechas']]
        with get_db_session() as db:
            all_t = db.query(EntregaEsperada).all()
            fil = [t for t in all_t if t.fecha_programada.date() in fechas]
            rows = []
            for t in fil:
                col = 'green' if t.estado=='CUMPLIDO' else ('red' if datetime.now() > t.fecha_limite_teorica else 'orange')
                if t.estado == 'EXTEMPORANEO': col = 'yellow'
                n_msgs = len(t.mensajes_chat)
                rows.append({
                    'id': t.id, 'inf': t.informe.nombre, 'resp': t.usuario.nombre,
                    'fecha': t.fecha_programada.strftime('%Y-%m-%d'), 'st': t.estado, 'col': col,
                    'chat_lbl': f'Chat ({n_msgs})' if n_msgs else 'Chat',
                    'arch': 'Sí' if t.ruta_archivo else 'No',
                    # `firmada` distingue TRES estados, no dos. «Sin firma» no
                    # es lo mismo que «firma inválida»: las entregas anteriores
                    # a esta función, y las registradas sin el servicio activo,
                    # no tienen firma y eso es normal. Pintarlas como
                    # sospechosas entrenaría a ignorar el aviso de verdad.
                    'firmada': bool(t.firma),
                })
            tabla_entregas.rows = rows
            _entregas_rows.clear()
            _entregas_rows.extend(rows)

    def verificar_firma(eid):
        """Comprueba que la firma corresponde a la entrega TAL COMO ESTÁ HOY.

        El valor de esto no es decir «sí, está firmada» —eso ya lo dice el icono
        de la tabla— sino detectar que alguien cambió el estado, la fecha o el
        archivo DESPUÉS de firmar. Por eso se recalcula la forma canónica en el
        momento de verificar y no se guarda un veredicto: un veredicto
        almacenado envejece y deja de significar nada.
        """
        with get_db_session() as db:
            t = db.query(EntregaEsperada).get(eid)
            if t is None:
                ui.notify('La entrega ya no existe', type='negative')
                return
            if not t.firma:
                ui.notify('Esta entrega no tiene firma (no es un fallo: la firma '
                          'es opcional y las entregas anteriores no la llevan)',
                          type='info', multi_line=True, classes='w-96')
                return
            ok, motivo = verificar_entrega(t, t.firma)

        with ui.dialog() as dlg, ui.card().classes('w-[520px]'):
            with ui.row().classes('items-center gap-2'):
                ui.icon('verified' if ok else 'gpp_bad',
                        color='green' if ok else 'red').classes('text-3xl')
                ui.label('Firma verificada' if ok else 'Firma NO verificada') \
                    .classes('text-lg font-bold')
            ui.label(motivo).classes('text-sm text-gray-600')
            ui.separator()
            if ok:
                ui.label(
                    'Los campos auditables (informe, responsable, fecha límite, '
                    'fecha de entrega, estado y el SHA-256 del archivo) son los '
                    'mismos que se firmaron al registrar la entrega.'
                ).classes('text-xs text-gray-500')
            else:
                # Se explica qué significa cada fallo, porque «inválida» a secas
                # invita a asumir manipulación cuando lo más común es una
                # configuración distinta de claves.
                ui.label(
                    'Puede deberse a que la entrega se modificó después de '
                    'firmarse, o a que el sistema está configurado con una clave '
                    'de verificación distinta de la que firmó. Antes de concluir '
                    'manipulación, comprobar QUIPU_VERIFYING_KEY.'
                ).classes('text-xs text-gray-500')
            ui.button('Cerrar', on_click=dlg.close).props('flat').classes('self-end')
        dlg.open()

    # --- Funciones de Seguridad (nuevo) ---
    filtro_seg = {'evento': None, 'email': '', 'resultado': None}

    def buscar_seguridad():
        with get_db_session() as db:
            query = db.query(AuditLog).order_by(AuditLog.timestamp.desc())
            if filtro_seg['evento']:
                query = query.filter(AuditLog.evento == filtro_seg['evento'])
            if filtro_seg['email']:
                query = query.filter(AuditLog.email.ilike(f"%{filtro_seg['email']}%"))
            if filtro_seg['resultado'] == 'exitoso':
                query = query.filter(AuditLog.exitoso == True)
            elif filtro_seg['resultado'] == 'fallido':
                query = query.filter(AuditLog.exitoso == False)
            logs = query.limit(200).all()
            rows = []
            for log in logs:
                color_map = {
                    'login_attempt': 'blue' if log.exitoso else 'red',
                    'logout': 'grey',
                    'session_timeout': 'orange',
                    'permission_denied': 'red',
                }
                rows.append({
                    'id': log.id,
                    # `AuditLog.timestamp` se guarda en UTC (`default=datetime.utcnow`)
                    # y aquí se pintaba con `strftime` directo, o sea COMO SI
                    # fuera local: en Colombia, cada evento aparecía 5 horas en
                    # el futuro. La conversión arregla también las filas
                    # antiguas, porque todas son UTC. La exportación CSV
                    # reutiliza esta misma cadena, así que queda corregida sola.
                    'fecha': formatear_utc(log.timestamp),
                    'evento': log.evento or '',
                    'email': log.email or '',
                    'resultado': 'OK' if log.exitoso else ('Fallido' if log.exitoso is False else '—'),
                    'ip': log.ip_address or '—',
                    'recurso': log.recurso or '—',
                    'col': color_map.get(log.evento, 'grey'),
                })
            tabla_seguridad.rows = rows
            _seg_rows.clear()
            _seg_rows.extend(rows)

    # --- Export helpers ---

    def exportar_entregas_csv():
        if not _entregas_rows:
            ui.notify('Sin datos para exportar', type='warning')
            return
        buf = io.StringIO()
        writer = csv.writer(buf)
        writer.writerow(['ID', 'Fecha', 'Informe', 'Responsable', 'Estado', 'Archivo'])
        for r in _entregas_rows:
            writer.writerow([r['id'], r['fecha'], r['inf'], r['resp'], r['st'], r.get('arch', '')])
        ui.download(buf.getvalue().encode('utf-8'), 'entregas_auditoria.csv')

    def exportar_seguridad_csv():
        if not _seg_rows:
            ui.notify('Sin datos para exportar', type='warning')
            return
        buf = io.StringIO()
        writer = csv.writer(buf)
        writer.writerow(['ID', 'Fecha', 'Evento', 'Email', 'Resultado', 'IP', 'Recurso'])
        for r in _seg_rows:
            writer.writerow([r['id'], r['fecha'], r['evento'], r['email'], r['resultado'], r['ip'], r['recurso']])
        ui.download(buf.getvalue().encode('utf-8'), 'seguridad_auditoria.csv')

    # --- UI ---
    ui.label('Auditoría').classes('text-2xl font-bold mb-4')

    with ui.tabs().classes('w-full') as tabs:
        tab_entregas = ui.tab('Entregas')
        tab_seguridad = ui.tab('Seguridad')

    with ui.tab_panels(tabs, value='Entregas').classes('w-full'):
        # --- Tab Entregas (existente) ---
        with ui.tab_panel(tab_entregas):
            with ui.row().classes('w-full justify-end mb-2'):
                ui.button('Exportar CSV', icon='download', on_click=exportar_entregas_csv).props('flat size=sm color=primary')

            with ui.row().classes('w-full gap-6 items-start'):
                with ui.card().classes('w-1/3'):
                    ui.label('Fechas').classes('font-bold text-gray-500')
                    ui.date(on_change=buscar).bind_value(estado, 'fechas').props('multiple').classes('shadow-none')

                with ui.column().classes('w-2/3'):
                    cols = [{'name':'fecha','label':'Fecha','field':'fecha','align':'left'}, {'name':'inf','label':'Informe','field':'inf','align':'left'}, {'name':'resp','label':'Resp.','field':'resp','align':'left'}, {'name':'st','label':'Estado','field':'st','align':'center'}, {'name':'act','label':'Acciones','field':'act','align':'center'}]
                    tabla_entregas = ui.table(columns=cols, rows=[]).classes('w-full shadow-sm')
                    tabla_entregas.add_slot('body-cell-st', '<q-td :props="props"><q-badge :color="props.row.col">{{ props.row.st }}</q-badge></q-td>')
                    tabla_entregas.add_slot('body-cell-act', '<q-td :props="props"><q-btn :label="props.row.chat_lbl" icon="chat" flat dense size="sm" color="purple" @click="$parent.$emit('+"'chat'"+', props.row.id)" /><q-icon v-if="props.row.arch==\'Sí\'" name="attachment" color="grey" class="ml-2"/><q-btn v-if="props.row.firmada" icon="verified_user" flat dense size="sm" color="teal" @click="$parent.$emit('+"'verificar'"+', props.row.id)"><q-tooltip>Verificar firma</q-tooltip></q-btn></q-td>')
                    tabla_entregas.on('chat', lambda e: abrir_chat(e.args))
                    tabla_entregas.on('verificar', lambda e: verificar_firma(e.args))

        # --- Tab Seguridad (nuevo) ---
        with ui.tab_panel(tab_seguridad):
            with ui.row().classes('w-full gap-4 items-end mb-4 justify-between'):
                with ui.row().classes('gap-4 items-end flex-wrap'):
                    ui.select(
                        {'login_attempt': 'Login', 'logout': 'Logout', 'session_timeout': 'Timeout', 'permission_denied': 'Acceso Denegado'},
                        label='Evento', clearable=True
                    ).bind_value(filtro_seg, 'evento').classes('w-48')
                    ui.input('Email').bind_value(filtro_seg, 'email').classes('w-48')
                    ui.select(
                        {'exitoso': 'Exitoso', 'fallido': 'Fallido'},
                        label='Resultado', clearable=True
                    ).bind_value(filtro_seg, 'resultado').classes('w-36')
                    ui.button('Buscar', icon='search', on_click=buscar_seguridad).classes('bg-slate-800 text-white')
                ui.button('Exportar CSV', icon='download', on_click=exportar_seguridad_csv).props('flat size=sm color=primary')

            cols_seg = [
                {'name': 'fecha', 'label': 'Fecha/Hora', 'field': 'fecha', 'align': 'left'},
                {'name': 'evento', 'label': 'Evento', 'field': 'evento', 'align': 'left'},
                {'name': 'email', 'label': 'Email', 'field': 'email', 'align': 'left'},
                {'name': 'resultado', 'label': 'Resultado', 'field': 'resultado', 'align': 'center'},
                {'name': 'ip', 'label': 'IP', 'field': 'ip', 'align': 'left'},
                {'name': 'recurso', 'label': 'Recurso', 'field': 'recurso', 'align': 'left'},
            ]
            tabla_seguridad = ui.table(columns=cols_seg, rows=[]).classes('w-full shadow-sm border rounded')
            tabla_seguridad.add_slot('body-cell-evento', '<q-td :props="props"><q-badge :color="props.row.col">{{ props.row.evento }}</q-badge></q-td>')
            buscar_seguridad()
