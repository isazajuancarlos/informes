"""Del calendario a la regla: se marcan fechas y el sistema dice qué son.

Es la dirección contraria a todo lo demás, y para este dominio la buena: quien
crea la obligación TIENE las fechas —están en la norma— y lo que no tiene es el
nombre de la abstracción.

Lo que estas pruebas vigilan es que no se convierta en un adivino. Un patrón mal
inferido es peor que ninguno, porque el usuario lo da por bueno: ya vio que el
sistema «lo entendió».
"""
from __future__ import annotations

import os
from datetime import date, timedelta

import pytest

os.environ.setdefault("DATABASE_URL", "sqlite:///./test_inferir.db")

from app.services.inferir_periodicidad import inferir  # noqa: E402
from app.services.scheduler import GeneradorTareas as G  # noqa: E402

TODOS = list(range(1, 13))
I = {"fecha_inicio": "2026-01-01"}


def marcar(regla, a1=date(2026, 1, 1), a2=date(2026, 12, 31)):
    """Las fechas que el usuario habría marcado en el calendario."""
    salida, dia = [], a1
    while dia <= a2:
        salida += [v.nominal for v in G.obtener_vencimientos_para_fecha(dia, regla)]
        dia += timedelta(days=1)
    return salida


REGLAS = [
    ("el día 15 de cada mes", "MENSUAL", "el día 15",
     {"modo": "ESPECIFICO", "meses": TODOS, "dia_corte": 15, **I}),
    ("el último día del mes", "MENSUAL", "último día",
     {"modo": "ESPECIFICO", "meses": TODOS, "es_ultimo_dia": True, **I}),
    ("el 5º día hábil", "MENSUAL", "día hábil",
     {"modo": "ESPECIFICO", "meses": TODOS, "dia_habil": 5, **I}),
    ("el último martes", "MENSUAL", "último martes",
     {"modo": "ESPECIFICO", "meses": TODOS, "dia_semana_mes": 1,
      "semana_del_mes": -1, **I}),
    ("los días 7 y 20", "MENSUAL", "7, 20",
     {"modo": "ESPECIFICO", "meses": TODOS, "dias_corte": [7, 20], **I}),
    ("trimestral el 15", "TRIMESTRAL", "el día 15",
     {"modo": "ESPECIFICO", "meses": [3, 6, 9, 12], "dia_corte": 15, **I}),
    ("todos los lunes", "SEMANAL", "lunes",
     {"modo": "SEMANAL", "dias_semana": [0], **I}),
    ("cada dos martes", "CADA_N_SEMANAS", "cada 2 semanas",
     {"modo": "SEMANAL", "dias_semana": [1], "cada_n_semanas": 2, **I}),
]


@pytest.mark.parametrize("nombre,esperada,trozo,regla", REGLAS)
def test_el_sistema_RECONOCE_lo_que_se_marcó(nombre, esperada, trozo, regla):
    inf = inferir(marcar(regla))
    assert inf.periodicidad == esperada, f"{nombre}: dijo {inf.explicacion}"
    assert trozo in inf.explicacion, f"{nombre}: {inf.explicacion}"


@pytest.mark.parametrize("nombre,_e,_t,regla", REGLAS)
def test_la_regla_inferida_REPRODUCE_exactamente_lo_marcado(nombre, _e, _t, regla):
    """Es lo que separa esto de un adivino. Si la regla inferida generara una
    fecha que el usuario no marcó, o se saltara una que sí, el sistema estaría
    cambiando la obligación mientras dice que la entendió."""
    marcadas = marcar(regla)
    inf = inferir(marcadas)
    reproducidas = marcar(inf.regla, min(marcadas), max(marcadas))
    assert sorted(set(reproducidas)) == sorted(set(marcadas)), nombre


# --------------------------------------------------------------------------
# Sobreajuste: el defecto propio de inferir
# --------------------------------------------------------------------------

def test_UNA_sola_fecha_no_autoriza_a_inventar_un_patron():
    """El 15 de marzo de 2026 es domingo y es el tercero del mes, así que «el 3º
    domingo de cada mes» lo explica perfectamente — y es una invención.

    Con una observación, casi cualquier patrón encaja. Se devuelve la lectura
    LITERAL, que es lo único que consta.
    """
    inf = inferir([date(2026, 3, 15)])
    assert inf.periodicidad == "ANUAL"
    assert "día 15" in inf.explicacion, inf.explicacion
    assert "domingo" not in inf.explicacion, inf.explicacion


def test_DOS_fechas_tampoco():
    inf = inferir([date(2026, 3, 15), date(2026, 9, 15)])
    assert "día 15" in inf.explicacion, inf.explicacion
    assert "domingo" not in inf.explicacion and "hábil" not in inf.explicacion


def test_con_TRES_meses_ya_se_puede_afirmar_un_patron_derivado():
    """El contrapunto: si nunca infiriera nada derivado, la función no serviría.
    Tres meses distintos bastan, y el patrón se verifica ejecutándolo."""
    regla = {"modo": "ESPECIFICO", "meses": [1, 2, 3], "dia_habil": 5, **I}
    inf = inferir(marcar(regla, date(2026, 1, 1), date(2026, 3, 31)))
    assert "día hábil" in inf.explicacion, inf.explicacion


# --------------------------------------------------------------------------
# Cuando no hay patrón, se dice — no se fuerza uno
# --------------------------------------------------------------------------

def test_unas_fechas_SIN_patron_quedan_como_fechas_sueltas():
    sueltas = [date(2026, 3, 15), date(2026, 7, 4), date(2026, 11, 2)]
    inf = inferir(sueltas)
    assert inf.periodicidad == "SIN_PATRON"
    assert inf.regla["modo"] == "FECHAS"
    assert len(inf.regla["fechas"]) == 3


def test_una_fecha_DESCOLGADA_impide_afirmar_el_patron():
    """Once del día 15 y una del 17 no son «mensual el 15»: si el sistema lo
    dijera, generaría un vencimiento el 15 de ese mes —que nadie marcó— y se
    saltaría el 17 —que sí—. Cambiar la obligación mientras se dice que se
    entendió es el peor resultado posible."""
    marcadas = marcar({"modo": "ESPECIFICO", "meses": TODOS, "dia_corte": 15, **I})
    rotas = [f if f.month != 6 else date(2026, 6, 17) for f in marcadas]
    inf = inferir(rotas)
    assert inf.periodicidad == "SIN_PATRON", inf.explicacion
    # Y aun así se registra: ninguna obligación se queda sin poder crearse.
    assert len(inf.regla["fechas"]) == len(rotas)


def test_la_lista_literal_SIEMPRE_reproduce_lo_marcado():
    """Es la salida de emergencia, y tiene que ser exacta por construcción."""
    sueltas = [date(2026, 1, 9), date(2026, 4, 22), date(2026, 8, 3)]
    inf = inferir(sueltas)
    reproducidas = marcar(inf.regla, min(sueltas), max(sueltas))
    assert sorted(set(reproducidas)) == sueltas


def test_sin_fechas_no_se_inventa_nada():
    inf = inferir([])
    assert not inf.exacta
    assert "ninguna fecha" in inf.explicacion
