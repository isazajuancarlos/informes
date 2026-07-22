"""La traducción entre el FORMULARIO y la REGLA, que no tenía pruebas.

Entre «el motor calcula bien» y «el producto hace lo correcto» hay un paso que
nadie comprobaba: lo que el formulario de creación arma a partir de lo que el
usuario marca en pantalla. Si esa traducción pierde una bandera, el motor
funciona impecablemente sobre una regla que no es la que se pidió — y la vista
previa muestra lo mismo que se guardará, así que ni siquiera se nota.

Se prueba en las dos direcciones:

  - **Ida**: la opción del selector produce las banderas correctas.
  - **Vuelta**: al reabrir un informe guardado, el selector muestra lo que el
    sistema está haciendo de verdad, no lo que la interfaz preferiría.

La ida y la vuelta tienen que componer: si no, editar un informe sin tocar la
periodicidad la cambiaría sola. Es el defecto más caro de este bloque porque no
deja rastro: nadie edita un informe esperando que cambien sus vencimientos.
"""
from __future__ import annotations

import os
from datetime import date

import pytest

os.environ.setdefault("DATABASE_URL", "sqlite:///./test_form.db")
os.environ.setdefault("DEPLOYMENT_PROFILE", "standalone")

from app.services.preview_calendario import proximos  # noqa: E402
from app.ui.reports_manager import (  # noqa: E402
    AJUSTE_HABIL,
    _ajuste_desde_banderas,
    _banderas_desde_ajuste,
    _construir_regla,
)

OPCIONES = list(AJUSTE_HABIL.keys())


def formulario(modo, **bloques):
    """El estado del formulario, con los mismos valores por defecto que la UI."""
    datos = {
        'modo_tiempo': modo,
        'fecha_inicio': '2026-01-01',
        'tiempo_ciclico': {'anos': 0, 'meses': 0, 'dias': 0, 'total_dias': 0,
                           'ajuste': 'ninguno'},
        'tiempo_especifico': {'meses_seleccionados': [], 'dia_corte': 1,
                              'es_ultimo_dia': False, 'ajuste': 'ninguno'},
        'tiempo_semanal': {'dias_seleccionados': [], 'ajuste': 'ninguno'},
        'tiempo_horas': {'hora_inicio': 8, 'hora_fin': 18, 'intervalo': 1},
    }
    for clave, valores in bloques.items():
        datos[clave].update(valores)
    return datos


# --------------------------------------------------------------------------
# Las tres opciones existen y significan cosas distintas
# --------------------------------------------------------------------------

def test_el_selector_ofrece_exactamente_tres_opciones():
    assert set(OPCIONES) == {"ninguno", "anterior", "siguiente"}


def test_los_textos_NO_prometen_que_el_vencimiento_se_mueve():
    """El día de vencimiento es el que fija la norma. Redactar las opciones como
    «mover la fecha» era lo que hacía creer que la obligación cambiaba de sitio,
    y con ella el periodo al que pertenece."""
    for clave, texto in AJUSTE_HABIL.items():
        assert "Mover" not in texto, f"{clave}: «{texto}»"

    assert "no cambia" in AJUSTE_HABIL["anterior"], AJUSTE_HABIL["anterior"]
    assert "gracia" in AJUSTE_HABIL["siguiente"], AJUSTE_HABIL["siguiente"]


# --------------------------------------------------------------------------
# Ida y vuelta: editar sin tocar nada no puede cambiar la periodicidad
# --------------------------------------------------------------------------

@pytest.mark.parametrize("opcion", OPCIONES)
@pytest.mark.parametrize("siguiente,anterior", [
    ("ajuste_habil", "ajuste_habil_anterior"),
    ("mover_habil", "mover_habil_anterior"),
])
def test_la_ida_y_la_vuelta_componen(opcion, siguiente, anterior):
    """opción -> banderas -> opción tiene que devolver la misma opción.

    Las dos parejas de nombres se prueban porque el modo ESPECÍFICO usa una y
    el CÍCLICO y el SEMANAL la otra. Esa divergencia ya costó que el ajuste al
    hábil anterior no se aplicara nunca en modo semanal.
    """
    banderas = _banderas_desde_ajuste(opcion, siguiente, anterior)
    vuelta = _ajuste_desde_banderas(banderas.get(siguiente), banderas.get(anterior))
    assert vuelta == opcion, f"{opcion} -> {banderas} -> {vuelta}"


def test_las_dos_banderas_a_la_vez_muestran_lo_que_el_motor_HACE():
    """Un informe viejo puede traer las dos. El motor resuelve por su `elif`:
    gana «siguiente». La interfaz tiene que decir eso, no lo que preferiría."""
    assert _ajuste_desde_banderas(True, True) == "siguiente"


# --------------------------------------------------------------------------
# El formulario produce una regla que el motor entiende
# --------------------------------------------------------------------------

@pytest.mark.parametrize("opcion", OPCIONES)
def test_mensual_del_dia_1_llega_al_motor_con_su_ajuste(opcion):
    """El caso que dio nombre a todo esto: informe con fecha fija el día 1.

    Se comprueba de punta a punta —formulario, regla, motor— y con el resultado
    esperado por opción, no solo que «no reviente».
    """
    datos = formulario('ESPECIFICO', tiempo_especifico={
        'meses_seleccionados': [1], 'dia_corte': 1, 'ajuste': opcion})
    regla = _construir_regla(datos)

    venc, aviso = proximos(regla, cuantos=1, desde=date(2026, 1, 1))
    assert venc, f"{opcion}: no generó nada ({aviso})"
    v = venc[0]

    # El día NOMINAL es el 1 de enero en las tres opciones. Eso es lo que
    # significa que el vencimiento no se mueve.
    assert v.nominal == date(2026, 1, 1), f"{opcion}: nominal {v.nominal}"

    if opcion == "ninguno":
        assert v.limite == v.nominal
    elif opcion == "anterior":
        assert v.limite < v.nominal, f"anticipada va hacia atrás: {v.limite}"
    else:
        assert v.limite > v.nominal, f"la gracia va hacia delante: {v.limite}"


@pytest.mark.parametrize("modo,bloque,extra", [
    ('CICLICO', 'tiempo_ciclico', {'total_dias': 30}),
    ('ESPECIFICO', 'tiempo_especifico', {'meses_seleccionados': [3, 6, 9, 12],
                                         'dia_corte': 15}),
    ('SEMANAL', 'tiempo_semanal', {'dias_seleccionados': [0, 3]}),
])
@pytest.mark.parametrize("opcion", ["anterior", "siguiente"])
def test_el_ajuste_llega_en_TODOS_los_modos(modo, bloque, extra, opcion):
    """El ajuste elegido tiene que cambiar algo en los tres modos.

    Si en alguno se perdiera por el camino, el usuario elegiría una cosa y
    obtendría otra sin ningún aviso — que es exactamente lo que pasaba en modo
    semanal con la opción «anterior».
    """
    con = _construir_regla(formulario(modo, **{bloque: {**extra, 'ajuste': opcion}}))
    sin = _construir_regla(formulario(modo, **{bloque: {**extra, 'ajuste': 'ninguno'}}))
    assert con != sin, f"{modo}/{opcion}: el ajuste no llegó a la regla"

    v_con, _ = proximos(con, cuantos=12, desde=date(2026, 1, 1))
    v_sin, _ = proximos(sin, cuantos=12, desde=date(2026, 1, 1))
    assert v_con and v_sin

    # Los días nominales son los mismos: el ajuste no mueve la obligación.
    assert [v.nominal for v in v_con] == [v.nominal for v in v_sin], modo
    # Y alguno tiene que haberse ajustado, o no se estaría probando nada.
    assert any(v.se_ajusto for v in v_con), f"{modo}/{opcion}: no se ajustó ninguno"


def test_horas_no_pierde_la_franja():
    regla = _construir_regla(formulario('HOURLY', tiempo_horas={
        'hora_inicio': 6, 'hora_fin': 22, 'intervalo': 4}))
    assert regla['hora_inicio'] == 6 and regla['hora_fin'] == 22
    assert regla['intervalo_horas'] == 4


def test_la_fecha_de_inicio_llega_en_todos_los_modos():
    """Sin ella el motor no genera nada, y el validador lo rechaza. Perderla en
    la traducción daría un «esta regla no genera ningún vencimiento» que no se
    explica mirando el formulario."""
    for modo in ('CICLICO', 'ESPECIFICO', 'SEMANAL', 'HOURLY'):
        regla = _construir_regla(formulario(modo))
        assert regla.get('fecha_inicio') == '2026-01-01', modo


# --------------------------------------------------------------------------
# La regla del asistente manda, y no se traduce
# --------------------------------------------------------------------------

def test_la_regla_del_asistente_se_guarda_TAL_CUAL():
    """Las pestañas no tienen dónde poner la mitad de lo que el asistente sabe
    expresar. Reconstruirla desde los campos del formulario la mutilaría."""
    from app.ui.reports_manager import _construir_regla

    regla = {'modo': 'ESPECIFICO', 'meses': list(range(1, 13)),
             'dias_corte': [7, 20], 'ajuste_habil_anterior': True,
             'fecha_inicio': '2026-01-01'}
    datos = formulario('CICLICO')          # el formulario dice otra cosa…
    datos['regla'] = regla                 # …y la regla del asistente manda
    assert _construir_regla(datos) == regla


def test_la_regla_del_asistente_no_es_el_MISMO_objeto():
    """Devolver el diccionario guardado dejaría que cualquiera lo mutara desde
    fuera: la obligación cambiaría sin que nadie la editara."""
    from app.ui.reports_manager import _construir_regla

    datos = formulario('CICLICO')
    datos['regla'] = {'modo': 'CICLICO', 'intervalo': 30,
                      'fecha_inicio': '2026-01-01'}
    salida = _construir_regla(datos)
    salida['intervalo'] = 99
    assert datos['regla']['intervalo'] == 30


@pytest.mark.parametrize("regla,excede", [
    ({'modo': 'ESPECIFICO', 'meses': [1], 'dia_corte': 15}, False),
    ({'modo': 'SEMANAL', 'dias_semana': [0]}, False),
    ({'modo': 'CICLICO', 'intervalo': 30}, False),
    ({'modo': 'HOURLY', 'hora_inicio': 8, 'hora_fin': 18}, False),
    ({'modo': 'ESPECIFICO', 'meses': [1], 'dias_corte': [7, 20]}, True),
    ({'modo': 'ESPECIFICO', 'meses': [1], 'dia_semana_mes': 1,
      'semana_del_mes': -1}, True),
    ({'modo': 'ESPECIFICO', 'meses': [3], 'dia_corte': 1, 'cada_n_anios': 2}, True),
    ({'modo': 'SEMANAL', 'dias_semana': [1], 'cada_n_semanas': 2}, True),
    ({'modo': 'SEMANAL', 'dia_habil_semana': 4}, True),
    ({'modo': 'SEMANAL', 'dias_semana': [0], 'omitir_festivos': True}, True),
    ({'modo': 'FECHAS', 'fechas': ['2026-03-15']}, True),
    # Sin fechas dentro tampoco: las pestañas no tienen modo «fechas sueltas»,
    # así que abrirla las convertiría en cíclica en silencio. Una regla rota que
    # se abre y se guarda tiene que salir igual de rota, no distinta.
    ({'modo': 'FECHAS', 'fechas': []}, True),
])
def test_se_reconoce_lo_que_las_pestanas_NO_saben_representar(regla, excede):
    from app.ui.reports_manager import excede_a_las_pestanas

    assert excede_a_las_pestanas(regla) is excede, regla


@pytest.mark.parametrize("regla", [
    {'modo': 'ESPECIFICO', 'meses': list(range(1, 13)), 'dias_corte': [7, 20],
     'fecha_inicio': '2026-01-01'},
    {'modo': 'SEMANAL', 'dias_semana': [1], 'cada_n_semanas': 2,
     'fecha_inicio': '2026-01-01'},
    {'modo': 'ESPECIFICO', 'meses': list(range(1, 13)), 'dia_semana_mes': 1,
     'semana_del_mes': -1, 'fecha_inicio': '2026-01-01'},
])
def test_abrir_y_guardar_sin_tocar_nada_NO_cambia_los_vencimientos(regla):
    """El defecto más caro de este bloque, porque no deja rastro: abrir un
    informe quincenal para corregirle una tilde lo convertía en mensual.

    Nadie edita un informe esperando que cambien sus vencimientos, y una fecha
    que deja de generarse no se nota — el efecto es que no pasa nada.
    """
    from app.ui.reports_manager import _construir_regla, excede_a_las_pestanas

    # Lo que hace `ir_a_editar` al cargar un informe guardado.
    datos = formulario('CICLICO')
    datos['fecha_inicio'] = regla['fecha_inicio']
    if excede_a_las_pestanas(regla):
        datos['regla'] = dict(regla)

    antes, _ = proximos(regla, cuantos=24, desde=date(2026, 1, 1))
    despues, _ = proximos(_construir_regla(datos), cuantos=24,
                          desde=date(2026, 1, 1))
    assert [v.nominal for v in antes] == [v.nominal for v in despues]
    assert [v.ajuste for v in antes] == [v.ajuste for v in despues]


def test_describir_una_regla_guardada_usa_el_VOCABULARIO_del_asistente():
    """La frase que se ve al abrir un informe viejo tiene que ser la misma que
    se vería al crearlo. Dos redacciones acaban diciendo cosas distintas."""
    from app.ui.reports_manager import _describir_regla

    texto = _describir_regla({'modo': 'ESPECIFICO', 'meses': list(range(1, 13)),
                              'dias_corte': [7, 20], 'fecha_inicio': '2026-01-01'})
    assert "Quincenal" in texto and "7, 20" in texto, texto

    texto = _describir_regla({'modo': 'SEMANAL', 'dias_semana': [0],
                              'fecha_inicio': '2026-01-01'})
    assert "lunes" in texto, texto


# --------------------------------------------------------------------------
# Las partes del flujo: obligatorias, y pueden ser internas
# --------------------------------------------------------------------------

class _Parte:
    """Lo mínimo de una `Participacion` para el códec del selector."""
    def __init__(self, usuario_id=None, stakeholder_id=None):
        self.usuario_id, self.stakeholder_id = usuario_id, stakeholder_id


class _Con:
    def __init__(self, ident, nombre, tipo=None):
        self.id, self.nombre, self.tipo = ident, nombre, tipo


def test_una_parte_INTERNA_y_una_externa_no_se_confunden():
    """El id no basta para identificar una parte: el usuario 3 y el tercero 3
    son personas distintas. Sin prefijo, guardar el informe crearía la
    participación de quien no era, y eso no se ve nunca."""
    from app.ui.reports_manager import clave_de_parte, descomponer_parte

    assert clave_de_parte(_Parte(usuario_id=3)) == "u:3"
    assert clave_de_parte(_Parte(stakeholder_id=3)) == "s:3"
    assert descomponer_parte("u:3") == ("u", 3)
    assert descomponer_parte("s:3") == ("s", 3)


@pytest.mark.parametrize("basura", ["", "3", "x:3", "u:", "u:abc", None, "::"])
def test_una_clave_ILEGIBLE_no_se_adivina(basura):
    """Interpretarla como interna crearía la participación de la persona
    equivocada. Se devuelve None y quien llama decide."""
    from app.ui.reports_manager import descomponer_parte

    assert descomponer_parte(basura) is None


def test_el_selector_DICE_cual_es_interna_y_cual_externa():
    """De eso depende el tipo de flujo del informe, así que quien elige tiene
    que verlo sin abrir otra pantalla."""
    from app.ui.reports_manager import opciones_de_partes

    opciones = opciones_de_partes([_Con(1, "Ana")],
                                  [_Con(7, "Ministerio", "Entidad")])
    assert opciones["u:1"] == "Ana · interno"
    assert "externo" in opciones["s:7"] and "Entidad" in opciones["s:7"]


@pytest.mark.parametrize("datos,esperado", [
    ({}, 'ENVIO_INTERNO'),
    ({'destinatarios': ['u:1', 'u:2']}, 'ENVIO_INTERNO'),
    ({'destinatarios': ['s:7']}, 'ENVIO_EXTERNO'),
    ({'destinatarios': ['u:1', 's:7']}, 'ENVIO_EXTERNO'),
    ({'origen_externo': True}, 'RECEPCION_EXTERNA'),
    ({'destinatarios': ['s:7'], 'origen_externo': True}, 'BIDIRECCIONAL'),
])
def test_el_tipo_de_flujo_lo_deciden_las_partes_EXTERNAS(datos, esperado):
    """El matiz «externo» es todo. Aquí se miraba `bool(destinatarios)`, y en
    cuanto el selector admitió partes internas un informe entregado a otra área
    de la propia entidad se habría marcado «Envío Externo» — que es el dato del
    que cuelga todo el análisis de qué información sale de la entidad.

    Medido sobre la base real: 208 de 397 informes son de envío interno.
    """
    from app.ui.reports_manager import flujo_derivado

    assert flujo_derivado(datos) == esperado


def test_un_informe_SIN_PARTES_no_se_puede_guardar():
    """Generaría entregas sin quien las reciba ni quien reclame si no llegan."""
    from app.ui.reports_manager import faltan_partes

    problemas = faltan_partes({})
    assert len(problemas) == 2
    assert any('solicita' in p for p in problemas)
    assert any('entrega' in p for p in problemas)


def test_una_parte_INTERNA_basta_para_cumplir_el_requisito():
    """Exigir partes EXTERNAS haría que 208 de los 397 informes de la base real
    —los de envío interno— no se pudieran registrar. Es la restricción bien
    argumentada que cierra la puerta al caso legítimo."""
    from app.ui.reports_manager import faltan_partes

    assert faltan_partes({'solicitantes': ['u:1'],
                          'destinatarios': ['u:2']}) == []


# --------------------------------------------------------------------------
# Los campos los define el administrador, no el programa
# --------------------------------------------------------------------------

def test_el_nombre_interno_se_DERIVA_del_visible():
    """Pedir dos nombres para lo mismo obliga a inventarse un identificador a
    quien solo quería añadir un campo, y garantiza que con el tiempo digan
    cosas distintas."""
    from app.ui.reports_manager import _nombre_interno

    assert _nombre_interno('Aplicativo de Origen') == 'aplicativo_de_origen'
    assert _nombre_interno('Justificación / Normatividad') == 'justificacion_normatividad'
    assert _nombre_interno('  ¿Requiere acuerdo?  ') == 'requiere_acuerdo'
    assert _nombre_interno('') == 'campo'
    assert len(_nombre_interno('x' * 200)) <= 50


def test_un_campo_OBLIGATORIO_en_blanco_bloquea_el_guardado():
    """`es_obligatorio` estaba declarado desde el principio y no lo miraba
    nadie: quien marcaba un campo obligatorio creía haber acotado algo y no
    había acotado nada. Una bandera que no bloquea promete un control que no
    existe."""
    from app.ui.reports_manager import faltan_campos

    definiciones = [
        {'id': 1, 'etiqueta': 'Subsistema', 'obligatorio': True},
        {'id': 2, 'etiqueta': 'Observación', 'obligatorio': False},
    ]
    assert faltan_campos(definiciones, {}) == ['Subsistema']
    assert faltan_campos(definiciones, {'1': '   '}) == ['Subsistema']
    assert faltan_campos(definiciones, {'1': 'Vigilancia'}) == []
    # Y el opcional en blanco no molesta: un validador que se queja de todo
    # entrena a ignorar los avisos.
    assert faltan_campos(definiciones, {'1': 'Vigilancia', '2': ''}) == []


def test_un_NO_no_es_lo_mismo_que_no_contestado():
    """Al mover los campos viejos, `False` en una casilla es un dato —«no»— y
    no un hueco. Tratarlo como vacío perdería la diferencia entre «no requiere
    acuerdo» y «nadie lo ha contestado»."""
    from migrar_campos_ficha import _texto

    assert _texto(False) == 'No'
    assert _texto(True) == 'Sí'
    assert _texto(None) is None
    assert _texto('   ') is None
    assert _texto('Vigilancia') == 'Vigilancia'


def test_los_campos_de_ficha_MIGRADOS_conservan_su_etiqueta():
    """Quien abra un informe migrado tiene que reconocer sus campos, no
    encontrarse unos nombres nuevos."""
    from migrar_campos_ficha import CAMPOS

    etiquetas = {etiqueta for _, _, etiqueta, _, _ in CAMPOS}
    assert 'Subsistema' in etiquetas
    assert 'Cargo Autorizador' in etiquetas
    assert 'Justificación / Normatividad' in etiquetas
    # Y ninguna columna se queda fuera de la mudanza.
    assert len(CAMPOS) == 7


def test_borrar_un_campo_EN_USO_se_impide_y_se_dice_cuantos():
    """La guarda consultaba `Informe.variables_ids`, un atributo que NO EXISTE:
    borrar un campo con cualquier informe en la base reventaba con
    AttributeError. Una guarda que parece estar y no protege es peor que
    ninguna — a nadie se le ocurre comprobar un guardián ya escrito.

    Vivía dentro de la página como un cierre, así que ninguna prueba podía
    alcanzarla. Por eso ahora está fuera.
    """
    from app.core.database import Base, engine, get_db_session
    from app.models.meta_tables import (AMBITO_FICHA, ConfiguracionInformeVariable,
                                        Informe, MetaVariable)
    from app.ui.variables import puede_borrarse

    Base.metadata.create_all(engine)
    with get_db_session() as db:
        usado = MetaVariable(nombre_interno='campo_en_uso', etiqueta_visual='En uso',
                             tipo_dato='TEXTO', ambito=AMBITO_FICHA)
        libre = MetaVariable(nombre_interno='campo_libre', etiqueta_visual='Libre',
                             tipo_dato='TEXTO', ambito=AMBITO_FICHA)
        # El informe necesita un creador de verdad: `creador_id` es clave
        # foránea, y en una base recién creada no hay ningún usuario.
        from app.models.meta_tables import Usuario
        autor = db.query(Usuario).first()
        if autor is None:
            autor = Usuario(nombre='Prueba', email='prueba@local',
                            password_hash='x', es_admin=True)
            db.add(autor)
            db.flush()
        inf = Informe(nombre='Informe que usa el campo', creador_id=autor.id)
        db.add_all([usado, libre, inf])
        db.flush()
        db.add(ConfiguracionInformeVariable(informe_id=inf.id, variable_id=usado.id,
                                            valor_parametro='Vigilancia'))
        db.commit()
        id_usado, id_libre = usado.id, libre.id

    with get_db_session() as db:
        puede, motivo = puede_borrarse(db, id_usado)
        assert puede is False, motivo
        # Y se dice CUÁNTOS y CUÁL: «no se puede» sin más obliga a buscarlo a ojo.
        assert '1 informe' in motivo, motivo
        assert 'Informe que usa el campo' in motivo, motivo

        # El contrapunto, para que la prueba no pase por prohibirlo todo.
        puede, motivo = puede_borrarse(db, id_libre)
        assert puede is True, motivo
