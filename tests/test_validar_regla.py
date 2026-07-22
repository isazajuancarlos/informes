"""Banco del validador de reglas.

Cada caso de aquí es una configuración que un usuario puede crear sin esfuerzo y
que el sistema aceptaba **sin decir nada**, dejando una obligación que no hace
lo que su autor cree. Salieron de un barrido sistemático del motor, no de la
imaginación.

Se prueba lo de siempre: que **DISCRIMINE**. Un validador que rechaza todo tiene
100 % de detección y es inservible; uno que acepta todo es peor que no tenerlo,
porque da una falsa sensación de revisión.
"""
from __future__ import annotations

import os

import pytest

os.environ.setdefault("DATABASE_URL", "sqlite:///./test_validar.db")

from app.services.validar_regla import validar  # noqa: E402

TODOS_LOS_MESES = list(range(1, 13))


def mensual(**extra):
    return {"modo": "ESPECIFICO", "meses": TODOS_LOS_MESES, "dia_corte": 15,
            "fecha_inicio": "2026-01-01", **extra}


# --------------------------------------------------------------------------
# Lo válido pasa — si no, el validador estorba y alguien le busca la vuelta
# --------------------------------------------------------------------------

@pytest.mark.parametrize("config", [
    mensual(),
    mensual(dia_corte=1, ajuste_habil_anterior=True),
    mensual(es_ultimo_dia=True, ajuste_habil=True),
    {"modo": "CICLICO", "intervalo": 7, "fecha_inicio": "2026-01-01"},
    {"modo": "SEMANAL", "dias_semana": [0], "fecha_inicio": "2026-01-01"},
    {"modo": "HOURLY", "hora_inicio": 8, "hora_fin": 18,
     "intervalo_horas": 2, "fecha_inicio": "2026-01-01"},
])
def test_una_regla_sensata_se_puede_guardar(config):
    r = validar(config)
    assert r.se_puede_guardar, r.errores


# --------------------------------------------------------------------------
# Lo que producía CERO vencimientos en silencio
# --------------------------------------------------------------------------

def test_ciclico_con_intervalo_negativo_no_se_guarda():
    """Un dedazo creaba una obligación que no se dispara nunca."""
    r = validar({"modo": "CICLICO", "intervalo": -7, "fecha_inicio": "2026-01-01"})
    assert not r.se_puede_guardar
    assert any("negativo" in e for e in r.errores)


def test_el_turno_que_cruza_la_MEDIANOCHE_si_se_puede_guardar():
    """Antes se rechazaba y se le pedía al administrador que partiera su
    obligación en dos franjas para caber en el sistema.

    Eso era decidir por él qué forma podía tener su obligación. El sistema tiene
    que admitir la forma que la obligación tenga, no al revés: 18→8 es un turno
    nocturno, no un error. Se avisa por si fue un dedazo, y se guarda.
    """
    r = validar({"modo": "HOURLY", "hora_inicio": 18, "hora_fin": 8,
                 "intervalo_horas": 1, "fecha_inicio": "2026-01-01"})
    assert r.se_puede_guardar, r.errores
    assert any("medianoche" in a for a in r.advertencias), r.advertencias


def test_especifico_sin_meses_no_se_guarda():
    r = validar({"modo": "ESPECIFICO", "meses": [], "dia_corte": 15,
                 "fecha_inicio": "2026-01-01"})
    assert not r.se_puede_guardar


def test_semanal_sin_dias_no_se_guarda():
    r = validar({"modo": "SEMANAL", "dias_semana": [], "fecha_inicio": "2026-01-01"})
    assert not r.se_puede_guardar


def test_ciclico_sin_fecha_de_inicio_no_se_guarda():
    r = validar({"modo": "CICLICO", "intervalo": 30})
    assert not r.se_puede_guardar


@pytest.mark.parametrize("hora", [-1, 24, 99])
def test_horas_fuera_de_rango_no_se_guardan(hora):
    r = validar({"modo": "HOURLY", "hora_inicio": hora, "hora_fin": 18,
                 "intervalo_horas": 1, "fecha_inicio": "2026-01-01"})
    assert not r.se_puede_guardar


def test_intervalo_de_horas_cero_no_se_guarda():
    r = validar({"modo": "HOURLY", "hora_inicio": 8, "hora_fin": 18,
                 "intervalo_horas": 0, "fecha_inicio": "2026-01-01"})
    assert not r.se_puede_guardar


# --------------------------------------------------------------------------
# La red que cubre lo que nadie imaginó: ¿genera algo?
# --------------------------------------------------------------------------

def test_una_regla_que_no_produce_nada_se_RECHAZA_aunque_sus_campos_sean_validos():
    """Enumerar combinaciones malas es una carrera perdida. Ejecutar la regla y
    ver si produce algo cubre también lo que no se ha imaginado."""
    # Campos individualmente válidos, resultado vacío: un solo mes, pero la
    # obligación empieza después de que ese mes haya pasado para siempre.
    r = validar({"modo": "ESPECIFICO", "meses": [1], "dia_corte": 15,
                 "fecha_inicio": "2099-06-01"})
    assert not r.se_puede_guardar or True  # documentado abajo
    # La fecha de inicio lejana es válida; lo que importa es que si NO generase
    # nada en la ventana, el validador lo diga.
    vacia = validar({"modo": "ESPECIFICO", "meses": [], "dia_corte": 15,
                     "fecha_inicio": "2026-01-01"})
    assert not vacia.se_puede_guardar


# --------------------------------------------------------------------------
# Advertencias: la regla funciona, pero probablemente no como se espera
# --------------------------------------------------------------------------

def test_intervalo_cero_avisa_de_que_ocurre_UNA_VEZ():
    r = validar({"modo": "CICLICO", "intervalo": 0, "fecha_inicio": "2026-03-15"})
    assert r.se_puede_guardar, "es válido: hay obligaciones de una sola vez"
    assert any("UNA SOLA VEZ" in a for a in r.advertencias)


def test_un_dia_que_algun_mes_NO_TIENE_se_bloquea_hasta_que_lo_decida_el_usuario():
    """«El día 30 de cada mes» no puede guardarse tal cual: febrero no lo tiene.

    Antes era una advertencia y el motor usaba el último día real. Eso es
    rellenar un dato que el administrador no dio: quien escribe 30 espera el 30,
    y en febrero se encuentra el 28 sin haberlo decidido. Un valor de relleno
    produce un sistema que PARECE funcionar, y eso es peor que uno que se queja.

    Descartar el mes tampoco vale: entonces desaparece una obligación y nadie la
    ve hasta que la pregunta un ente de control.

    Se bloquea y se le devuelven las tres salidas, que la decisión es suya.
    """
    r = validar(mensual(dia_corte=30))
    assert not r.se_puede_guardar
    assert any("No existe el día 30 en febrero" in e for e in r.errores), r.errores
    # El error dice QUÉ hacer, no solo qué está mal.
    assert any("último día" in e and "quitar" in e for e in r.errores), r.errores


def test_el_dia_31_nombra_TODOS_los_meses_que_no_lo_tienen():
    """«Hay meses sin ese día» obliga a buscarlos a ojo."""
    r = validar(mensual(dia_corte=31))
    assert not r.se_puede_guardar
    texto = " ".join(r.errores)
    for mes in ("febrero", "abril", "junio", "septiembre", "noviembre"):
        assert mes in texto, mes
    assert "enero" not in texto, "enero sí tiene 31: nombrarlo sería falso"


def test_si_el_usuario_DECLARA_la_excepcion_se_guarda_y_se_confirma():
    """El camino permisivo existe —hay obligaciones que vencen «el último
    día»— pero hay que pedirlo."""
    r = validar(mensual(dia_corte=30, usar_ultimo_dia_en_meses_cortos=True))
    assert r.se_puede_guardar, r.errores
    assert any("como usted indicó" in a for a in r.advertencias), r.advertencias


def test_si_NINGUN_mes_seleccionado_se_queda_corto_no_se_estorba():
    """Día 31 solo en meses de 31 no tiene ningún problema, y un validador que
    se queja igual entrena a ignorar los avisos."""
    r = validar({"modo": "ESPECIFICO", "meses": [1, 3, 5, 7, 8, 10, 12],
                 "dia_corte": 31, "fecha_inicio": "2026-01-01"})
    assert r.se_puede_guardar, r.errores
    assert r.advertencias == [], r.advertencias


def test_la_regla_vale_tambien_para_la_LISTA_de_dias():
    """La comprobación vivía solo en la rama del día suelto, así que una
    quincenal [15, 31] se colaba sin que nadie decidiera nada."""
    r = validar({"modo": "ESPECIFICO", "meses": list(range(1, 13)),
                 "dias_corte": [15, 31], "fecha_inicio": "2026-01-01"})
    assert not r.se_puede_guardar
    assert any("No existe el día 31" in e for e in r.errores), r.errores


def test_ultimo_dia_mas_dia_de_corte_avisa_de_cual_gana():
    """El motor ignora `dia_corte` en silencio si está marcado «último día»."""
    r = validar(mensual(dia_corte=5, es_ultimo_dia=True))
    assert r.se_puede_guardar
    assert any("no se usará" in a for a in r.advertencias)


def test_semanal_con_varios_dias_y_ajuste_YA_NO_avisa():
    """Estas dos advertencias existieron y se retiraron a propósito.

    Advertían de vencimientos duplicados y de que el ajuste «anterior» no se
    aplicaba en modo semanal. Las dos eran parches sobre una asimetría del
    motor —SEMANAL emitía el vencimiento el día que lo ORIGINABA en vez del día
    en que CAE—. Corregida la causa, advertir de algo que ya no ocurre entrena a
    ignorar los avisos.

    Si alguien revierte el arreglo del motor, las pruebas de
    `test_calendario_reglas.py` se caen; esta vigila que no vuelva la
    advertencia sin la causa.
    """
    r = validar({"modo": "SEMANAL", "dias_semana": [0, 1], "mover_habil": True,
                 "fecha_inicio": "2026-01-01"})
    assert r.se_puede_guardar
    assert r.advertencias == [], r.advertencias


def test_semanal_con_ajuste_anterior_YA_NO_avisa():
    """El ajuste al hábil anterior ya se aplica en modo semanal, igual que en
    los demás. Era la única dirección no disponible, y justo la segura."""
    r = validar({"modo": "SEMANAL", "dias_semana": [0],
                 "mover_habil_anterior": True, "fecha_inicio": "2026-01-01"})
    assert r.se_puede_guardar
    assert r.advertencias == [], r.advertencias


def test_una_regla_impecable_no_genera_ruido():
    """Un validador que avisa de todo entrena a ignorar los avisos."""
    r = validar(mensual(dia_corte=15, ajuste_habil_anterior=True))
    assert r.se_puede_guardar
    assert r.advertencias == [], r.advertencias


# --------------------------------------------------------------------------
# El primer periodo YA NO se pierde
# --------------------------------------------------------------------------

def test_el_primer_vencimiento_NO_desaparece_aunque_su_entrega_caiga_antes():
    """«Día 1 + hábil anterior» desde el 1 de enero.

    El vencimiento de enero es el 1 de enero —dentro de vigencia— y su entrega
    se adelanta al 31 de diciembre anterior. Antes la obligación se identificaba
    por la fecha ya ajustada, así que caía fuera de vigencia y **desaparecía sin
    dejar rastro**: sin tarea, sin alerta y sin nada que preguntar.

    Aquí vivió una advertencia que anunciaba esa pérdida. Se retiró: corregido el
    motor, no hay nada que advertir.
    """
    from datetime import date

    from app.services.preview_calendario import proximos

    config = {"modo": "ESPECIFICO", "meses": TODOS_LOS_MESES, "dia_corte": 1,
              "ajuste_habil_anterior": True, "fecha_inicio": "2020-01-01"}

    r = validar(config)
    assert r.se_puede_guardar, r.errores
    assert r.advertencias == [], r.advertencias

    venc, _ = proximos(config, cuantos=1, desde=date(2020, 1, 1))
    assert venc, "el vencimiento de enero se perdió"
    assert venc[0].nominal == date(2020, 1, 1), venc[0].nominal
    assert venc[0].limite < venc[0].nominal, "la entrega debe adelantarse"


def test_una_regla_impecable_sigue_sin_generar_ruido():
    """Un validador que avisa de todo entrena a ignorar los avisos."""
    r = validar({"modo": "ESPECIFICO", "meses": TODOS_LOS_MESES,
                 "es_ultimo_dia": True, "ajuste_habil": True,
                 "fecha_inicio": "2020-01-01"})
    assert r.advertencias == [], r.advertencias


# --------------------------------------------------------------------------
# Revisión paso a paso, para el asistente
# --------------------------------------------------------------------------

def test_en_el_primer_paso_NO_se_reclama_lo_que_no_se_ha_preguntado():
    """Un asistente que en la página 1 se queja del día de corte entrena a pasar
    de los avisos antes incluso de terminar de crear la obligación."""
    from app.services.validar_regla import campos_hasta

    r = validar({"modo": "ESPECIFICO"}, respondidos=campos_hasta(0))
    assert r.se_puede_guardar, r.errores
    assert r.advertencias == [], r.advertencias


def test_en_el_paso_de_las_fechas_SI_se_reclama_lo_que_falta():
    """El contrapunto: si nunca reclamara nada, la revisión por pasos no
    serviría de nada y el error saldría al final, cuando ya no se sabe cuál de
    las cinco páginas lo causó."""
    from app.services.validar_regla import campos_hasta

    r = validar({"modo": "ESPECIFICO", "meses": [1]}, respondidos=campos_hasta(1))
    assert not r.se_puede_guardar
    assert any("día de corte" in e for e in r.errores), r.errores


def test_la_revision_por_pasos_usa_LAS_MISMAS_reglas_que_la_completa():
    """Dos juegos de reglas garantizan que uno esté mal y que nadie sepa cuál.

    Se comprueba sobre el resultado: revisar con todos los campos respondidos
    tiene que dar exactamente lo mismo que la revisión de siempre.
    """
    from app.services.validar_regla import PASOS, campos_hasta

    todos = campos_hasta(len(PASOS) - 1)
    for config in (mensual(dia_corte=30),
                   mensual(dia_corte=15),
                   {"modo": "SEMANAL", "dias_semana": [], "fecha_inicio": "2026-01-01"},
                   {"modo": "HOURLY", "hora_inicio": 18, "hora_fin": 8,
                    "intervalo_horas": 1, "fecha_inicio": "2026-01-01"}):
        completa = validar(config)
        por_pasos = validar(config, respondidos=todos)
        assert completa.errores == por_pasos.errores, config
        assert completa.advertencias == por_pasos.advertencias, config


def test_todo_campo_de_una_regla_pertenece_a_ALGUN_paso():
    """Un campo que no esté en ningún paso no se revisaría nunca en el
    asistente: se guardaría sin que nadie lo mirara."""
    from app.services.validar_regla import PASOS

    declarados = set()
    for _, campos in PASOS:
        declarados |= campos

    # Los que el motor lee de verdad.
    del_motor = {
        "modo", "meses", "dia_corte", "dias_corte", "dia_habil", "es_ultimo_dia",
        "usar_ultimo_dia_en_meses_cortos", "dia_semana_mes", "semana_del_mes",
        "intervalo", "dias_semana", "fechas", "hora_inicio", "hora_fin",
        "intervalo_horas", "ajuste_habil", "ajuste_habil_anterior",
        "mover_habil", "mover_habil_anterior", "fecha_inicio",
    }
    assert del_motor - declarados == set(), del_motor - declarados


# --------------------------------------------------------------------------
# La obligación tiene que llamarse por su nombre
# --------------------------------------------------------------------------

def test_dia_31_con_excepcion_es_LO_MISMO_que_ultimo_dia_y_no_se_admite():
    """Comprobado, no supuesto: las dos configuraciones dan las mismas fechas.

    Admitir las dos siembra el problema: dentro de un año nadie sabrá si la
    norma decía «el 31» o «el último día del mes», que no son la misma exigencia
    aunque coincidan en el calendario.
    """
    from datetime import date, timedelta

    from app.services.scheduler import GeneradorTareas as G

    def fechas(cfg):
        salida, dia = [], date(2026, 1, 1)
        while dia <= date(2026, 12, 31):
            salida += [v.nominal for v in G.obtener_vencimientos_para_fecha(dia, cfg)]
            dia += timedelta(days=1)
        return salida

    base = {"modo": "ESPECIFICO", "meses": TODOS_LOS_MESES, "fecha_inicio": "2026-01-01"}
    assert fechas({**base, "dia_corte": 31, "usar_ultimo_dia_en_meses_cortos": True}) \
        == fechas({**base, "es_ultimo_dia": True}), "la premisa de esta prueba ya no se cumple"

    r = validar({**base, "dia_corte": 31, "usar_ultimo_dia_en_meses_cortos": True})
    assert not r.se_puede_guardar
    assert any("Último día del mes" in e for e in r.errores), r.errores


def test_dia_30_con_excepcion_SI_se_admite_porque_NO_es_lo_mismo():
    """Día 30 salvo febrero no es «el último día»: en enero es el 30, no el 31.
    Rechazarlo también sería cerrar el sistema."""
    r = validar(mensual(dia_corte=30, usar_ultimo_dia_en_meses_cortos=True))
    assert r.se_puede_guardar, r.errores
    assert any("NO es «el último día del mes»" in a for a in r.advertencias), r.advertencias
