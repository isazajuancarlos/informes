from nicegui import ui, app, events
from app.core.database import get_db_session
from app.models.meta_tables import EntregaEsperada, Informe, Delegacion, Usuario
from app.services.scheduler import GeneradorTareas
from app.services.file_manager import GestorArchivos
from app.services.signing_service import firmar_entrega
from app.core.tiempo import describir_vencimiento
import os
import time
import inspect
from datetime import datetime

def page_my_tasks():
    user_id = app.storage.user.get('user_id')
    gestor_archivos = GestorArchivos()
    
    # Estado local
    estado_subida = {'tarea_id': None, 'permitidos': []}
    nueva_novedad = {'informe_id': None}

    # --- 1. LÓGICA DE UPLOAD BLINDADO (Versión Definitiva) ---
    async def manejar_upload(e):
        tarea_id = estado_subida['tarea_id']
        if not tarea_id: return
        
        # Obtener archivo real (soporte v3.3.1 / v1.4+)
        archivo_interno = getattr(e, 'file', None) or getattr(e, 'content', None)
        if not archivo_interno:
            ui.notify('Error leyendo el archivo', type='negative'); return

        # Detectar nombre
        nombre_archivo = getattr(archivo_interno, 'name', '') or getattr(archivo_interno, 'filename', '') or f"entrega_{tarea_id}_{int(time.time())}.dat"
        
        # Validar extensión
        ext_real = nombre_archivo.split('.')[-1].lower().strip()
        permitidos = [p.lower().strip() for p in estado_subida['permitidos']]
        
        if permitidos and ext_real not in permitidos:
            ui.notify(f'❌ Formato incorrecto. Se requiere: {permitidos}', type='negative', close_button=True)
            return

        # Ruta física
        ruta_carpeta = f"storage/entregas/{datetime.now().year}/{tarea_id}"
        if not os.path.exists(ruta_carpeta): os.makedirs(ruta_carpeta)
        ruta_completa = os.path.join(ruta_carpeta, nombre_archivo)

        try:
            # Lectura del contenido (Async/Sync Check)
            contenido = b""
            if hasattr(archivo_interno, 'read'):
                posible_corutina = archivo_interno.read()
                if inspect.iscoroutine(posible_corutina): contenido = await posible_corutina
                else: contenido = posible_corutina
            else:
                contenido = archivo_interno

            # Escribir en disco
            with open(ruta_completa, 'wb') as f:
                if isinstance(contenido, str): f.write(contenido.encode('utf-8'))
                else: f.write(contenido)

            # Actualizar BD y cerrar tarea
            with get_db_session() as db:
                tarea = db.get(EntregaEsperada, tarea_id)
                hora_actual = datetime.now()
                
                # Determinar estado final (Cumplido vs Extemporáneo)
                nuevo_estado = 'CUMPLIDO'
                if hora_actual > tarea.fecha_limite_teorica:
                    nuevo_estado = 'EXTEMPORANEO'
                
                tarea.ruta_archivo = ruta_completa
                tarea.fecha_entrega_real = hora_actual
                tarea.estado = nuevo_estado

                # Firma post-cuántica de los campos auditables, ANTES del commit
                # y no después: así no existe ningún instante en el que la
                # entrega esté guardada sin su firma. Un segundo commit podría
                # no llegar a ocurrir —excepción, corte, reinicio— y dejaría
                # entregas firmables sin firmar sin que nadie lo note.
                #
                # `firmar_entrega` devuelve None si el servicio no está
                # disponible (sin `quipu-crypto` o sin claves). Eso NO es un
                # error: la firma es opcional y una entrega debe poder
                # registrarse siempre. Lo que no puede pasar es que un fallo de
                # la firma impida entregar.
                try:
                    tarea.firma = firmar_entrega(tarea)
                except Exception as ex_firma:
                    # Se traga la excepción a propósito, pero NO en silencio:
                    # queda en el log del servidor. Bloquear una entrega porque
                    # falló un extra sería el peor intercambio posible.
                    print(f"[firma] no se pudo firmar la entrega {tarea_id}: {ex_firma}")
                    tarea.firma = None

                db.commit()

                tipo_notif = 'warning' if nuevo_estado == 'EXTEMPORANEO' else 'positive'
                sufijo = ' · firmada' if tarea.firma else ''
                ui.notify(f'Entrega registrada: {nuevo_estado}{sufijo}', type=tipo_notif)

            dialogo.close()
            cargar_tabla() # Refrescar vista
            
        except Exception as ex:
            print(f"Error upload: {ex}")
            ui.notify(f'Error guardando: {ex}', type='negative')

    # --- 2. LÓGICA DE DATOS Y SEMÁFORO ---
    def cargar_tabla():
        with get_db_session() as db:
            # Traer tareas del usuario ordenadas por urgencia (Fecha)
            tareas = db.query(EntregaEsperada).filter(
                EntregaEsperada.usuario_id == user_id
            ).order_by(EntregaEsperada.fecha_programada.desc()).all()
            
            filas = []
            now = datetime.now()

            for t in tareas:
                # Lógica Visual del Semáforo
                color = 'grey'
                texto_estado = t.estado  # Valor base de la BD

                if t.estado == 'PENDIENTE':
                    if now > t.fecha_limite_teorica:
                        color = 'red'
                        texto_estado = 'INCUMPLIDO'
                    else:
                        color = 'orange'
                        texto_estado = 'PENDIENTE'
                elif t.estado == 'CUMPLIDO':
                    color = 'green'
                elif t.estado == 'EXTEMPORANEO':
                    color = 'yellow text-black'

                # Check if this task was delegated to this user
                delegante_nombre = None
                delegacion = db.query(Delegacion).filter(
                    Delegacion.delegado_id == user_id,
                    Delegacion.activo == True,
                    Delegacion.fecha_inicio <= t.fecha_programada,
                    Delegacion.fecha_fin >= t.fecha_programada
                ).first()
                if delegacion:
                    delegante = db.get(Usuario, delegacion.delegante_id)
                    if delegante:
                        delegante_nombre = delegante.nombre

                # Las DOS fechas: el día que vence y hasta cuándo hay para
                # entregarlo. Mostrar solo el límite dejaba al responsable sin
                # saber si entregar ese día era cumplir o llegar tarde.
                fecha_venc = describir_vencimiento(t)
                dat = t.informe.datos_generales or {}

                filas.append({
                    'id': t.id,
                    'informe': t.informe.nombre if t.informe else '---',
                    'vencimiento': fecha_venc,
                    'estado_visual': texto_estado,
                    'color': color,
                    'archivo': 'Sí' if t.ruta_archivo else 'No',
                    'puede_subir': t.estado in ('PENDIENTE', 'CUMPLIDO', 'EXTEMPORANEO'),
                    'exts': dat.get('extensiones', []),
                    'delegado_por': delegante_nombre or '',
                })
            tabla.rows = filas

    def generar_tareas_hoy():
        """Simula el proceso nocturno"""
        n = GeneradorTareas.generar_pendientes_del_dia()
        ui.notify(f'Se generaron {n} tareas nuevas.', type='info' if n==0 else 'positive')
        cargar_tabla()

    def abrir_entrega(fila):
        estado_subida['tarea_id'] = fila['id']
        estado_subida['permitidos'] = fila['exts']
        lbl_titulo_modal.text = f"Entregar: {fila['informe']}"
        
        # Feedback visual de formatos
        txt_fmts = ", ".join(fila['exts']) if fila['exts'] else "Cualquiera"
        lbl_formatos.text = f"Formatos aceptados: {txt_fmts}"
        
        uploader.reset()
        dialogo.open()

    # --- LÓGICA REPORTE INMEDIATO ---
    def crear_reporte_inmediato():
        inf_id = nueva_novedad['informe_id']
        if not inf_id: return
        
        with get_db_session() as db:
            # Crear tarea que vence hoy a las 23:59
            nueva = EntregaEsperada(
                informe_id=inf_id, usuario_id=user_id,
                fecha_programada=datetime.now(),
                fecha_limite_teorica=datetime.now().replace(hour=23, minute=59),
                estado='PENDIENTE'
            )
            db.add(nueva); db.commit()
            
            # Preparar datos para abrir el modal inmediatamente
            id_tarea = nueva.id
            inf = db.get(Informe, inf_id)
            exts = (inf.datos_generales or {}).get('extensiones', [])

        ui.notify('Reporte creado. Suba la evidencia.', type='positive')
        dialogo_novedad.close()
        cargar_tabla()
        
        # Auto-abrir modal
        estado_subida['tarea_id'] = id_tarea
        estado_subida['permitidos'] = exts
        lbl_titulo_modal.text = f"Entregar: {inf.nombre}"
        lbl_formatos.text = f"Formatos: {exts}"
        uploader.reset()
        dialogo.open()

    # --- 3. INTERFAZ GRÁFICA (UI) ---
    ui.label('Mis Obligaciones Pendientes').classes('text-2xl font-bold text-slate-800 mb-4')

    with ui.row().classes('w-full justify-between mb-4'):
        ui.button('Actualizar Lista', on_click=generar_tareas_hoy).props('flat icon=sync').classes('text-slate-600')
        ui.button('Reportar Novedad', on_click=lambda: dialogo_novedad.open()).classes('bg-orange-600 text-white').props('icon=add_alert')

    # Definición de Columnas
    cols = [
        {'name': 'informe', 'label': 'Informe', 'field': 'informe', 'align': 'left', 'sortable': True},
        {'name': 'vencimiento', 'label': 'Vencimiento', 'field': 'vencimiento', 'align': 'left', 'sortable': True},
        {'name': 'st', 'label': 'Estado', 'field': 'st', 'align': 'center'},
        {'name': 'delegacion', 'label': 'Delegación', 'field': 'delegacion', 'align': 'center'},
        {'name': 'act', 'label': 'Acción', 'field': 'act', 'align': 'center'},
    ]

    tabla = ui.table(columns=cols, rows=[], pagination=10).classes('w-full shadow-sm border rounded')

    # Slot de Estado (Semáforo)
    tabla.add_slot('body-cell-st', '''
        <q-td :props="props">
            <q-badge :color="props.row.color" class="text-white font-bold shadow-sm px-2 py-1">
                {{ props.row.estado_visual }}
            </q-badge>
        </q-td>
    ''')

    # Slot de delegación
    tabla.add_slot('body-cell-delegacion', '''
        <q-td :props="props">
            <q-badge v-if="props.row.delegado_por" color="purple" class="text-white text-xs">
                Delegado por {{ props.row.delegado_por }}
            </q-badge>
            <span v-else class="text-gray-300 text-xs">—</span>
        </q-td>
    ''')

    # Slot de Acción — permite re-subida en cualquier estado
    tabla.add_slot('body-cell-act', '''
        <q-td :props="props">
            <q-btn v-if="props.row.puede_subir && props.row.estado_visual === \'PENDIENTE\'"
                   label="Subir" icon="cloud_upload" size="sm" color="primary"
                   @click="$parent.$emit(\'up\', props.row)" />
            <q-btn v-else-if="props.row.puede_subir"
                   label="Re-enviar" icon="refresh" size="sm" color="orange" flat
                   @click="$parent.$emit(\'up\', props.row)" />
            <div v-else class="text-gray-300 text-xs">—</div>
        </q-td>
    ''')

    tabla.on('up', lambda e: abrir_entrega(e.args))

    # --- MODALES ---
    with ui.dialog() as dialogo, ui.card().classes('min-w-[400px]'):
        lbl_titulo_modal = ui.label('Entregar').classes('text-lg font-bold')
        lbl_formatos = ui.label('').classes('text-xs text-gray-500 mb-2')
        
        uploader = ui.upload(
            label='Arrastre su archivo aquí', 
            auto_upload=True, 
            on_upload=manejar_upload, 
            max_files=1
        ).classes('w-full')
        
        ui.button('Cancelar', on_click=dialogo.close).props('flat color=grey').classes('w-full mt-2')

    with ui.dialog() as dialogo_novedad, ui.card().classes('min-w-[400px]'):
        ui.label('Crear Reporte A Demanda').classes('text-xl font-bold text-orange-800')
        
        with get_db_session() as db:
            # Cargar solo informes inmediatos asignados al usuario
            mis_informes = db.query(Informe).filter(Informe.activo==True).all()
            opciones = {}
            for inf in mis_informes:
                es_mio = any(u.id == user_id for u in inf.responsables)
                # Verificar modo Inmediato
                es_inmediato = (inf.recurrencia_json or {}).get('modo') == 'INMEDIATO'
                if es_mio and es_inmediato:
                    opciones[inf.id] = inf.nombre
        
        if not opciones:
            ui.label('No tienes informes inmediatos asignados.').classes('text-red italic')
        else:
            ui.select(opciones, label='Tipo de Evento').bind_value(nueva_novedad, 'informe_id').classes('w-full')
            ui.button('Generar', on_click=crear_reporte_inmediato).classes('w-full mt-4 bg-orange-600 text-white')

    cargar_tabla()