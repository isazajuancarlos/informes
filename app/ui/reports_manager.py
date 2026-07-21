from nicegui import ui
from app.core.database import get_db_session
from app.models.meta_tables import (
    Informe, Usuario, MetaVariable, ConfiguracionInformeVariable, Stakeholder,
    ConjuntoDatos, UnidadOrganizacional, Participacion,
    ROL_ORIGEN, ROL_DESTINO, ROL_USUARIO,
)
from app.ui.ui_helpers import estado_vacio, ayuda
from datetime import datetime
from app.services.preview_calendario import proximos as _proximos_vencimientos

#: Las tres únicas respuestas posibles a «¿y si cae en día no hábil?». Ser un
#: selector y no dos casillas es lo que impide el estado ambiguo por
#: construcción, en vez de tener que validarlo después.
AJUSTE_HABIL = {
    'ninguno': 'Dejar la fecha como está',
    'siguiente': 'Mover al siguiente día hábil',
    'anterior': 'Mover al día hábil anterior (seguro para plazos legales)',
}
AYUDA_AJUSTE = ("Si el vencimiento cae en fin de semana o festivo. «Anterior» es "
                "la opción segura para plazos legales: adelantar nunca llega tarde, "
                "aplazar sí.")


def _ajuste_desde_banderas(siguiente, anterior):
    """Banderas guardadas -> opción del selector.

    Las configuraciones creadas antes del selector pueden traer las DOS
    banderas en verdadero. Se resuelve igual que el motor (`siguiente` gana por
    su `elif`) para que abrir un informe viejo muestre lo que el sistema está
    haciendo de verdad, y no lo que la interfaz preferiría.
    """
    if siguiente:
        return 'siguiente'
    if anterior:
        return 'anterior'
    return 'ninguno'


def _construir_regla(datos):
    """La configuración de periodicidad, tal como se guarda.

    Extraída de `guardar()` para que la VISTA PREVIA use exactamente la misma
    construcción. Una vista previa que arme la regla de otra forma mostraría
    fechas que el sistema luego no genera — y nadie sabría cuál de las dos está
    mal, que es peor que no tener vista previa.
    """
    modo = datos['modo_tiempo']
    f_inicio = datos['fecha_inicio']
    if modo == 'CICLICO':
        total = datos['tiempo_ciclico']['total_dias']
        return {"modo": "CICLICO", "frecuencia": 'ONCE' if total == 0 else 'DAILY',
                "intervalo": total, "fecha_inicio": f_inicio,
                **_banderas_desde_ajuste(datos['tiempo_ciclico']['ajuste'],
                                         "mover_habil", "mover_habil_anterior")}
    if modo == 'ESPECIFICO':
        return {"modo": "ESPECIFICO",
                "meses": datos['tiempo_especifico']['meses_seleccionados'],
                "dia_corte": int(datos['tiempo_especifico']['dia_corte'] or 1),
                "es_ultimo_dia": datos['tiempo_especifico']['es_ultimo_dia'],
                **_banderas_desde_ajuste(datos['tiempo_especifico']['ajuste'],
                                         "ajuste_habil", "ajuste_habil_anterior"),
                "fecha_inicio": f_inicio}
    if modo == 'SEMANAL':
        return {"modo": "SEMANAL",
                "dias_semana": datos['tiempo_semanal']['dias_seleccionados'],
                "mover_habil": datos['tiempo_semanal']['mover_habil'],
                "fecha_inicio": f_inicio}
    if modo == 'HOURLY':
        return {"modo": "HOURLY",
                "hora_inicio": int(datos['tiempo_horas']['hora_inicio']),
                "hora_fin": int(datos['tiempo_horas']['hora_fin']),
                "intervalo_horas": int(datos['tiempo_horas']['intervalo']),
                "fecha_inicio": f_inicio}
    return {"modo": "INMEDIATO", "descripcion": "A demanda", "fecha_inicio": f_inicio}


def _banderas_desde_ajuste(ajuste, clave_siguiente, clave_anterior):
    """Opción del selector -> las banderas que espera el motor.

    Se emiten SIEMPRE las dos claves, con su valor explícito. Omitir la que va
    en falso dejaría el valor anterior si alguien fusiona diccionarios, y ese es
    el tipo de resto que hace que una obligación cambie de comportamiento sin
    que nadie la haya tocado.
    """
    return {clave_siguiente: ajuste == 'siguiente',
            clave_anterior: ajuste == 'anterior'}

def page_reports_manager():
    # --- 1. ESTADO GLOBAL ---
    estado = {
        'modo': 'LISTA', 
        'filtro': '',
        'id_edicion': None,
        'id_desactivar': None
    }

    nuevo_informe = {
        'nombre': '', 'descripcion': '',
        'responsables': [], 'extensiones': [],
        'solicitantes': [], 'destinatarios': [],
        # NUEVOS CAMPOS v3.0
        'subsistema': '',
        'conjunto_datos_id': None,
        'unidad_id': None,
        'tiene_acuerdo_confidencialidad': False,
        'cargo_autorizador': '',
        'url_plantilla': '',
        'ruta_plantilla': '',
        'justificacion_normatividad': '',
        'aplicativo_origen': '',
        'modo_tiempo': 'CICLICO',
        'fecha_inicio': datetime.now().strftime('%Y-%m-%d'),
        'tiempo_ciclico': {'anos': 0, 'meses': 0, 'dias': 0, 'total_dias': 0, 'ajuste': 'ninguno'},
        'tiempo_especifico': {'meses_seleccionados': [], 'dia_corte': 1, 'es_ultimo_dia': False, 'ajuste': 'ninguno'},
        'tiempo_semanal': {'dias_seleccionados': [], 'mover_habil': False},
        'tiempo_horas': {'hora_inicio': 8, 'hora_fin': 18, 'intervalo': 1},
        'variables_config': {}
    }
    
    motivo_input = {'texto': ''}
    
    contenedor_principal = ui.column().classes('w-full')
    contenedor_detalle = ui.column().classes('w-full gap-2')

    # --- 2. FUNCIONES BASE ---
    def render_ui():
        if estado['modo'] == 'LISTA': render_lista()
        else: render_formulario()

    def ir_a_lista(): estado['modo'] = 'LISTA'; render_ui()
    def ir_a_formulario_nuevo(): limpiar_formulario(); estado['modo'] = 'FORMULARIO'; render_ui()

    def calcular_dias(e=None):
        if nuevo_informe['modo_tiempo'] != 'CICLICO': return
        a = int(nuevo_informe['tiempo_ciclico']['anos'] or 0)
        m = int(nuevo_informe['tiempo_ciclico']['meses'] or 0)
        d = int(nuevo_informe['tiempo_ciclico']['dias'] or 0)
        nuevo_informe['tiempo_ciclico']['total_dias'] = (a * 360) + (m * 30) + d
        # La vista previa se refresca desde el cierre que la define; si aún no
        # existe (durante el primer render) no hace nada.
        try:
            refrescar_vista_previa()
        except NameError:
            pass

    def limpiar_formulario():
        nuevo_informe.update({
            'nombre': '', 'descripcion': '', 'responsables': [], 'extensiones': [],
            'solicitantes': [], 'destinatarios': [],
            'subsistema': '',
            'conjunto_datos_id': None,
            'unidad_id': None,
            'tiene_acuerdo_confidencialidad': False,
            'cargo_autorizador': '',
            'url_plantilla': '',
            'ruta_plantilla': '',
            'justificacion_normatividad': '',
            'aplicativo_origen': '',
            'modo_tiempo': 'CICLICO',
            'fecha_inicio': datetime.now().strftime('%Y-%m-%d'),
            'tiempo_ciclico': {'anos': 0, 'meses': 0, 'dias': 0, 'total_dias': 0, 'ajuste': 'ninguno'},
            'tiempo_especifico': {'meses_seleccionados': [], 'dia_corte': 1, 'es_ultimo_dia': False, 'ajuste': 'ninguno'},
            'tiempo_semanal': {'dias_seleccionados': [], 'mover_habil': False},
            'tiempo_horas': {'hora_inicio': 8, 'hora_fin': 18, 'intervalo': 1},
            'variables_config': {}
        })
        estado['id_edicion'] = None

    def ir_a_editar(id_inf):
        limpiar_formulario()
        estado['id_edicion'] = id_inf
        estado['modo'] = 'FORMULARIO'
        with get_db_session() as db:
            inf = db.query(Informe).get(id_inf)
            nuevo_informe['nombre'] = inf.nombre
            nuevo_informe['descripcion'] = inf.descripcion
            nuevo_informe['responsables'] = [str(u.id) for u in inf.responsables]
            # Cargar Stakeholders
            nuevo_informe['solicitantes'] = [str(s.id) for s in inf.solicitantes]
            nuevo_informe['destinatarios'] = [str(s.id) for s in inf.destinatarios]

            # Cargar nuevos campos v3.0 + v4.0 (tipo_flujo es derivado)
            nuevo_informe['subsistema'] = inf.subsistema or ''
            nuevo_informe['conjunto_datos_id'] = str(inf.conjunto_datos_id) if inf.conjunto_datos_id else None
            nuevo_informe['unidad_id'] = inf.unidad_id
            nuevo_informe['tiene_acuerdo_confidencialidad'] = inf.tiene_acuerdo_confidencialidad or False
            nuevo_informe['cargo_autorizador'] = inf.cargo_autorizador or ''
            nuevo_informe['url_plantilla'] = inf.url_plantilla or ''
            nuevo_informe['ruta_plantilla'] = inf.ruta_plantilla or ''
            nuevo_informe['justificacion_normatividad'] = inf.justificacion_normatividad or ''
            nuevo_informe['aplicativo_origen'] = inf.aplicativo_origen or ''

            datos = inf.datos_generales or {}
            nuevo_informe['extensiones'] = datos.get('extensiones', [])
            for cfg in inf.configuracion_variables: nuevo_informe['variables_config'][str(cfg.variable_id)] = cfg.valor_parametro
            
            rec = inf.recurrencia_json or {}
            modo = rec.get('modo', 'CICLICO')
            nuevo_informe['modo_tiempo'] = modo
            nuevo_informe['fecha_inicio'] = rec.get('fecha_inicio', datetime.now().strftime('%Y-%m-%d'))
            
            if modo == 'CICLICO':
                total = int(rec.get('intervalo', 0))
                nuevo_informe['tiempo_ciclico'].update({'total_dias': total, 'anos': total // 360, 'meses': (total % 360) // 30, 'dias': (total % 360) % 30, 'ajuste': _ajuste_desde_banderas(rec.get('mover_habil'), rec.get('mover_habil_anterior'))})
            elif modo == 'ESPECIFICO':
                nuevo_informe['tiempo_especifico'].update({'meses_seleccionados': rec.get('meses', []), 'dia_corte': rec.get('dia_corte', 1), 'es_ultimo_dia': rec.get('es_ultimo_dia', False), 'ajuste': _ajuste_desde_banderas(rec.get('ajuste_habil'), rec.get('ajuste_habil_anterior'))})
            elif modo == 'SEMANAL':
                nuevo_informe['tiempo_semanal'].update({'dias_seleccionados': rec.get('dias_semana', []), 'mover_habil': rec.get('mover_habil', False)})
            elif modo == 'HOURLY':
                nuevo_informe['tiempo_horas'].update({'hora_inicio': rec.get('hora_inicio', 8), 'hora_fin': rec.get('hora_fin', 18), 'intervalo': rec.get('intervalo_horas', 1)})
        render_ui()

    def guardar():
        if not nuevo_informe['nombre']: ui.notify('Nombre obligatorio', type='warning'); return
        if not nuevo_informe['extensiones']: ui.notify('Seleccione extensiones', type='warning'); return

        modo = nuevo_informe['modo_tiempo']
        f_inicio = nuevo_informe['fecha_inicio']

        rec_final = _construir_regla(nuevo_informe)

        datos_gen = {"extensiones": nuevo_informe['extensiones']}

        with get_db_session() as db:
            if estado['id_edicion']:
                inf = db.query(Informe).get(estado['id_edicion'])
                inf.nombre = nuevo_informe['nombre']; inf.descripcion = nuevo_informe['descripcion']
                inf.recurrencia_json = rec_final; inf.datos_generales = datos_gen
                # Actualizar nuevos campos v3.0 + v4.0
                # tipo_flujo no se asigna: lo determinan las partes del flujo.
                inf.subsistema = nuevo_informe['subsistema'] if nuevo_informe['subsistema'] else None
                inf.conjunto_datos_id = int(nuevo_informe['conjunto_datos_id']) if nuevo_informe['conjunto_datos_id'] else None
                inf.tiene_acuerdo_confidencialidad = nuevo_informe['tiene_acuerdo_confidencialidad']
                inf.cargo_autorizador = nuevo_informe['cargo_autorizador'] if nuevo_informe['cargo_autorizador'] else None
                inf.url_plantilla = nuevo_informe['url_plantilla'] if nuevo_informe['url_plantilla'] else None
                inf.ruta_plantilla = nuevo_informe['ruta_plantilla'] if nuevo_informe['ruta_plantilla'] else None
                inf.justificacion_normatividad = nuevo_informe['justificacion_normatividad'] if nuevo_informe['justificacion_normatividad'] else None
                inf.aplicativo_origen = nuevo_informe['aplicativo_origen'] if nuevo_informe['aplicativo_origen'] else None
                inf.unidad_id = nuevo_informe.get('unidad_id')
                inf.tipo_novedad = "Actualización"; inf.fecha_actualizacion = datetime.now()
                # Las partes se reescriben enteras: una sola tabla que limpiar.
                for p in list(inf.participaciones): db.delete(p)
                inf.participaciones.clear()
                for cfg in inf.configuracion_variables: db.delete(cfg)
                msg = "Actualizado"
            else:
                inf = Informe(
                    nombre=nuevo_informe['nombre'],
                    descripcion=nuevo_informe['descripcion'],
                    creador_id=1,
                    recurrencia_json=rec_final,
                    datos_generales=datos_gen,
                    # Nuevos campos v3.0 + v4.0 (tipo_flujo es derivado, no se pasa)
                    subsistema=nuevo_informe['subsistema'] if nuevo_informe['subsistema'] else None,
                    conjunto_datos_id=int(nuevo_informe['conjunto_datos_id']) if nuevo_informe['conjunto_datos_id'] else None,
                    tiene_acuerdo_confidencialidad=nuevo_informe['tiene_acuerdo_confidencialidad'],
                    cargo_autorizador=nuevo_informe['cargo_autorizador'] if nuevo_informe['cargo_autorizador'] else None,
                    url_plantilla=nuevo_informe['url_plantilla'] if nuevo_informe['url_plantilla'] else None,
                    ruta_plantilla=nuevo_informe['ruta_plantilla'] if nuevo_informe['ruta_plantilla'] else None,
                    justificacion_normatividad=nuevo_informe['justificacion_normatividad'] if nuevo_informe['justificacion_normatividad'] else None,
                    aplicativo_origen=nuevo_informe['aplicativo_origen'] if nuevo_informe['aplicativo_origen'] else None,
                    unidad_id=nuevo_informe.get('unidad_id'),
                    activo=True,
                    tipo_novedad="Creación",
                    fecha_creacion=datetime.now()
                )
                db.add(inf); msg = "Creado"
            
            db.flush()
            # Las tres listas de la pantalla son el MISMO hecho con distinto rol:
            # quién produce (interno), quién consume y quién recibe (externos).
            for orden, uid in enumerate(nuevo_informe['responsables']):
                if db.query(Usuario).get(int(uid)):
                    db.add(Participacion(informe_id=inf.id, usuario_id=int(uid),
                                         rol=ROL_ORIGEN, orden=orden))

            for orden, sid in enumerate(nuevo_informe['solicitantes']):
                if db.query(Stakeholder).get(int(sid)):
                    db.add(Participacion(informe_id=inf.id, stakeholder_id=int(sid),
                                         rol=ROL_USUARIO, orden=orden))

            for orden, did in enumerate(nuevo_informe['destinatarios']):
                if db.query(Stakeholder).get(int(did)):
                    db.add(Participacion(informe_id=inf.id, stakeholder_id=int(did),
                                         rol=ROL_DESTINO, orden=orden))

            for var_id, val in nuevo_informe['variables_config'].items():
                if val: db.add(ConfiguracionInformeVariable(informe_id=inf.id, variable_id=int(var_id), valor_parametro=str(val)))
            db.commit()
        
        ui.notify(f'Informe {msg}', type='positive'); ir_a_lista()

    # --- MODALES ---
    with ui.dialog() as dialogo_detalle, ui.card().classes('w-[700px] max-w-full p-0'):
        with ui.row().classes('w-full bg-slate-900 text-white p-3 items-center justify-between'):
            ui.label('Ficha Técnica del Informe').classes('text-lg font-bold')
            ui.button(icon='close', on_click=dialogo_detalle.close).props('flat round dense text-color=white')
        contenedor_info_detalle = ui.column().classes('w-full p-4 gap-2 scroll-y max-h-[80vh]')
        with ui.row().classes('w-full justify-end p-3 bg-gray-50 border-t'):
            ui.button('Cerrar', on_click=dialogo_detalle.close).props('flat')

    def ver_detalle(id_inf):
        contenedor_info_detalle.clear()
        with get_db_session() as db:
            inf = db.query(Informe).get(id_inf)
            if not inf: return
            rec = inf.recurrencia_json or {}
            
            modo = rec.get('modo', 'CICLICO')
            txt_frec = "Desconocido"
            if modo == 'CICLICO': txt_frec = f"Cíclico: Cada {rec.get('intervalo')} días"
            elif modo == 'ESPECIFICO': txt_frec = f"Meses: {rec.get('meses')}"
            elif modo == 'SEMANAL': txt_frec = f"Días: {rec.get('dias_semana')}"
            elif modo == 'HOURLY': txt_frec = "Por Horas"

            with contenedor_info_detalle:
                ui.label(inf.nombre).classes('text-xl font-bold text-blue-900')
                ui.label(inf.descripcion).classes('text-gray-600 italic mb-4')
                
                with ui.grid(columns=2).classes('w-full gap-4'):
                    with ui.card().classes('p-3 bg-blue-50 shadow-none border border-blue-100'):
                        ui.label('Frecuencia').classes('font-bold text-blue-800 text-xs'); ui.label(txt_frec).classes('text-sm')
                    with ui.card().classes('p-3 bg-orange-50 shadow-none border border-orange-100'):
                        ui.label('Responsables').classes('font-bold text-orange-800 text-xs')
                        for u in inf.responsables: ui.chip(u.nombre).props('dense outline')

                # --- NUEVA SECCIÓN STAKEHOLDERS ---
                ui.separator().classes('my-4')
                ui.label('Flujo de Información').classes('font-bold text-sm')
                with ui.row().classes('gap-4'):
                    with ui.column().classes('flex-1'):
                        ui.label('Solicitado por:').classes('text-xs text-gray-400')
                        for s in inf.solicitantes: ui.chip(s.nombre, icon='campaign').props('dense color=purple-100')
                    with ui.column().classes('flex-1'):
                        ui.label('Enviar a:').classes('text-xs text-gray-400')
                        for d in inf.destinatarios: ui.chip(d.nombre, icon='send').props('dense color=green-100')
                
                ui.separator().classes('my-4')
                ui.label('Variables Configuradas:').classes('font-bold text-sm')
                for cfg in inf.configuracion_variables:
                    ui.label(f"• {cfg.variable.etiqueta_visual}: {cfg.valor_parametro}").classes('text-xs ml-2')

        dialogo_detalle.open()

    # --- Desactivación (Igual) ---
    def confirmar_desactivacion(id_inf): estado['id_desactivar'] = id_inf; motivo_input['texto'] = ''; dialogo_motivo.open()
    def procesar_desactivacion():
        if not motivo_input['texto'].strip(): ui.notify('Motivo obligatorio', type='negative'); return
        with get_db_session() as db:
            inf = db.query(Informe).get(estado['id_desactivar'])
            inf.activo = False; inf.tipo_novedad = "Desactivación"; inf.fecha_desactivacion = datetime.now(); inf.motivo_desactivacion = motivo_input['texto']; db.commit()
        ui.notify('Desactivado', type='warning'); dialogo_motivo.close(); render_ui()
    def activar_informe(id_inf):
        with get_db_session() as db:
            inf = db.query(Informe).get(id_inf)
            inf.activo = True; inf.tipo_novedad = "Actualización"; inf.fecha_actualizacion = datetime.now(); inf.motivo_desactivacion = None; db.commit()
        ui.notify('Reactivado', type='positive'); render_ui()
    def manejar_toggle(fila):
        if fila['activo']: confirmar_desactivacion(fila['id'])
        else: activar_informe(fila['id'])

    with ui.dialog() as dialogo_motivo, ui.card().classes('min-w-[400px]'):
        ui.label('⚠️ Desactivar').classes('text-xl font-bold text-red-600')
        ui.textarea(placeholder='Motivo...').bind_value(motivo_input, 'texto').classes('w-full')
        with ui.row().classes('w-full justify-end mt-4'):
            ui.button('Cancelar', on_click=dialogo_motivo.close).props('flat')
            ui.button('Confirmar', color='red', on_click=procesar_desactivacion)

    # --- RENDER LISTA (Igual) ---
    def render_lista():
        contenedor_principal.clear()
        with contenedor_principal:
            with ui.row().classes('w-full justify-between items-center mb-4'):
                ui.label('Gestor de Informes').classes('text-2xl font-bold text-slate-800')
                with ui.row().classes('gap-2'):
                    ui.input(placeholder='Buscar...', on_change=lambda e: render_ui()).bind_value(estado, 'filtro')
                    ui.button('Nuevo', icon='add', on_click=ir_a_formulario_nuevo).classes('bg-slate-900 text-white')

            with get_db_session() as db:
                query = db.query(Informe)
                if estado['filtro']: query = query.filter(Informe.nombre.ilike(f"%{estado['filtro']}%"))
                informes = query.order_by(Informe.id).all()
                cols = [{'name': 'nombre', 'label': 'Nombre', 'field': 'nombre', 'align': 'left'}, {'name': 'frec', 'label': 'Frecuencia', 'field': 'frec', 'align': 'left'}, {'name': 'novedad', 'label': 'Estado', 'field': 'novedad', 'align': 'left'}, {'name': 'acciones', 'label': 'Acciones', 'field': 'acciones', 'align': 'center'}]
                rows = []
                for i in informes:
                    rec = i.recurrencia_json or {}
                    modo = rec.get('modo', 'CICLICO')
                    if modo == 'INMEDIATO': txt = "A Demanda"
                    elif modo == 'ESPECIFICO': txt = "Meses Esp."
                    else: txt = "Diario" if int(rec.get('intervalo', 0))==1 else f"Cada {rec.get('intervalo')} días"
                    rows.append({'id': i.id, 'nombre': i.nombre, 'frec': txt, 'novedad': i.tipo_novedad, 'activo': i.activo, 'color': 'green' if i.activo else 'red'})
                if not rows:
                    # Estado vacío: distingue "no hay ninguno" de "sin resultados de búsqueda".
                    if estado['filtro']:
                        estado_vacio('search_off', 'Sin resultados',
                                     f'Ningún informe coincide con "{estado["filtro"]}". '
                                     'Prueba con otro término.')
                    else:
                        estado_vacio(
                            'description',
                            'Aún no tienes informes',
                            'Un informe es la obligación que el sistema programa en el tiempo y '
                            'audita: cada cuánto vence, quién responde y si se cumplió. '
                            'Aquí defines su cronograma y sus responsables.',
                            'Crear primer informe',
                            ir_a_formulario_nuevo)
                else:
                    tabla = ui.table(columns=cols, rows=rows).classes('w-full shadow-sm border rounded')
                    tabla.add_slot('body-cell-novedad', '<q-td :props="props"><q-badge :color="props.row.color">{{ props.row.novedad }}</q-badge></q-td>')
                    tabla.add_slot('body-cell-acciones', '''<q-td :props="props"><q-btn icon="visibility" flat dense color="teal" @click="$parent.$emit('detail', props.row.id)" /><q-btn icon="edit" flat dense color="primary" @click="$parent.$emit('edit', props.row.id)" /><q-btn :icon="props.row.activo ? 'block' : 'check_circle'" flat dense :color="props.row.activo ? 'warning' : 'positive'" @click="$parent.$emit('tog', props.row)" /></q-td>''')
                    tabla.on('edit', lambda e: ir_a_editar(e.args)); tabla.on('tog', lambda e: manejar_toggle(e.args)); tabla.on('detail', lambda e: ver_detalle(e.args))

    # --- RENDER FORMULARIO (ACTUALIZADO) ---
    def render_formulario():
        contenedor_principal.clear()
        with contenedor_principal:
            with ui.row().classes('w-full items-center mb-4'):
                ui.button(icon='arrow_back', on_click=ir_a_lista).props('flat round')
                ui.label("Edición" if estado['id_edicion'] else "Nuevo Informe").classes('text-xl font-bold')

            with get_db_session() as db:
                usuarios = db.query(Usuario).all()
                vars_db = db.query(MetaVariable).all()
                sh_db = db.query(Stakeholder).all()
                
                opciones_users = {str(u.id): u.nombre for u in usuarios}
                # --- CARGAR TERCEROS PARA SELECTOR ---
                opciones_sh = {str(s.id): f"{s.nombre} ({s.tipo})" for s in sh_db}

            with ui.row().classes('w-full gap-6'):
                # COL 1
                with ui.card().classes('w-1/2 p-4'):
                    ui.label('Datos Básicos').classes('text-lg font-bold text-blue-800')
                    ui.input('Nombre').bind_value(nuevo_informe, 'nombre').classes('w-full')
                    ui.textarea('Descripción').bind_value(nuevo_informe, 'descripcion').classes('w-full')

                    ui.label('Formatos').classes('mt-2 text-sm font-bold')
                    ui.select(['pdf', 'xlsx', 'xls', 'docx', 'zip'], multiple=True).bind_value(nuevo_informe, 'extensiones').classes('w-full')

                    ui.label('Responsables (Usuarios)').classes('mt-2 text-sm font-bold')
                    ui.select(opciones_users, multiple=True).bind_value(nuevo_informe, 'responsables').classes('w-full')

                    # --- NUEVO: STAKEHOLDERS ---
                    ui.separator().classes('my-2')
                    ui.label('Solicitado por:').classes('text-sm font-bold')
                    ui.select(opciones_sh, multiple=True, label='Seleccione Entidades').bind_value(nuevo_informe, 'solicitantes').classes('w-full')

                    ui.label('Destinatarios (Envío):').classes('text-sm font-bold')
                    ui.select(opciones_sh, multiple=True, label='Seleccione Entidades').bind_value(nuevo_informe, 'destinatarios').classes('w-full')

                # COL 2 (Tiempo)
                with ui.card().classes('w-1/2 p-0 overflow-hidden border-t-4 border-purple-600'):
                    with ui.row().classes('items-center gap-0 no-wrap px-4 pt-3'):
                        ui.label('Cronograma').classes('font-bold text-purple-800 text-sm')
                        ayuda('Define cuándo vence este informe. '
                              'Cíclico: cada cierto intervalo (ej. cada 30 días). '
                              'Específico: en meses y día de corte concretos (ej. día 7 de marzo y septiembre). '
                              'Semanal: en días de la semana. '
                              'Por Horas: varias veces al día. '
                              'A Demanda: sin cronograma fijo, se genera cuando se solicita. '
                              'Las casillas de "día hábil" mueven la fecha si cae en fin de semana o festivo.')
                    with ui.tabs().classes('bg-slate-100 text-slate-600 w-full') as tabs:
                        t_cic = ui.tab('Cíclico'); t_esp = ui.tab('Específico'); t_sem = ui.tab('Semanal'); t_hor = ui.tab('Por Horas'); t_inm = ui.tab('A Demanda')
                    
                    def on_tab(e): 
                        modos = {'Cíclico': 'CICLICO', 'Específico': 'ESPECIFICO', 'Semanal': 'SEMANAL', 'Por Horas': 'HOURLY', 'A Demanda': 'INMEDIATO'}
                        nuevo_informe['modo_tiempo'] = modos.get(e.value, 'CICLICO')
                    tabs.on_value_change(on_tab)
                    
                    curr = nuevo_informe['modo_tiempo']
                    map_tabs = {'CICLICO': 'Cíclico', 'ESPECIFICO': 'Específico', 'SEMANAL': 'Semanal', 'HOURLY': 'Por Horas', 'INMEDIATO': 'A Demanda'}
                    tabs.set_value(map_tabs.get(curr, 'Cíclico'))

                    # Fecha inicio compartida por todos los modos
                    with ui.row().classes('w-full px-4 pt-4'):
                        ui.input('Fecha Inicio', value=nuevo_informe['fecha_inicio'], on_change=refrescar_vista_previa).props('type="date"').bind_value(nuevo_informe, 'fecha_inicio').classes('w-full')

                    # --- Vista previa de los vencimientos --------------
                    # Definir la obligación era, hasta ahora, un acto a ciegas:
                    # se elegía periodicidad y ajuste, se guardaba, y las fechas
                    # solo se veían cuando el planificador ya había creado las
                    # tareas. Una regla mal entendida no falla ruidosamente —
                    # genera fechas distintas de las que la persona creía, o
                    # ninguna.
                    contenedor_vista_previa = None  # se crea bajo las pestañas

                    def refrescar_vista_previa(*_):
                        if contenedor_vista_previa is None:
                            return  # aún no se ha construido la interfaz
                        contenedor_vista_previa.clear()
                        with contenedor_vista_previa:
                            with ui.card().classes('w-full bg-slate-50 mt-3 p-3'):
                                with ui.row().classes('items-center gap-2'):
                                    ui.icon('event_available').classes('text-slate-500')
                                    ui.label('Próximos vencimientos con esta regla') \
                                        .classes('text-sm font-bold text-slate-700')
                                try:
                                    venc, aviso = _proximos_vencimientos(
                                        _construir_regla(nuevo_informe), cuantos=6)
                                except Exception as e:  # noqa: BLE001
                                    ui.label(f'No se pudo calcular: {e}') \
                                        .classes('text-xs text-red-600')
                                    return
                                if aviso:
                                    ui.label(aviso).classes('text-xs text-amber-700')
                                for v in venc:
                                    with ui.row().classes('items-center gap-2'):
                                        ui.icon('chevron_right').classes('text-xs text-slate-400')
                                        ui.label(v.describir()).classes(
                                            'text-xs ' + ('text-amber-800' if v.se_ajusto
                                                          else 'text-slate-600'))

                    with ui.tab_panels(tabs, value=tabs.value).classes('w-full p-4'):
                        with ui.tab_panel(t_cic):
                            with ui.row().classes('w-full gap-2'):
                                ui.number('Años', format='%.0f', on_change=calcular_dias).bind_value(nuevo_informe['tiempo_ciclico'], 'anos').classes('w-1/3')
                                ui.number('Meses', format='%.0f', on_change=calcular_dias).bind_value(nuevo_informe['tiempo_ciclico'], 'meses').classes('w-1/3')
                                ui.number('Días', format='%.0f', on_change=calcular_dias).bind_value(nuevo_informe['tiempo_ciclico'], 'dias').classes('w-1/4')
                            ui.separator().classes('my-2')
                            # UN selector, no dos casillas. Antes se podían marcar
                            # las dos a la vez y el motor resolvía con un `elif`:
                            # ganaba «siguiente» EN SILENCIO. Y la dirección no es
                            # cosmética — si un plazo legal cae en sábado, moverlo
                            # al lunes puede ser TARDE. Quien pedía «anterior»
                            # recibía lo contrario sin enterarse. Con un selector,
                            # ese estado no se puede expresar.
                            ui.select(AJUSTE_HABIL, label='Si cae en día no hábil',
                                      on_change=refrescar_vista_previa) \
                                .bind_value(nuevo_informe['tiempo_ciclico'], 'ajuste') \
                                .classes('w-full').tooltip(AYUDA_AJUSTE)

                        with ui.tab_panel(t_esp):
                            nombres_meses = {1:'Ene', 2:'Feb', 3:'Mar', 4:'Abr', 5:'May', 6:'Jun', 7:'Jul', 8:'Ago', 9:'Sep', 10:'Oct', 11:'Nov', 12:'Dic'}
                            ui.label('Meses de entrega:').classes('text-sm font-bold')
                            with ui.row().classes('w-full flex-wrap gap-1'):
                                for num, nombre in nombres_meses.items():
                                    def toggle_mes(e, m=num):
                                        sel = nuevo_informe['tiempo_especifico']['meses_seleccionados']
                                        if m in sel: sel.remove(m)
                                        else: sel.append(m)
                                        refrescar_vista_previa()
                                    checked = num in nuevo_informe['tiempo_especifico']['meses_seleccionados']
                                    ui.checkbox(nombre, value=checked, on_change=toggle_mes).props('dense')
                            ui.separator().classes('my-2')
                            with ui.row().classes('w-full gap-4 items-center'):
                                ui.number('Día de corte', min=1, max=31, format='%.0f', on_change=refrescar_vista_previa).bind_value(nuevo_informe['tiempo_especifico'], 'dia_corte').classes('w-1/3')
                                ui.checkbox('Último día del mes', on_change=refrescar_vista_previa).bind_value(nuevo_informe['tiempo_especifico'], 'es_ultimo_dia')
                            ui.select(AJUSTE_HABIL, label='Si cae en día no hábil',
                                      on_change=refrescar_vista_previa) \
                                .bind_value(nuevo_informe['tiempo_especifico'], 'ajuste') \
                                .classes('w-full mt-2').tooltip(AYUDA_AJUSTE)

                        with ui.tab_panel(t_sem):
                            dias_semana = {0:'Lunes', 1:'Martes', 2:'Miércoles', 3:'Jueves', 4:'Viernes', 5:'Sábado', 6:'Domingo'}
                            ui.label('Días de entrega:').classes('text-sm font-bold')
                            with ui.row().classes('w-full flex-wrap gap-2'):
                                for num, nombre in dias_semana.items():
                                    def toggle_dia(e, d=num):
                                        sel = nuevo_informe['tiempo_semanal']['dias_seleccionados']
                                        if d in sel: sel.remove(d)
                                        else: sel.append(d)
                                    checked = num in nuevo_informe['tiempo_semanal']['dias_seleccionados']
                                    ui.checkbox(nombre, value=checked, on_change=toggle_dia).props('dense')
                            ui.separator().classes('my-2')
                            ui.checkbox('Ajustar a día hábil').bind_value(nuevo_informe['tiempo_semanal'], 'mover_habil').props('color=purple')

                        with ui.tab_panel(t_hor):
                            with ui.row().classes('w-full gap-4'):
                                ui.number('Hora inicio (0-23)', min=0, max=23, format='%.0f').bind_value(nuevo_informe['tiempo_horas'], 'hora_inicio').classes('w-1/3')
                                ui.number('Hora fin (0-23)', min=0, max=23, format='%.0f').bind_value(nuevo_informe['tiempo_horas'], 'hora_fin').classes('w-1/3')
                                ui.number('Intervalo (horas)', min=1, max=12, format='%.0f').bind_value(nuevo_informe['tiempo_horas'], 'intervalo').classes('w-1/3')

                        with ui.tab_panel(t_inm):
                            ui.label('Este informe se genera a demanda.').classes('text-gray-500 italic')
                            ui.label('No tiene periodicidad automática. Las entregas se crean manualmente desde "Entregas Manuales".').classes('text-sm text-gray-400')

                    contenedor_vista_previa = ui.column().classes('w-full px-4 pb-3')
                    refrescar_vista_previa()

            # NUEVA SECCIÓN: CLASIFICACIÓN Y ORIGEN v3.0
            with ui.row().classes('w-full gap-4 mt-4'):
                with ui.card().classes('w-1/2 p-4 border-l-4 border-teal-500'):
                    ui.label('Clasificación y Origen').classes('text-lg font-bold text-teal-800')

                    # Tipo de Flujo: se MUESTRA, no se elige. Lo determinan las
                    # partes (abajo: responsables, solicitantes, destinatarios).
                    # Dejarlo elegible permitía marcar "Recepción Externa" un
                    # informe cuyas partes eran todas internas.
                    ETIQUETAS_FLUJO = {
                        'ENVIO_INTERNO': 'Envío Interno',
                        'ENVIO_EXTERNO': 'Envío Externo',
                        'RECEPCION_EXTERNA': 'Recepción Externa',
                        'BIDIRECCIONAL': 'Bidireccional',
                    }

                    def _flujo_derivado() -> str:
                        sale = bool(nuevo_informe['destinatarios'])
                        entra = nuevo_informe.get('origen_externo') or False
                        if sale and entra:
                            return 'BIDIRECCIONAL'
                        if sale:
                            return 'ENVIO_EXTERNO'
                        if entra:
                            return 'RECEPCION_EXTERNA'
                        return 'ENVIO_INTERNO'

                    with ui.row().classes('w-full items-center gap-2'):
                        ui.label('Tipo de Flujo:').classes('text-sm text-gray-600')
                        lbl_flujo = ui.label().classes('text-sm font-bold text-teal-700')
                        ayuda('Se deduce de las partes: si hay destinatarios externos '
                              'el informe sale; si el origen es un tercero, entra.')
                    lbl_flujo.bind_text_from(
                        nuevo_informe, 'destinatarios',
                        backward=lambda _: ETIQUETAS_FLUJO[_flujo_derivado()])

                    # Subsistema
                    ui.input('Subsistema').bind_value(nuevo_informe, 'subsistema').classes('w-full').props('outlined')

                    # Conjunto de Datos
                    with get_db_session() as db2:
                        conjuntos = db2.query(ConjuntoDatos).filter_by(activo=True).order_by(ConjuntoDatos.nombre).all()
                        opciones_conjuntos = {str(c.id): c.nombre for c in conjuntos}
                    opciones_conjuntos[''] = 'Sin asignar'
                    ui.select(
                        label='Conjunto de Datos',
                        options=opciones_conjuntos,
                        with_input=True
                    ).bind_value(nuevo_informe, 'conjunto_datos_id').classes('w-full').props('outlined')

                    # Unidad Organizacional (v4.0)
                    with get_db_session() as db3:
                        unidades = db3.query(UnidadOrganizacional).filter_by(activo=True).order_by(UnidadOrganizacional.nombre).all()
                        opts_unidad = {None: '— Sin unidad —'}
                        opts_unidad.update({u.id: u.nombre for u in unidades})
                    ui.select(
                        label='Unidad Responsable',
                        options=opts_unidad,
                    ).bind_value(nuevo_informe, 'unidad_id').classes('w-full').props('outlined')

                    # Aplicativo Origen
                    ui.input('Aplicativo/Herramienta Origen').bind_value(nuevo_informe, 'aplicativo_origen').classes('w-full').props('outlined')

                with ui.card().classes('w-1/2 p-4 border-l-4 border-orange-500'):
                    ui.label('Plantillas y Normatividad').classes('text-lg font-bold text-orange-800')

                    # Plantillas
                    ui.input('URL Plantilla').bind_value(nuevo_informe, 'url_plantilla').classes('w-full').props('outlined')
                    ui.input('Ruta Plantilla (Filesystem)').bind_value(nuevo_informe, 'ruta_plantilla').classes('w-full').props('outlined')

                    # Justificación
                    ui.textarea('Justificación / Normatividad').bind_value(nuevo_informe, 'justificacion_normatividad').classes('w-full').props('outlined rows=3')

                    # Acuerdo de Confidencialidad
                    ui.separator().classes('my-2')
                    with ui.row().classes('w-full items-center gap-2'):
                        ui.checkbox('Requiere Acuerdo de Confidencialidad').bind_value(nuevo_informe, 'tiene_acuerdo_confidencialidad')

                    ui.input('Cargo Autorizador').bind_value(nuevo_informe, 'cargo_autorizador').classes('w-full').props('outlined')

            # COL 3 (Variables - Igual)
            with ui.card().classes('w-full mt-4 p-4 bg-green-50'):
                ui.label('Variables').classes('text-lg font-bold text-green-900')
                with ui.column().classes('w-full gap-2'):
                    for v in vars_db:
                        with ui.row().classes('items-center justify-between border-b p-2'):
                            checked = str(v.id) in nuevo_informe['variables_config'] and nuevo_informe['variables_config'][str(v.id)] is not None
                            chk = ui.checkbox(v.etiqueta_visual, value=checked).classes('text-bold')
                            ui.input(placeholder='Valor').bind_value(nuevo_informe['variables_config'], str(v.id)).bind_visibility_from(chk, 'value').classes('w-1/2')
                            def up(e, vid=str(v.id)):
                                if not e.value: nuevo_informe['variables_config'][vid] = None
                                elif nuevo_informe['variables_config'].get(vid) is None: nuevo_informe['variables_config'][vid] = ""
                            chk.on('change', up)

            ui.button('Guardar', on_click=guardar).classes('mt-6 w-full bg-slate-900 text-white')

    render_ui()