"""El asistente recorrido entero, sin abrir un navegador.

Lo que se comprueba aquí no es la pintura: es que **la línea lógica se sostiene**.
Que ninguna página deje pasar algo que la siguiente ya no pueda arreglar, que
nada de lo que se ofrece se rechace después, y que cambiar una respuesta no deje
en pie una elección que ya no corresponde.

Esa última es la que de verdad muerde. Un asistente que conserva estado viejo al
volver atrás produce obligaciones que nadie configuró: el usuario ve una cosa en
pantalla y se guarda otra, y no hay forma de enterarse.
"""
import os
import sys
from datetime import date

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.environ.setdefault("DATABASE_URL", "sqlite:///:memory:")

from app.services.asistente_regla import (  # noqa: E402
    AJUSTES, CONFIRMAR, DETALLE, LECTURA, MARCAR, Asistente,
    banderas_de_ajuste, dias_entre)
from app.services.scheduler import (  # noqa: E402
    AJUSTE_ANTICIPADO, AJUSTE_GRACIA, AJUSTE_OMITIR, GeneradorTareas as G)

LUNES = date(2026, 7, 6)


def hasta_el_final(a: Asistente, indice=0, ajuste="ninguno"):
    """Recorre el asistente entero eligiendo la lectura `indice`."""
    assert a.avanzar(), a.problemas()
    a.elegir(indice)
    assert a.avanzar(), a.problemas()
    a.ajuste = ajuste
    assert a.avanzar(), a.problemas()
    return a


# --------------------------------------------------------------------------
# Página 1: el calendario
# --------------------------------------------------------------------------

def test_sin_marcar_nada_no_se_puede_avanzar_y_se_dice_por_que():
    a = Asistente()
    assert not a.puede_avanzar()
    assert a.problemas(), "tiene que decir qué falta, no solo negarse"
    assert not a.avanzar()
    assert a.paso == MARCAR


def test_marcar_es_un_interruptor():
    a = Asistente()
    a.marcar(LUNES)
    assert a.seleccion == [LUNES]
    a.marcar(LUNES)
    assert a.seleccion == []


def test_un_rango_se_marca_de_una_vez():
    """Marcar día a día una cadena de dos semanas son catorce clics."""
    a = Asistente()
    assert a.marcar_rango(date(2026, 7, 1), date(2026, 7, 5)) == 5
    assert a.seleccion == [date(2026, 7, d) for d in range(1, 6)]
    # Y no duplica lo que ya estaba.
    assert a.marcar_rango(date(2026, 7, 3), date(2026, 7, 7)) == 2
    assert len(a.seleccion) == 7


def test_un_rango_al_reves_no_marca_nada():
    assert dias_entre(date(2026, 7, 5), date(2026, 7, 1)) == []


# --------------------------------------------------------------------------
# Página 2: la lista es la RESPUESTA, no la pregunta
# --------------------------------------------------------------------------

def test_un_solo_dia_marcado_deja_varias_obligaciones_posibles():
    """El fundamento del diseño: marcar no determina.

    Si esta prueba se cayera porque el sistema devuelve una sola lectura, la
    lista sobraría… y el sistema estaría adivinando la obligación.
    """
    a = Asistente()
    a.marcar(LUNES)
    dichos = [L.dice for L in a.lecturas()]
    assert len(dichos) >= 6, dichos
    for esperado in ("todos los días desde esa fecha", "todas las semanas, lunes",
                     "el día 6 de cada mes", "el 1º lunes de cada mes",
                     "cada 6 de julio", "solo esa fecha"):
        assert esperado in dichos, (esperado, dichos)


def test_no_se_avanza_sin_elegir_una_lectura():
    a = Asistente()
    a.marcar(LUNES)
    a.avanzar()
    assert a.paso == LECTURA
    assert not a.puede_avanzar()
    assert "no lo supone" in " ".join(a.problemas())


def test_cambiar_lo_marcado_BORRA_la_lectura_elegida():
    """El defecto silencioso que este asistente no puede permitirse.

    Si al volver atrás y tocar una fecha la elección sobreviviera, la pantalla
    mostraría una lectura que ya no corresponde a lo marcado, y se guardaría una
    obligación que nadie configuró.
    """
    a = Asistente()
    a.marcar(LUNES)
    a.avanzar()
    a.elegir(0)
    assert a.lectura is not None

    a.marcar(date(2026, 8, 6))
    assert a.eleccion is None
    assert a.lectura is None
    assert a.regla() == {}


def test_todas_las_lecturas_CONTIENEN_lo_que_se_marcó():
    """Comprobado ejecutando el motor, no razonando sobre el nombre."""
    seleccion = [date(2026, 3, 2), date(2026, 4, 1), date(2026, 5, 4)]
    a = Asistente(seleccion=list(seleccion))
    assert a.lecturas()
    for L in a.lecturas():
        if L.regla.get("modo") == "HOURLY":
            continue  # su rejilla es horaria: se comprueba aparte
        generadas = set()
        for i in range((max(seleccion) - min(seleccion)).days + 1):
            from datetime import timedelta
            dia = min(seleccion) + timedelta(days=i)
            generadas |= {v.nominal
                          for v in G.obtener_vencimientos_para_fecha(dia, L.regla)}
        assert set(seleccion) <= generadas, (L.dice, sorted(generadas))


def test_varias_veces_al_dia_se_ofrece_cuando_lo_diario_tiene_sentido():
    """Sin esta lectura, un informe por turnos no se puede crear con el
    calendario: el calendario no tiene horas, pero la obligación existe."""
    a = Asistente()
    a.marcar(LUNES)
    claves = [L.periodicidad for L in a.lecturas()]
    assert "VARIAS_AL_DIA" in claves
    assert claves.index("DIARIA") < claves.index("VARIAS_AL_DIA")


# --------------------------------------------------------------------------
# Página 3: el ajuste son CUATRO cosas distintas, no cuatro maneras de mover
# --------------------------------------------------------------------------

@pytest.mark.parametrize("modo,gracia,anticipado", [
    ("ESPECIFICO", "ajuste_habil", "ajuste_habil_anterior"),
    ("SEMANAL", "mover_habil", "mover_habil_anterior"),
    ("CICLICO", "mover_habil", "mover_habil_anterior"),
    ("FECHAS", "ajuste_habil", "ajuste_habil_anterior"),
])
def test_cada_modo_escribe_la_pareja_de_banderas_que_le_toca(modo, gracia, anticipado):
    """El motor lee las dos parejas, pero el formulario clásico solo la suya: una
    regla creada aquí tiene que poder abrirse allí sin perder el ajuste."""
    assert banderas_de_ajuste(modo, "siguiente")[gracia] is True
    assert banderas_de_ajuste(modo, "anterior")[anticipado] is True
    # Las tres claves van SIEMPRE, también la que va en falso.
    for opcion in AJUSTES:
        b = banderas_de_ajuste(modo, opcion)
        assert set(b) == {gracia, anticipado, "omitir_festivos"}


@pytest.mark.parametrize("opcion,esperado", [
    ("anterior", AJUSTE_ANTICIPADO),
    ("siguiente", AJUSTE_GRACIA),
])
def test_la_opcion_elegida_produce_el_ajuste_que_dice(opcion, esperado):
    """MEDIDO sobre el motor. Que la bandera se escriba no prueba que haga algo:
    el ajuste al hábil anterior ya estuvo escrito y sin aplicarse en modo semanal.
    """
    # 1 de enero de 2027: viernes festivo en Colombia.
    a = Asistente(seleccion=[date(2027, 1, 1)])
    a.avanzar()
    indice = next(i for i, L in enumerate(a.lecturas())
                  if L.dice == "cada 1 de enero")
    a.elegir(indice)
    a.avanzar()
    a.ajuste = opcion

    venc = G.obtener_vencimientos_para_fecha(date(2027, 1, 1), a.regla())
    assert len(venc) == 1
    assert venc[0].ajuste == esperado
    assert venc[0].nominal == date(2027, 1, 1), "el día nominal NO se mueve"
    assert venc[0].se_movio


def test_omitir_significa_que_ese_dia_NO_HAY_informe():
    """No se mueve ni antes ni después: no lo hay. Es lo que quiere decir
    «diario en días laborales» — un festivo genera cero informes, no uno que
    haya que entregar otro día."""
    a = Asistente(seleccion=[date(2027, 1, 1)])
    a.avanzar()
    a.elegir(next(i for i, L in enumerate(a.lecturas())
                  if L.dice == "cada 1 de enero"))
    a.avanzar()
    a.ajuste = "omitir"
    assert G.obtener_vencimientos_para_fecha(date(2027, 1, 1), a.regla()) == []

    # Y el mismo informe un año en que el 1 de enero no sea festivo... siempre lo
    # es. Así que se comprueba con uno mensual, que sí tiene meses hábiles.
    b = Asistente(seleccion=[date(2027, 1, 1)])
    b.avanzar()
    b.elegir(next(i for i, L in enumerate(b.lecturas())
                  if L.dice == "el día 1 de cada mes"))
    b.avanzar()
    b.ajuste = "omitir"
    venc = G.obtener_vencimientos_para_fecha(date(2027, 2, 1), b.regla())
    assert len(venc) == 1 and venc[0].ajuste == AJUSTE_OMITIR


def test_cuantos_caen_en_no_habil_se_MIDE_sobre_el_calendario_real():
    """«Elija qué pasa si cae en día no hábil» es una pregunta hipotética;
    «3 de los próximos 24 caen en día no hábil» es la razón de preguntarla."""
    a = Asistente(seleccion=[date(2027, 1, 1)])
    a.avanzar()
    a.elegir(next(i for i, L in enumerate(a.lecturas())
                  if L.dice == "el día 1 de cada mes"))
    caen, total = a.cuantos_caen_en_no_habil(cuantos=12)
    assert total == 12
    assert 0 < caen < 12, caen  # ni todos ni ninguno: el dato tiene que informar

    # Y con la omisión puesta la cuenta NO cambia: se pregunta cuántos días
    # nominales caen en no hábil, y omitirlos los borraría justo del recuento
    # que sirve para decidir si omitirlos.
    a.ajuste = "omitir"
    assert a.cuantos_caen_en_no_habil(cuantos=12) == (caen, total)


# --------------------------------------------------------------------------
# Página 4: la promesa, y que se cumple
# --------------------------------------------------------------------------

def test_lo_que_el_asistente_deja_pasar_al_final_se_puede_guardar():
    """La invariante: el asistente no termina en una regla que el sistema
    rechace. Si `puede_avanzar` dice que sí en la última página, `validar` no
    puede decir que no — son la misma revisión."""
    a = Asistente(seleccion=[LUNES])
    hasta_el_final(a)
    assert a.paso == CONFIRMAR
    if a.puede_avanzar():
        from app.services.validar_regla import validar
        assert validar(a.regla(),
                       periodicidad=a.lectura.periodicidad).se_puede_guardar


@pytest.mark.parametrize("seleccion", [
    [LUNES],
    [date(2026, 7, 6), date(2026, 7, 20)],
    [date(2026, 1, 15), date(2026, 6, 15)],          # meses irregulares
    [date(2026, m, 15) for m in (3, 6, 9, 12)],
    [date(2026, 2, 1), date(2026, 3, 1), date(2026, 4, 1)],
])
def test_NINGUNA_lectura_ofrecida_se_rechaza_al_confirmar(seleccion):
    """Ofrecer algo y negarse a guardarlo después es la peor forma de cerrar:
    el usuario ya vio que el sistema lo entendía."""
    for indice in range(len(Asistente(seleccion=list(seleccion)).lecturas())):
        a = Asistente(seleccion=list(seleccion))
        hasta_el_final(a, indice)
        assert a.puede_avanzar(), (a.lectura.nombre, a.lectura.dice,
                                   a.problemas())


def test_la_proyeccion_sale_del_MOTOR_DE_PRODUCCION():
    """Una vista previa con su propia copia de la lógica mostraría fechas que el
    sistema luego no genera, y nadie sabría cuál de las dos está mal."""
    a = Asistente(seleccion=[LUNES])
    a.avanzar()
    a.elegir(next(i for i, L in enumerate(a.lecturas())
                  if L.dice == "el día 6 de cada mes"))
    a.avanzar()
    a.avanzar()
    venc, aviso = a.proyeccion(cuantos=12)
    assert aviso == "", aviso
    assert [v.nominal.day for v in venc] == [6] * 12
    assert venc[0].nominal == LUNES


def test_la_vigencia_por_defecto_es_la_primera_fecha_marcada():
    a = Asistente(seleccion=[date(2026, 9, 3), date(2026, 7, 6)])
    assert a.vigencia_efectiva() == LUNES
    a.vigencia = date(2026, 1, 1)
    a.avanzar()
    a.elegir(0)
    assert a.regla()["fecha_inicio"] == "2026-01-01"


# --------------------------------------------------------------------------
# La línea lógica: cada página revisa lo suyo y NADA MÁS
# --------------------------------------------------------------------------

def test_una_pagina_no_se_queja_de_lo_que_todavia_no_ha_preguntado():
    """Acusar a alguien de no haber contestado algo que nadie le ha preguntado
    entrena a saltarse los avisos antes incluso de terminar."""
    a = Asistente(seleccion=[LUNES])
    a.avanzar()
    a.elegir(0)
    a.paso = LECTURA
    revision = a.revisar()
    assert not any("hábil" in e for e in revision.errores), revision.errores
    assert not any("fecha de inicio" in e for e in revision.errores)


def test_por_horas_pregunta_la_FRANJA_y_no_el_ajuste():
    """Una franja horaria no se traslada a otro día: ofrecer el ajuste sería dar
    una opción que no hace nada."""
    a = Asistente(seleccion=[LUNES])
    a.avanzar()
    a.elegir(next(i for i, L in enumerate(a.lecturas())
                  if L.periodicidad == "VARIAS_AL_DIA"))
    assert a.es_por_horas
    a.avanzar()
    assert a.paso == DETALLE
    a.franja = {"hora_inicio": 6, "hora_fin": 22, "intervalo_horas": 4}
    regla = a.regla()
    assert regla["modo"] == "HOURLY" and regla["hora_inicio"] == 6
    for bandera in ("ajuste_habil", "mover_habil", "omitir_festivos"):
        assert bandera not in regla, bandera
    horas = [v.hora for v in G.obtener_vencimientos_para_fecha(LUNES, regla)]
    assert horas == [6, 10, 14, 18, 22]


def test_un_turno_que_cruza_la_medianoche_NO_se_rechaza():
    """El sistema admite la forma que tenga la obligación, no al revés."""
    a = Asistente(seleccion=[LUNES])
    a.avanzar()
    a.elegir(next(i for i, L in enumerate(a.lecturas())
                  if L.periodicidad == "VARIAS_AL_DIA"))
    a.avanzar()
    a.franja = {"hora_inicio": 18, "hora_fin": 8, "intervalo_horas": 2}
    a.avanzar()
    assert a.paso == CONFIRMAR
    assert a.puede_avanzar(), a.problemas()
    assert a.revisar().advertencias, "cruzar la medianoche merece un aviso"


def test_el_resumen_dice_la_periodicidad_Y_el_ajuste():
    a = Asistente(seleccion=[date(2027, 1, 1)])
    a.avanzar()
    a.elegir(next(i for i, L in enumerate(a.lecturas())
                  if L.dice == "cada 1 de enero"))
    a.avanzar()
    a.ajuste = "anterior"
    resumen = a.resumen()
    assert "Anual" in resumen and "1 de enero" in resumen
    assert "hábil anterior" in resumen
