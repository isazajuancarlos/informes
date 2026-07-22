from nicegui import ui
from app.core.database import get_db_session
from app.models.meta_tables import (
    Informe, Usuario, MetaVariable, ConfiguracionInformeVariable, Stakeholder,
    ConjuntoDatos, UnidadOrganizacional, Participacion,
    ROL_ORIGEN, ROL_DESTINO, ROL_USUARIO,
    AMBITO_FICHA, AMBITO_CONTENIDO,
)
from app.ui.ui_helpers import estado_vacio, ayuda
from app.ui.asistente_cronograma import abrir_asistente
from datetime import datetime
from app.services.preview_calendario import proximos as _proximos_vencimientos
from app.services.validar_regla import validar as _validar_regla

#: Las tres únicas respuestas posibles a «¿y si cae en día no hábil?». Ser un
#: selector y no dos casillas es lo que impide el estado ambiguo por
#: construcción, en vez de tener que validarlo después.
#: Qué hacer cuando el día de vencimiento no es hábil.
#:
#: Las tres opciones NO mueven el vencimiento. El día de vencimiento es el que
#: fija la norma —el 1 de enero de un informe anual sigue venciendo el 1 de
#: enero, sea festivo o no—; lo que se elige aquí es **qué día se entrega**.
#: Redactarlas como «mover la fecha» era lo que hacía creer que la obligación
#: cambiaba de sitio, y con ella el período al que pertenece.
AJUSTE_HABIL = {
    'ninguno': 'Se entrega ese mismo día, sea hábil o no',
    'anterior': 'Se entrega el día hábil anterior (el vencimiento no cambia)',
    'siguiente': 'Hay plazo de gracia hasta el siguiente día hábil',
}
AYUDA_AJUSTE = ("El día de vencimiento no cambia nunca: es el que fija la norma. "
                "Lo que se elige es hasta cuándo hay para entregarlo si ese día "
                "no es hábil.\n\n"
                "«Día hábil anterior» es la opción segura para plazos legales —"
                "adelantar nunca llega tarde, aplazar sí— y no concede plazo "
                "extra: es una regla de control interno.\n\n"
                "«Plazo de gracia» sí amplía el plazo: entregar ese día NO cuenta "
                "como incumplimiento. Úselo solo si la norma o el contrato lo "
                "conceden.")


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


#: Una parte del flujo puede ser INTERNA o EXTERNA, y el selector tiene que
#: admitir las dos. Antes solo ofrecía terceros externos, y eso tenía una
#: consecuencia medida: de los 397 informes de la base real, **208 son de envío
#: interno**. Con un selector que solo admite externos, exigir destinatario
#: haría que más de la mitad de la base no se pudiera registrar — y de paso
#: dejaría sin usar el tipo de flujo «Envío Interno», que el propio modelo
#: deduce de que NINGÚN destino sea externo.
#:
#: El modelo ya lo soporta: `Participacion` lleva `usuario_id` o
#: `stakeholder_id`. Lo que faltaba era la pantalla.
PREFIJO_INTERNO = "u"
PREFIJO_EXTERNO = "s"


def clave_de_parte(participacion) -> str:
    """`Participacion` → la clave que usa el selector («u:12», «s:5»)."""
    if participacion.stakeholder_id:
        return f"{PREFIJO_EXTERNO}:{participacion.stakeholder_id}"
    return f"{PREFIJO_INTERNO}:{participacion.usuario_id}"


def descomponer_parte(clave: str):
    """«s:5» → («s», 5). Devuelve `None` si la clave no se entiende.

    Se devuelve `None` en vez de suponer un tipo: una clave ilegible que se
    interpretara como interna crearía una participación de la persona
    equivocada, y eso no se ve nunca — el informe queda creado y con partes que
    nadie eligió.
    """
    try:
        prefijo, ident = str(clave).split(":", 1)
    except (ValueError, AttributeError):
        return None
    if prefijo not in (PREFIJO_INTERNO, PREFIJO_EXTERNO) or not ident.isdigit():
        return None
    return (prefijo, int(ident))


def opciones_de_partes(usuarios, stakeholders) -> dict:
    """Las opciones del selector, internas y externas en una sola lista.

    Se marca cuál es cuál en la etiqueta: de eso depende el tipo de flujo del
    informe, así que quien elige tiene que poder verlo sin abrir otra pantalla.
    """
    opciones = {f"{PREFIJO_EXTERNO}:{s.id}": f"{s.nombre} · externo"
                + (f" ({s.tipo})" if s.tipo else "")
                for s in stakeholders}
    opciones.update({f"{PREFIJO_INTERNO}:{u.id}": f"{u.nombre} · interno"
                     for u in usuarios})
    return opciones


def faltan_campos(definiciones, valores) -> list[str]:
    """Los campos marcados como obligatorios que vienen en blanco.

    `es_obligatorio` estaba declarado desde el principio y **no lo miraba
    nadie**: quien creaba un campo y lo marcaba obligatorio creía haber acotado
    algo, y no había acotado nada. Una bandera que no bloquea es peor que no
    tenerla, porque promete un control que no existe.

    Recibe las definiciones ya leídas para poder comprobarse sin base de datos.
    """
    faltan = []
    for definicion in definiciones:
        if not definicion.get('obligatorio'):
            continue
        valor = valores.get(str(definicion['id']))
        if valor is None or str(valor).strip() == '':
            faltan.append(definicion['etiqueta'])
    return faltan


def _nombre_interno(etiqueta: str) -> str:
    """«Aplicativo de Origen» → `aplicativo_de_origen`.

    El nombre técnico se DERIVA del visible en vez de pedirlo aparte. Pedir dos
    nombres para lo mismo obliga a inventarse un identificador a quien solo
    quería añadir un campo, y garantiza que con el tiempo digan cosas distintas.
    """
    import re
    import unicodedata

    sin_tildes = unicodedata.normalize('NFKD', str(etiqueta or '')) \
        .encode('ascii', 'ignore').decode()
    limpio = re.sub(r'[^a-z0-9]+', '_', sin_tildes.lower()).strip('_')
    return (limpio or 'campo')[:50]


def flujo_derivado(datos: dict) -> str:
    """El tipo de flujo, deducido de las partes. NUNCA se elige a mano.

    Sale información si algún DESTINO es **externo**; entra si el origen lo es.
    El matiz «externo» es todo: aquí se miraba `bool(destinatarios)`, y en
    cuanto el selector empezó a admitir partes internas un informe entregado a
    otra área de la propia entidad se habría marcado «Envío Externo». Es el
    dato del que cuelga después todo el análisis de qué información sale de la
    entidad.

    Es la misma regla que `Informe.tipo_flujo` aplica sobre lo ya guardado; aquí
    se aplica sobre lo que hay en pantalla, para que se vea antes de guardar.
    """
    sale = any(str(c).startswith(f"{PREFIJO_EXTERNO}:")
               for c in datos.get('destinatarios') or [])
    entra = bool(datos.get('origen_externo'))
    if sale and entra:
        return 'BIDIRECCIONAL'
    if sale:
        return 'ENVIO_EXTERNO'
    if entra:
        return 'RECEPCION_EXTERNA'
    return 'ENVIO_INTERNO'


def faltan_partes(datos: dict) -> list[str]:
    """Qué le falta al flujo para que el informe pueda guardarse.

    Un informe sin partes es un informe que no se le envía a nadie y que nadie
    consume: el sistema lo programaría, generaría entregas, y no habría quien
    las reciba ni quien reclame si no llegan. Se cae en cuanto alguien lo mira,
    y por eso hay que impedirlo al crearlo, no descubrirlo después.

    Se exigen las DOS —quién lo pide y quién lo recibe— porque son preguntas
    distintas y la respuesta puede ser la misma entidad: el Ministerio que lo
    exige suele ser también el que lo recibe, y entonces se marca en las dos.
    Lo que no puede quedar es en blanco, porque en blanco no significa «no
    aplica»: significa que nadie lo contestó.
    """
    faltan = []
    if not datos.get('solicitantes'):
        faltan.append(
            "Falta quién solicita el informe. Si es una obligación de norma, "
            "la parte que la exige suele ser también la que lo recibe: márquela "
            "en las dos listas. Si no está en la lista, créela con «+ Nueva "
            "parte».")
    if not datos.get('destinatarios'):
        faltan.append(
            "Falta a quién se entrega el informe. Puede ser una parte interna "
            "—un área de la propia entidad— o un tercero externo; de eso "
            "depende que el flujo sea interno o externo.")
    return faltan


#: Campos de regla que las pestañas de esta pantalla NO saben representar.
#:
#: No es una lista de cosas pendientes: es el motivo por el que una regla creada
#: con el asistente se guarda tal cual en vez de traducirse a los campos del
#: formulario. La traducción PIERDE — `dias_corte: [7, 20]` vuelve como
#: `dia_corte: 7` y la obligación pasa de veinticuatro entregas al año a doce,
#: sin que nadie toque nada ni vea un aviso. Solo se descubre cuando lo pregunta
#: un ente de control.
CAMPOS_QUE_LAS_PESTANAS_NO_SABEN = {
    'dias_corte', 'dia_semana_mes', 'semana_del_mes', 'cada_n_anios',
    'cada_n_semanas', 'dia_habil_semana', 'omitir_festivos', 'fechas',
}


def excede_a_las_pestanas(regla: dict) -> bool:
    """¿Esta regla se perdería al abrirla en las pestañas de siempre?"""
    if (regla or {}).get('modo') == 'FECHAS':
        return True
    return any((regla or {}).get(c) for c in CAMPOS_QUE_LAS_PESTANAS_NO_SABEN)


def _describir_regla(regla: dict) -> str:
    """Una línea que dice qué es esta regla, sin inventarse nada.

    Se recorre el camino del asistente AL REVÉS: se generan las fechas de un año
    y se le pregunta al reconocedor qué patrón son. Así la frase que ve quien
    abre un informe viejo es la misma que vería al crearlo, y no una segunda
    redacción que con el tiempo diga otra cosa.

    Si el reconocedor no encuentra un patrón exacto, se dice eso mismo en vez de
    aproximar: una descripción aproximada de una obligación es una descripción
    equivocada.
    """
    from datetime import date, timedelta

    from app.services.inferir_periodicidad import inferir
    from app.services.scheduler import GeneradorTareas

    if (regla or {}).get('modo') == 'HOURLY':
        return (f"Varias veces al día, de las {regla.get('hora_inicio')} a las "
                f"{regla.get('hora_fin')}, cada {regla.get('intervalo_horas')} h")
    try:
        inicio = date.fromisoformat(str(regla.get('fecha_inicio')))
    except (ValueError, TypeError):
        return "Cronograma definido"

    fechas = []
    for i in range(366):
        dia = inicio + timedelta(days=i)
        fechas += [v.nominal
                   for v in GeneradorTareas.obtener_vencimientos_para_fecha(dia, regla)]
    if not fechas:
        return "Cronograma definido — no genera vencimientos en el primer año"
    inferencia = inferir(fechas)
    if inferencia.periodicidad == "SIN_PATRON":
        return f"Cronograma propio — {len(fechas)} vencimiento(s) en el primer año"
    return inferencia.explicacion


def _construir_regla(datos):
    """La configuración de periodicidad, tal como se guarda.

    Extraída de `guardar()` para que la VISTA PREVIA use exactamente la misma
    construcción. Una vista previa que arme la regla de otra forma mostraría
    fechas que el sistema luego no genera — y nadie sabría cuál de las dos está
    mal, que es peor que no tener vista previa.
    """
    # Una regla del asistente se guarda VERBATIM. Reconstruirla desde los campos
    # del formulario es exactamente lo que no se puede hacer: las pestañas no
    # tienen dónde poner la mitad de lo que el asistente sabe expresar.
    if datos.get('regla'):
        return dict(datos['regla'])

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
                "dia_habil": int(datos['tiempo_especifico'].get('dia_habil') or 0),
                "usar_ultimo_dia_en_meses_cortos": bool(
                    datos['tiempo_especifico'].get('usar_ultimo_dia_en_meses_cortos')),
                "es_ultimo_dia": datos['tiempo_especifico']['es_ultimo_dia'],
                **_banderas_desde_ajuste(datos['tiempo_especifico']['ajuste'],
                                         "ajuste_habil", "ajuste_habil_anterior"),
                "fecha_inicio": f_inicio}
    if modo == 'SEMANAL':
        return {"modo": "SEMANAL",
                "dias_semana": datos['tiempo_semanal']['dias_seleccionados'],
                **_banderas_desde_ajuste(datos['tiempo_semanal']['ajuste'],
                                         "mover_habil", "mover_habil_anterior"),
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
        'conjunto_datos_id': None,
        'unidad_id': None,
        'modo_tiempo': 'CICLICO',
        'regla': None,
        'regla_resumen': '',
        'fecha_inicio': datetime.now().strftime('%Y-%m-%d'),
        'tiempo_ciclico': {'anos': 0, 'meses': 0, 'dias': 0, 'total_dias': 0, 'ajuste': 'ninguno'},
        'tiempo_especifico': {'meses_seleccionados': [], 'dia_corte': 1, 'dia_habil': 0, 'es_ultimo_dia': False, 'usar_ultimo_dia_en_meses_cortos': False, 'ajuste': 'ninguno'},
        'tiempo_semanal': {'dias_seleccionados': [], 'ajuste': 'ninguno'},
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
            'conjunto_datos_id': None,
            'unidad_id': None,
            'modo_tiempo': 'CICLICO',
            # La regla del asistente, cuando la hay. Cuando está puesta manda
            # ella y las pestañas ni se muestran: dos fuentes para la misma
            # obligación garantizan que una esté mal.
            'regla': None,
            'regla_resumen': '',
            'fecha_inicio': datetime.now().strftime('%Y-%m-%d'),
            'tiempo_ciclico': {'anos': 0, 'meses': 0, 'dias': 0, 'total_dias': 0, 'ajuste': 'ninguno'},
            'tiempo_especifico': {'meses_seleccionados': [], 'dia_corte': 1, 'dia_habil': 0, 'es_ultimo_dia': False, 'usar_ultimo_dia_en_meses_cortos': False, 'ajuste': 'ninguno'},
            'tiempo_semanal': {'dias_seleccionados': [], 'ajuste': 'ninguno'},
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
            # Las partes del flujo, internas Y externas. Cargarlas desde
            # `inf.solicitantes` / `inf.destinatarios` perdía las internas: esas
            # dos propiedades filtran `externas=True` a propósito, porque el
            # portal de terceros las usa para otra cosa.
            nuevo_informe['solicitantes'] = [clave_de_parte(p)
                                             for p in inf.partes(ROL_USUARIO)]
            nuevo_informe['destinatarios'] = [clave_de_parte(p)
                                              for p in inf.partes(ROL_DESTINO)]

            # Cargar nuevos campos v3.0 + v4.0 (tipo_flujo es derivado)
            nuevo_informe['conjunto_datos_id'] = str(inf.conjunto_datos_id) if inf.conjunto_datos_id else None
            nuevo_informe['unidad_id'] = inf.unidad_id

            datos = inf.datos_generales or {}
            nuevo_informe['extensiones'] = datos.get('extensiones', [])
            for cfg in inf.configuracion_variables: nuevo_informe['variables_config'][str(cfg.variable_id)] = cfg.valor_parametro
            
            rec = inf.recurrencia_json or {}
            modo = rec.get('modo', 'CICLICO')
            nuevo_informe['modo_tiempo'] = modo
            nuevo_informe['fecha_inicio'] = rec.get('fecha_inicio', datetime.now().strftime('%Y-%m-%d'))

            # Una regla que las pestañas no saben representar se queda ENTERA y
            # no se reparte por los campos. Antes se repartía igualmente y lo
            # que no cabía se perdía al guardar: abrir un informe quincenal para
            # cambiarle el nombre lo convertía en mensual. Nadie lo veía, porque
            # el efecto de una fecha que deja de generarse es que no pasa nada.
            if excede_a_las_pestanas(rec):
                nuevo_informe['regla'] = dict(rec)
                nuevo_informe['regla_resumen'] = _describir_regla(rec)

            if modo == 'CICLICO':
                total = int(rec.get('intervalo', 0))
                nuevo_informe['tiempo_ciclico'].update({'total_dias': total, 'anos': total // 360, 'meses': (total % 360) // 30, 'dias': (total % 360) % 30, 'ajuste': _ajuste_desde_banderas(rec.get('mover_habil'), rec.get('mover_habil_anterior'))})
            elif modo == 'ESPECIFICO':
                nuevo_informe['tiempo_especifico'].update({'meses_seleccionados': rec.get('meses', []), 'dia_corte': rec.get('dia_corte', 1), 'es_ultimo_dia': rec.get('es_ultimo_dia', False), 'ajuste': _ajuste_desde_banderas(rec.get('ajuste_habil'), rec.get('ajuste_habil_anterior'))})
            elif modo == 'SEMANAL':
                nuevo_informe['tiempo_semanal'].update({'dias_seleccionados': rec.get('dias_semana', []), 'ajuste': _ajuste_desde_banderas(rec.get('mover_habil'), rec.get('mover_habil_anterior'))})
            elif modo == 'HOURLY':
                nuevo_informe['tiempo_horas'].update({'hora_inicio': rec.get('hora_inicio', 8), 'hora_fin': rec.get('hora_fin', 18), 'intervalo': rec.get('intervalo_horas', 1)})
        render_ui()

    def guardar():
        if not nuevo_informe['nombre']: ui.notify('Nombre obligatorio', type='warning'); return
        if not nuevo_informe['extensiones']: ui.notify('Seleccione extensiones', type='warning'); return

        problemas_de_partes = faltan_partes(nuevo_informe)
        if problemas_de_partes:
            for problema in problemas_de_partes:
                ui.notify(problema, type='negative', multi_line=True,
                          classes='w-[420px]', timeout=8000)
            return

        with get_db_session() as db:
            definiciones = [{'id': c.id, 'etiqueta': c.etiqueta_visual,
                             'obligatorio': bool(c.es_obligatorio)}
                            for c in db.query(MetaVariable).filter(
                                (MetaVariable.activo == True)
                                | (MetaVariable.activo.is_(None))).all()]
        sin_contestar = faltan_campos(definiciones, nuevo_informe['variables_config'])
        if sin_contestar:
            ui.notify('Faltan campos obligatorios: ' + ', '.join(sin_contestar),
                      type='negative', multi_line=True, classes='w-[420px]',
                      timeout=8000)
            return

        # Aquí se leían `modo` y `f_inicio` sin usarlos para nada: quedaron de
        # cuando esta función armaba la regla a mano. Ahora la arma
        # `_construir_regla`, y dejarlos sugería que el modo del formulario
        # decide algo cuando puede estar mandando el cronograma del asistente.
        rec_final = _construir_regla(nuevo_informe)

        # La regla se valida ANTES de guardar. Un barrido del motor encontró
        # varias configuraciones que se aceptaban en silencio y dejaban una
        # obligación que no hace lo que su autor cree — y ese error no se
        # descubre por su efecto, porque el efecto es que NO PASA NADA.
        revision = _validar_regla(rec_final)
        if not revision.se_puede_guardar:
            for problema in revision.errores:
                ui.notify(problema, type='negative', multi_line=True,
                          classes='w-[420px]', timeout=8000)
            return
        for aviso in revision.advertencias:
            # Se avisa y se guarda: la regla funciona, solo puede que no como se
            # espera. Bloquear aquí haría que alguien acabara buscándole la
            # vuelta al validador, y entonces no serviría para nada.
            ui.notify(aviso, type='warning', multi_line=True,
                      classes='w-[420px]', timeout=7000)

        datos_gen = {"extensiones": nuevo_informe['extensiones']}

        with get_db_session() as db:
            if estado['id_edicion']:
                inf = db.query(Informe).get(estado['id_edicion'])
                inf.nombre = nuevo_informe['nombre']; inf.descripcion = nuevo_informe['descripcion']
                inf.recurrencia_json = rec_final; inf.datos_generales = datos_gen
                # Los siete campos de ficha que se asignaban aquí —subsistema,
                # cargo autorizador, plantillas, normatividad, aplicativo de
                # origen— YA NO SE TOCAN: dejaron de ser columnas fijas y son
                # campos definidos por el administrador.
                #
                # Y no basta con quitarlos del formulario: si se quedaran estas
                # asignaciones, editar un informe para cambiarle el nombre
                # habría escrito la cadena vacía en los siete y BORRADO lo que
                # tenía. `migrar_campos_ficha.py` mueve los valores viejos.
                #
                # tipo_flujo no se asigna: lo determinan las partes del flujo.
                inf.conjunto_datos_id = int(nuevo_informe['conjunto_datos_id']) if nuevo_informe['conjunto_datos_id'] else None
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
                    conjunto_datos_id=int(nuevo_informe['conjunto_datos_id']) if nuevo_informe['conjunto_datos_id'] else None,
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

            for rol, clave_lista in ((ROL_USUARIO, 'solicitantes'),
                                     (ROL_DESTINO, 'destinatarios')):
                for orden, clave in enumerate(nuevo_informe[clave_lista]):
                    parte = descomponer_parte(clave)
                    if parte is None:
                        continue  # clave ilegible: no se adivina de quién es
                    prefijo, ident = parte
                    if prefijo == PREFIJO_EXTERNO:
                        if db.query(Stakeholder).get(ident):
                            db.add(Participacion(informe_id=inf.id,
                                                 stakeholder_id=ident,
                                                 rol=rol, orden=orden))
                    elif db.query(Usuario).get(ident):
                        db.add(Participacion(informe_id=inf.id, usuario_id=ident,
                                             rol=rol, orden=orden))

            for var_id, val in nuevo_informe['variables_config'].items():
                if val: db.add(ConfiguracionInformeVariable(informe_id=inf.id, variable_id=int(var_id), valor_parametro=str(val)))
            db.commit()
        
        ui.notify(f'Informe {msg}', type='positive'); ir_a_lista()

    # --- LOS CAMPOS, DIBUJADOS DESDE LO QUE EL ADMINISTRADOR DEFINIÓ ---
    def _pintar_campos(ambito: str, vacio: str):
        """Un control por campo definido, del tipo que el campo declara.

        Antes todos los campos se capturaban con un `ui.input` de texto, aunque
        el campo dijera ser una fecha o una lista. El tipo estaba declarado y no
        servía para nada: quien lo definía creía haber acotado el dato y no
        había acotado nada.
        """
        with get_db_session() as db:
            campos = db.query(MetaVariable) \
                .filter(MetaVariable.ambito == ambito) \
                .filter((MetaVariable.activo == True) | (MetaVariable.activo.is_(None))) \
                .order_by(MetaVariable.orden, MetaVariable.id).all()
            campos = [{'id': c.id, 'etiqueta': c.etiqueta_visual,
                       'tipo': c.tipo_dato, 'obligatorio': bool(c.es_obligatorio),
                       'descripcion': c.descripcion,
                       'opciones': c.lista_de_opciones} for c in campos]

        if not campos:
            ui.label(vacio).classes('text-xs text-slate-500')
            return

        valores = nuevo_informe['variables_config']
        for campo in campos:
            clave = str(campo['id'])
            valores.setdefault(clave, '')
            etiqueta = campo['etiqueta'] + (' *' if campo['obligatorio'] else '')
            with ui.row().classes('w-full items-center gap-2 no-wrap'):
                control = _control_de_campo(campo, etiqueta, valores, clave)
                control.classes('flex-1').props('outlined dense')
                if campo['descripcion']:
                    ayuda(campo['descripcion'])

    def _control_de_campo(campo, etiqueta, valores, clave):
        tipo = (campo['tipo'] or 'TEXTO').upper()
        if campo['opciones'] or tipo == 'LISTA':
            # Una lista sin opciones definidas sería un desplegable vacío: se
            # deja escribir, que es mejor que un campo que no admite nada.
            return ui.select(campo['opciones'], label=etiqueta, with_input=True,
                             new_value_mode='add-unique').bind_value(valores, clave)
        if tipo == 'BOOLEANO':
            return ui.select({'': '—', 'Sí': 'Sí', 'No': 'No'}, label=etiqueta) \
                .bind_value(valores, clave)
        if tipo == 'FECHA':
            return ui.input(etiqueta).props('type=date').bind_value(valores, clave)
        if tipo in ('NUMERO', 'MONEDA'):
            return ui.input(etiqueta).props('type=number').bind_value(valores, clave)
        if tipo == 'HORA_MILITAR':
            return ui.input(etiqueta).props('type=time').bind_value(valores, clave)
        if tipo == 'TEXTO_LARGO':
            return ui.textarea(etiqueta).bind_value(valores, clave)
        return ui.input(etiqueta).bind_value(valores, clave)

    # --- ALTA DE UN CAMPO SIN SALIR DEL FORMULARIO ---
    nuevo_campo = {'etiqueta': '', 'tipo': 'TEXTO', 'opciones': '',
                   'obligatorio': False, 'descripcion': '', 'ambito': AMBITO_FICHA}

    TIPOS_DE_CAMPO = ['TEXTO', 'TEXTO_LARGO', 'NUMERO', 'FECHA', 'BOOLEANO',
                      'LISTA', 'HORA_MILITAR', 'MONEDA']

    def abrir_alta_campo(ambito: str):
        nuevo_campo.update({'etiqueta': '', 'tipo': 'TEXTO', 'opciones': '',
                            'obligatorio': False, 'descripcion': '',
                            'ambito': ambito})
        dialogo_campo.open()

    def guardar_campo():
        etiqueta = (nuevo_campo['etiqueta'] or '').strip()
        if not etiqueta:
            ui.notify('El nombre del campo es obligatorio', type='warning')
            return
        interno = _nombre_interno(etiqueta)
        with get_db_session() as db:
            if db.query(MetaVariable).filter_by(nombre_interno=interno).first():
                ui.notify(f'Ya existe un campo llamado «{etiqueta}»', type='warning')
                return
            db.add(MetaVariable(
                nombre_interno=interno, etiqueta_visual=etiqueta,
                tipo_dato=nuevo_campo['tipo'], ambito=nuevo_campo['ambito'],
                opciones=(nuevo_campo['opciones'] or '').strip() or None,
                es_obligatorio=bool(nuevo_campo['obligatorio']),
                descripcion=(nuevo_campo['descripcion'] or '').strip() or None,
                activo=True, orden=0))
            db.commit()
        ui.notify(f'Campo «{etiqueta}» creado', type='positive')
        dialogo_campo.close()
        render_ui()

    with ui.dialog() as dialogo_campo, ui.card().classes('w-[480px] max-w-full'):
        ui.label('Nuevo campo').classes('text-lg font-bold text-slate-800')
        ui.label('El campo queda disponible para TODOS los informes, no solo para '
                 'este. Es una manera de clasificar, no un dato suelto.') \
            .classes('text-xs text-slate-500')
        ui.input('Nombre del campo *').bind_value(nuevo_campo, 'etiqueta') \
            .classes('w-full').mark('nombre-campo')
        ui.select(TIPOS_DE_CAMPO, label='Tipo de dato') \
            .bind_value(nuevo_campo, 'tipo').classes('w-full')
        campo_opciones = ui.textarea('Opciones (una por línea)') \
            .bind_value(nuevo_campo, 'opciones').classes('w-full')
        campo_opciones.bind_visibility_from(
            nuevo_campo, 'tipo', backward=lambda v: v == 'LISTA')
        ui.input('Ayuda para quien lo rellene') \
            .bind_value(nuevo_campo, 'descripcion').classes('w-full')
        ui.switch('Obligatorio').bind_value(nuevo_campo, 'obligatorio')
        with ui.row().classes('w-full justify-end mt-3 gap-2'):
            ui.button('Cancelar', on_click=dialogo_campo.close).props('flat')
            ui.button('Crear campo', icon='check', on_click=guardar_campo) \
                .classes('bg-slate-900 text-white')

    # --- ALTA DE UNA PARTE SIN SALIR DEL FORMULARIO ---
    #
    # Si «Solicitado por» y «Destinatarios» son obligatorios, tiene que poder
    # crearse una parte desde aquí. Mandar a alguien a otra pantalla a mitad de
    # un formulario largo le hace perder lo que llevaba escrito — y en una
    # instalación recién hecha no hay ninguna parte, así que el primer informe
    # sería imposible de guardar.
    nueva_parte = {'nombre': '', 'tipo': 'Entidad', 'contacto_nombre': '',
                   'email': '', 'destino': 'solicitantes'}

    #: Tipos sugeridos, no cerrados: el campo admite escribir otro. Un catálogo
    #: fijo aquí decidiría por el cliente qué clase de partes puede tener.
    TIPOS_DE_PARTE = ['Entidad', 'Ente de control', 'Contratista', 'Proveedor',
                      'Ciudadano', 'Área interna', 'Otro']

    def abrir_alta_parte(destino: str):
        nueva_parte.update({'nombre': '', 'tipo': 'Entidad',
                            'contacto_nombre': '', 'email': '',
                            'destino': destino})
        dialogo_parte.open()

    def guardar_parte():
        nombre = (nueva_parte['nombre'] or '').strip()
        if not nombre:
            ui.notify('El nombre de la parte es obligatorio', type='warning')
            return
        with get_db_session() as db:
            ya = db.query(Stakeholder).filter(Stakeholder.nombre == nombre).first()
            if ya:
                # No se crea una segunda: se usa la que hay. Dos partes con el
                # mismo nombre son indistinguibles en cualquier informe futuro.
                ident = ya.id
                ui.notify(f'«{nombre}» ya existía; se usa esa.', type='info')
            else:
                parte = Stakeholder(nombre=nombre,
                                    tipo=(nueva_parte['tipo'] or '').strip() or None,
                                    contacto_nombre=(nueva_parte['contacto_nombre']
                                                     or '').strip() or None,
                                    email=(nueva_parte['email'] or '').strip() or None)
                db.add(parte)
                db.commit()
                ident = parte.id
                ui.notify(f'Parte «{nombre}» creada', type='positive')
        clave = f'{PREFIJO_EXTERNO}:{ident}'
        lista = nuevo_informe[nueva_parte['destino']]
        if clave not in lista:
            lista.append(clave)
        dialogo_parte.close()
        render_ui()

    with ui.dialog() as dialogo_parte, ui.card().classes('w-[460px] max-w-full'):
        ui.label('Nueva parte del flujo').classes('text-lg font-bold text-slate-800')
        ui.label('Un tercero externo: la entidad que exige el informe, la que lo '
                 'recibe, un ente de control. Las partes internas son los '
                 'usuarios del sistema y se crean en «Gestor de Usuarios».') \
            .classes('text-xs text-slate-500')
        ui.input('Nombre *').bind_value(nueva_parte, 'nombre').mark('nombre-parte').classes('w-full')
        ui.select(TIPOS_DE_PARTE, label='Tipo', new_value_mode='add-unique',
                  with_input=True).bind_value(nueva_parte, 'tipo').classes('w-full') \
            .tooltip('Puede escribir uno que no esté en la lista.')
        ui.input('Persona de contacto').bind_value(nueva_parte, 'contacto_nombre') \
            .classes('w-full')
        ui.input('Correo').bind_value(nueva_parte, 'email').classes('w-full')
        with ui.row().classes('w-full justify-end mt-3 gap-2'):
            ui.button('Cancelar', on_click=dialogo_parte.close).props('flat')
            ui.button('Crear y añadir', icon='check', on_click=guardar_parte) \
                .classes('bg-slate-900 text-white')

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
                    ui.button('Nuevo', icon='add', on_click=ir_a_formulario_nuevo).mark('nuevo-informe').classes('bg-slate-900 text-white')

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
                sh_db = db.query(Stakeholder).all()
                
                opciones_users = {str(u.id): u.nombre for u in usuarios}
                # Las partes del flujo: internas y externas en la misma lista.
                opciones_partes = opciones_de_partes(usuarios, sh_db)

            with ui.row().classes('w-full gap-6'):
                # COL 1
                with ui.card().classes('w-1/2 p-4'):
                    ui.label('Datos Básicos').classes('text-lg font-bold text-blue-800')
                    ui.input('Nombre').bind_value(nuevo_informe, 'nombre').mark('nombre-informe').classes('w-full')
                    ui.textarea('Descripción').bind_value(nuevo_informe, 'descripcion').classes('w-full')

                    ui.label('Formatos').classes('mt-2 text-sm font-bold')
                    ui.select(['pdf', 'xlsx', 'xls', 'docx', 'zip'], multiple=True).bind_value(nuevo_informe, 'extensiones').mark('formatos-informe').classes('w-full')

                    ui.label('Responsables (Usuarios)').classes('mt-2 text-sm font-bold')
                    ui.select(opciones_users, multiple=True).bind_value(nuevo_informe, 'responsables').classes('w-full')

                    # --- LAS PARTES DEL FLUJO ---
                    # Obligatorias las dos: un informe que nadie pide y que no
                    # se le entrega a nadie generaría entregas sin quien las
                    # reciba ni quien reclame si no llegan.
                    ui.separator().classes('my-2')
                    with ui.row().classes('items-center gap-0 no-wrap'):
                        ui.label('Partes del flujo').classes('text-sm font-bold')
                        ayuda('Una parte puede ser INTERNA —un área o una persona '
                              'de la propia entidad— o EXTERNA —un tercero que no '
                              'entra al sistema—. De eso depende el tipo de flujo, '
                              'y por eso el selector marca cuál es cuál.\n\n'
                              'La misma entidad puede ser las dos cosas: el '
                              'ministerio que exige el informe suele ser también '
                              'el que lo recibe.')

                    for etiqueta, clave, pista in (
                            ('Solicitado por *', 'solicitantes',
                             'Quién pide el informe, o qué norma lo exige'),
                            ('Destinatarios (envío) *', 'destinatarios',
                             'A quién se entrega')):
                        ui.label(etiqueta).classes('text-sm font-bold mt-2')
                        with ui.row().classes('w-full items-center gap-2 no-wrap'):
                            ui.select(opciones_partes, multiple=True, with_input=True,
                                      label=pista) \
                                .bind_value(nuevo_informe, clave).classes('flex-1')
                            ui.button(icon='person_add',
                                      on_click=lambda _, k=clave: abrir_alta_parte(k)) \
                                .props('flat dense round color=primary') \
                                .mark(f'alta-parte-{clave}') \
                                .tooltip('Crear una parte nueva sin salir de aquí')
                    if not opciones_partes:
                        # Sin partes en el sistema, los dos campos obligatorios
                        # son imposibles de contestar. Se dice aquí, no al
                        # intentar guardar.
                        ui.label('Todavía no hay partes registradas: cree la '
                                 'primera con el botón de al lado.') \
                            .classes('text-xs text-amber-700')

                # COL 2 (Tiempo)
                with ui.card().classes('w-1/2 p-0 overflow-hidden border-t-4 border-purple-600'):
                    with ui.row().classes('items-center gap-0 no-wrap px-4 pt-3'):
                        ui.label('Cronograma').classes('font-bold text-purple-800 text-sm')
                        ayuda('Define cuándo vence este informe.\n\n'
                              'El camino recomendado es el asistente: se marcan '
                              'las fechas en un calendario y el sistema dice qué '
                              'periodicidad son. Quien crea una obligación tiene '
                              'delante fechas —las de la norma o el contrato—, no '
                              'un catálogo de periodicidades.\n\n'
                              'Las pestañas son el modo manual, y no saben '
                              'expresar todo lo que el motor admite: varios días '
                              'al mes, «el último martes», cada N semanas o cada '
                              'N años solo se pueden definir con el asistente.')

                    # --- El asistente: el calendario pregunta, la lista responde
                    def usar_regla_del_asistente(regla, resumen):
                        nuevo_informe['regla'] = regla
                        nuevo_informe['regla_resumen'] = resumen
                        # La vigencia del cronograma manda sobre la del
                        # formulario: son el mismo dato y tener dos sería
                        # garantizar que uno esté mal.
                        if regla.get('fecha_inicio'):
                            nuevo_informe['fecha_inicio'] = regla['fecha_inicio']
                        ui.notify('Cronograma definido', type='positive')
                        render_ui()

                    def quitar_regla():
                        nuevo_informe['regla'] = None
                        nuevo_informe['regla_resumen'] = ''
                        render_ui()

                    with ui.column().classes('w-full px-4 pt-3 gap-2'):
                        if nuevo_informe.get('regla'):
                            with ui.row().classes('w-full items-start gap-2 p-3 no-wrap '
                                                  'bg-emerald-50 border border-emerald-200 rounded'):
                                ui.icon('event_available').classes('text-emerald-600 mt-0.5')
                                with ui.column().classes('gap-0 flex-1'):
                                    ui.label(nuevo_informe.get('regla_resumen')
                                             or 'Cronograma definido') \
                                        .classes('text-sm font-bold text-emerald-900')
                                    ui.label('Rige desde '
                                             f"{nuevo_informe['regla'].get('fecha_inicio', '—')}") \
                                        .classes('text-xs text-emerald-700')
                            with ui.row().classes('w-full gap-2'):
                                ui.button('Volver a definirlo', icon='edit_calendar',
                                          on_click=lambda: abrir_asistente(
                                              usar_regla_del_asistente,
                                              nuevo_informe['regla'])).props('outline dense')
                                ui.button('Quitar', icon='delete', on_click=quitar_regla) \
                                    .props('flat dense color=grey')
                        else:
                            ui.button('Definir con el calendario', icon='edit_calendar',
                                      on_click=lambda: abrir_asistente(
                                          usar_regla_del_asistente)) \
                                .classes('w-full bg-purple-700 text-white')
                            ui.label('Se marcan las fechas y el sistema dice qué '
                                     'periodicidad son. O úselo a mano, más abajo.') \
                                .classes('text-xs text-slate-500')

                    with ui.tabs().classes('bg-slate-100 text-slate-600 w-full') as tabs:
                        t_cic = ui.tab('Cíclico'); t_esp = ui.tab('Específico'); t_sem = ui.tab('Semanal'); t_hor = ui.tab('Por Horas'); t_inm = ui.tab('A Demanda')
                    
                    def on_tab(e): 
                        modos = {'Cíclico': 'CICLICO', 'Específico': 'ESPECIFICO', 'Semanal': 'SEMANAL', 'Por Horas': 'HOURLY', 'A Demanda': 'INMEDIATO'}
                        nuevo_informe['modo_tiempo'] = modos.get(e.value, 'CICLICO')
                    tabs.on_value_change(on_tab)
                    
                    curr = nuevo_informe['modo_tiempo']
                    map_tabs = {'CICLICO': 'Cíclico', 'ESPECIFICO': 'Específico', 'SEMANAL': 'Semanal', 'HOURLY': 'Por Horas', 'INMEDIATO': 'A Demanda'}
                    tabs.set_value(map_tabs.get(curr, 'Cíclico'))

                    # --- Vista previa de los vencimientos --------------
                    # Definir la obligación era, hasta ahora, un acto a ciegas:
                    # se elegía periodicidad y ajuste, se guardaba, y las fechas
                    # solo se veían cuando el planificador ya había creado las
                    # tareas. Una regla mal entendida no falla ruidosamente —
                    # genera fechas distintas de las que la persona creía, o
                    # ninguna.
                    #
                    # Esto va ANTES del campo de fecha de inicio, y no es
                    # cosmético: aquel lo pasa como `on_change`, así que el
                    # nombre tiene que existir ya. Estando después, abrir «Nuevo
                    # Informe» reventaba con UnboundLocalError y el formulario
                    # se quedaba a medio dibujar. Lo encontró
                    # `tests/test_asistente_pantalla.py`, que es la primera
                    # prueba que ABRE esta pantalla en vez de importarla.
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

                    # Fecha inicio compartida por todos los modos
                    with ui.row().classes('w-full px-4 pt-4') as fila_fecha_inicio:
                        ui.input('Fecha Inicio', value=nuevo_informe['fecha_inicio'], on_change=refrescar_vista_previa).props('type="date"').bind_value(nuevo_informe, 'fecha_inicio').classes('w-full')

                    with ui.tab_panels(tabs, value=tabs.value).classes('w-full p-4') as paneles:
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
                            # Febrero no tiene día 30. El sistema NO decide por
                            # el administrador: bloquea al guardar y le devuelve
                            # las tres salidas. Esta es una de ellas, y hay que
                            # marcarla a propósito.
                            ui.checkbox('Si el mes no tiene ese día, usar el último',
                                        on_change=refrescar_vista_previa) \
                                .bind_value(nuevo_informe['tiempo_especifico'],
                                            'usar_ultimo_dia_en_meses_cortos') \
                                .tooltip('Con día de corte 29, 30 o 31 hay meses que no lo '
                                         'tienen. Marque esto para que en esos meses el '
                                         'vencimiento sea el último día, o elija otro día '
                                         'de corte.')
                            # «Dentro de los primeros cinco días HÁBILES del mes
                            # siguiente» es de las formas más frecuentes de fijar
                            # un plazo en la norma, y no se puede expresar
                            # moviendo una fecha: si el mes empieza en festivo,
                            # el 5º hábil es el 8, no el 5 corrido al 8.
                            ui.number('… o día HÁBIL del mes (0 = no usar)', min=0, max=23,
                                      format='%.0f', on_change=refrescar_vista_previa) \
                                .bind_value(nuevo_informe['tiempo_especifico'], 'dia_habil') \
                                .classes('w-full') \
                                .tooltip('Cuenta solo días hábiles: «el 5º día hábil del mes». '
                                         'Distinto de poner 5 en el día de corte, que cuenta '
                                         'días corridos. Si lo usa, el día de corte se ignora.')
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
                            # El mismo selector que los otros modos. Tener aquí
                            # solo «siguiente» era una asimetría difícil de
                            # defender: es justo la dirección INSEGURA para un
                            # plazo legal, y era la única disponible.
                            ui.select(AJUSTE_HABIL, label='Si cae en día no hábil',
                                      on_change=refrescar_vista_previa) \
                                .bind_value(nuevo_informe['tiempo_semanal'], 'ajuste') \
                                .classes('w-full').tooltip(AYUDA_AJUSTE)

                        with ui.tab_panel(t_hor):
                            with ui.row().classes('w-full gap-4'):
                                ui.number('Hora inicio (0-23)', min=0, max=23, format='%.0f').bind_value(nuevo_informe['tiempo_horas'], 'hora_inicio').classes('w-1/3')
                                ui.number('Hora fin (0-23)', min=0, max=23, format='%.0f').bind_value(nuevo_informe['tiempo_horas'], 'hora_fin').classes('w-1/3')
                                ui.number('Intervalo (horas)', min=1, max=12, format='%.0f').bind_value(nuevo_informe['tiempo_horas'], 'intervalo').classes('w-1/3')

                        with ui.tab_panel(t_inm):
                            ui.label('Este informe se genera a demanda.').classes('text-gray-500 italic')
                            ui.label('No tiene periodicidad automática. Las entregas se crean manualmente desde "Entregas Manuales".').classes('text-sm text-gray-400')

                    # Con un cronograma del asistente puesto, el modo manual se
                    # esconde entero. Tener las dos vías visibles a la vez sería
                    # dejar en pantalla dos fuentes para la misma obligación, y
                    # quien tocara las pestañas creería estar cambiando algo que
                    # `_construir_regla` ya no mira.
                    if nuevo_informe.get('regla'):
                        tabs.visible = False
                        paneles.visible = False
                        fila_fecha_inicio.visible = False

                    contenedor_vista_previa = ui.column().classes('w-full px-4 pb-3')
                    refrescar_vista_previa()

            # ---- LOS CAMPOS DEL INFORME, DEFINIDOS POR EL ADMINISTRADOR ----
            #
            # Aquí había DOS TARJETAS con nueve campos fijos —subsistema, cargo
            # autorizador, URL de plantilla, justificación normativa…— copiados
            # del Excel de un cliente concreto. Eso convertía el ejemplo en la
            # especificación: otra entidad clasificaba sus informes de otra
            # manera y no tenía dónde ponerlo, y añadir un campo exigía tocar el
            # modelo, la migración y esta pantalla.
            #
            # Ahora los campos son datos. Los de FICHA describen el informe; los
            # de CONTENIDO son datos que la entrega lleva y que se validan al
            # entregarla. Se crean desde aquí mismo.
            ETIQUETAS_FLUJO = {
                'ENVIO_INTERNO': 'Envío Interno',
                'ENVIO_EXTERNO': 'Envío Externo',
                'RECEPCION_EXTERNA': 'Recepción Externa',
                'BIDIRECCIONAL': 'Bidireccional',
            }

            with ui.row().classes('w-full gap-4 mt-4'):
                with ui.card().classes('w-1/2 p-4 border-l-4 border-teal-500'):
                    ui.label('Clasificación').classes('text-lg font-bold text-teal-800')

                    # El tipo de flujo se MUESTRA, no se elige: lo determinan las
                    # partes. Dejarlo elegible permitía marcar «Recepción
                    # Externa» un informe cuyas partes eran todas internas.
                    with ui.row().classes('w-full items-center gap-2'):
                        ui.label('Tipo de Flujo:').classes('text-sm text-gray-600')
                        lbl_flujo = ui.label().classes('text-sm font-bold text-teal-700')
                        ayuda('Se deduce de las partes: si algún destinatario es '
                              'EXTERNO el informe sale de la entidad; si el origen '
                              'es un tercero, entra. Con todas las partes internas '
                              'el flujo es interno.')
                    lbl_flujo.bind_text_from(
                        nuevo_informe, 'destinatarios',
                        backward=lambda _: ETIQUETAS_FLUJO[flujo_derivado(nuevo_informe)])

                    # Conjunto de datos y unidad SIGUEN siendo relaciones y no
                    # campos libres: apuntan a catálogos que se administran en su
                    # propia pantalla, con su propia integridad. Un campo de
                    # texto libre aquí rompería esa relación.
                    with get_db_session() as db2:
                        conjuntos = db2.query(ConjuntoDatos).filter_by(activo=True).order_by(ConjuntoDatos.nombre).all()
                        opciones_conjuntos = {str(c.id): c.nombre for c in conjuntos}
                    opciones_conjuntos[''] = 'Sin asignar'
                    ui.select(label='Conjunto de Datos', options=opciones_conjuntos,
                              with_input=True) \
                        .bind_value(nuevo_informe, 'conjunto_datos_id') \
                        .classes('w-full').props('outlined')

                    with get_db_session() as db3:
                        unidades = db3.query(UnidadOrganizacional).filter_by(activo=True).order_by(UnidadOrganizacional.nombre).all()
                        opts_unidad = {None: '— Sin unidad —'}
                        opts_unidad.update({u.id: u.nombre for u in unidades})
                    ui.select(label='Unidad Responsable', options=opts_unidad) \
                        .bind_value(nuevo_informe, 'unidad_id') \
                        .classes('w-full').props('outlined')

                with ui.card().classes('w-1/2 p-4 border-l-4 border-orange-500'):
                    with ui.row().classes('w-full items-center justify-between no-wrap'):
                        with ui.row().classes('items-center gap-0 no-wrap'):
                            ui.label('Ficha del informe') \
                                .classes('text-lg font-bold text-orange-800')
                            ayuda('Estos campos los define usted en «Campos de los '
                                  'informes». No vienen impuestos: cada entidad '
                                  'clasifica sus informes a su manera.')
                        ui.button(icon='add', on_click=lambda: abrir_alta_campo(AMBITO_FICHA)) \
                            .props('flat dense round color=orange') \
                            .mark('alta-campo-ficha') \
                            .tooltip('Crear un campo nuevo sin salir de aquí')
                    _pintar_campos(AMBITO_FICHA,
                                   'Todavía no hay campos de ficha. Créelos con el '
                                   '«+» y aparecerán aquí para todos los informes.')

            with ui.card().classes('w-full mt-4 p-4 bg-green-50'):
                with ui.row().classes('w-full items-center justify-between no-wrap'):
                    with ui.row().classes('items-center gap-0 no-wrap'):
                        ui.label('Datos que lleva la entrega') \
                            .classes('text-lg font-bold text-green-900')
                        ayuda('Lo que hay DENTRO del informe cuando alguien lo '
                              'entrega: montos, horas de corte, radicados. Se '
                              'validan al recibir la entrega, no ahora.')
                    ui.button(icon='add', on_click=lambda: abrir_alta_campo(AMBITO_CONTENIDO)) \
                        .props('flat dense round color=green') \
                        .mark('alta-campo-contenido') \
                        .tooltip('Crear un campo nuevo sin salir de aquí')
                _pintar_campos(AMBITO_CONTENIDO,
                               'Todavía no hay campos de contenido definidos.')

            ui.button('Guardar', on_click=guardar).classes('mt-6 w-full bg-slate-900 text-white')

    render_ui()