"""Del calendario a la regla: se marcan fechas y el sistema dice qué son.

# Por qué al revés

Todo lo demás va en la dirección «elijo una periodicidad y veo qué fechas
produce». Para quien crea la obligación suele ser al revés: **las fechas las
tiene** —están en la norma, en el contrato, en la resolución— y lo que no tiene
es el nombre de la abstracción. Pedirle que traduzca «los primeros cinco días
hábiles» a una configuración es pedirle que haga el trabajo del sistema.

Aquí marca las fechas en un calendario y el sistema responde: *«esto es un
informe MENSUAL, el 5º día hábil de cada mes»*.

# La regla que hace esto fiable

Cada patrón candidato se **ejecuta** y se compara con lo que el usuario marcó. Si
no reproduce EXACTAMENTE esas fechas, se descarta. Nada de «se parece a
mensual»: o las genera todas y ninguna de más, o no es.

Sin eso, esto sería un adivino: diría «mensual» ante once fechas y una
descolgada, y el sistema generaría una fecha que el usuario nunca marcó y se
saltaría la que sí. Un patrón mal inferido es peor que ninguno, porque el usuario
lo da por bueno — ya vio que el sistema «lo entendió».

# Cuando no hay patrón

Se devuelve la lista literal (`modo: FECHAS`), que reproduce lo marcado por
construcción. Ninguna obligación se queda sin poder registrarse por no encajar en
una forma conocida.
"""
from __future__ import annotations

import calendar
from dataclasses import dataclass, field
from datetime import date, timedelta


@dataclass
class Inferencia:
    #: La regla lista para guardar.
    regla: dict
    #: Clave del catálogo de periodicidades.
    periodicidad: str
    #: Cómo se le dice al usuario lo que acaba de definir.
    explicacion: str
    #: Si la regla reproduce EXACTAMENTE las fechas marcadas. Solo se devuelve
    #: una inferencia exacta; el campo existe para que quien la consuma no tenga
    #: que fiarse de eso de palabra.
    exacta: bool = True
    #: Patrones que se probaron y no reprodujeron lo marcado. Sirve para
    #: explicar por qué no se reconoció nada, en vez de un «no hay patrón» seco.
    descartados: list[str] = field(default_factory=list)


def _nominales(regla, desde, hasta):
    from app.services.scheduler import GeneradorTareas

    salida, dia = set(), desde
    while dia <= hasta:
        for v in GeneradorTareas.obtener_vencimientos_para_fecha(dia, regla):
            salida.add(v.nominal)
        dia += timedelta(days=1)
    return salida


def _n_esimo_habil_del_mes(f):
    """En qué puesto de días hábiles del mes cae `f`. None si no es hábil."""
    from app.services.calendar_engine import MotorReglasDinamicas

    motor = MotorReglasDinamicas()
    if not motor._es_habil(f):
        return None
    return sum(1 for d in range(1, f.day + 1)
               if motor._es_habil(date(f.year, f.month, d)))


def _ordinal_en_el_mes(f):
    """(ordinal, ordinal_desde_el_final) de ese día de la semana dentro del mes."""
    mismos = [d for d in range(1, calendar.monthrange(f.year, f.month)[1] + 1)
              if date(f.year, f.month, d).weekday() == f.weekday()]
    return mismos.index(f.day) + 1, mismos.index(f.day) - len(mismos)


def _candidatos(fechas):
    """Reglas plausibles, de la más específica a la más genérica.

    El orden importa: «el último martes» y «el día 26» pueden coincidir en un
    mes concreto, y se prefiere la que explica el patrón —la que seguirá siendo
    cierta el año que viene— sobre la coincidencia numérica.
    """
    inicio = min(fechas)
    meses = sorted({f.month for f in fechas})
    dias = sorted({f.day for f in fechas})
    semanas = sorted({f.weekday() for f in fechas})
    base = {"fecha_inicio": inicio.isoformat()}
    salida = []

    # --- patrones de día de la semana ---
    if len(fechas) >= 3:
        separaciones = {(b - a).days for a, b in zip(fechas, fechas[1:])}
        if separaciones and separaciones <= {7}:
            salida.append(("SEMANAL",
                           {"modo": "SEMANAL", "dias_semana": semanas, **base},
                           f"todas las semanas, {_dias_es(semanas)}"))
        for n in (2, 3, 4):
            if separaciones and separaciones <= {7 * n}:
                salida.append(("CADA_N_SEMANAS",
                               {"modo": "SEMANAL", "dias_semana": semanas,
                                "cada_n_semanas": n, **base},
                               f"cada {n} semanas, {_dias_es(semanas)}"))
        if len(semanas) > 1 and separaciones and max(separaciones) <= 7:
            salida.append(("SEMANAL",
                           {"modo": "SEMANAL", "dias_semana": semanas, **base},
                           f"todas las semanas, {_dias_es(semanas)}"))

    # --- patrones mensuales, en orden de qué EXPLICA mejor ---
    #
    # Los patrones DERIVADOS —n-ésimo día hábil, último martes, último día del
    # mes— necesitan varias observaciones. Con una sola fecha marcada casi
    # cualquiera la explica: el 15 de marzo de 2026 es domingo y es el tercero
    # del mes, así que «el 3º domingo de cada mes» encaja perfectamente y es una
    # invención. Sobreajuste: el patrón se ajusta al dato en vez de explicarlo.
    #
    # Con pocos datos se prefiere la lectura LITERAL —«el día 15»— que es lo que
    # el usuario marcó y lo único que consta.
    suficientes = len({(f.year, f.month) for f in fechas}) >= 3

    habiles = {_n_esimo_habil_del_mes(f) for f in fechas}
    if suficientes and len(habiles) == 1 and None not in habiles:
        n = habiles.pop()
        salida.append((_por_meses(meses),
                       {"modo": "ESPECIFICO", "meses": meses, "dia_habil": n, **base},
                       f"el {n}º día hábil de cada mes"))

    ordinales = {_ordinal_en_el_mes(f)[0] for f in fechas}
    desde_el_final = {_ordinal_en_el_mes(f)[1] for f in fechas}
    if suficientes and len(semanas) == 1 and len(desde_el_final) == 1 \
            and desde_el_final == {-1}:
        salida.append((_por_meses(meses),
                       {"modo": "ESPECIFICO", "meses": meses,
                        "dia_semana_mes": semanas[0], "semana_del_mes": -1, **base},
                       f"el último {_dias_es(semanas)} de cada mes"))
    elif suficientes and len(semanas) == 1 and len(ordinales) == 1:
        n = ordinales.pop()
        salida.append((_por_meses(meses),
                       {"modo": "ESPECIFICO", "meses": meses,
                        "dia_semana_mes": semanas[0], "semana_del_mes": n, **base},
                       f"el {n}º {_dias_es(semanas)} de cada mes"))

    if suficientes and all(f.day == calendar.monthrange(f.year, f.month)[1]
                           for f in fechas):
        salida.append((_por_meses(meses),
                       {"modo": "ESPECIFICO", "meses": meses,
                        "es_ultimo_dia": True, **base},
                       "el último día de cada mes"))

    if len(dias) == 1:
        salida.append((_por_meses(meses),
                       {"modo": "ESPECIFICO", "meses": meses,
                        "dia_corte": dias[0], **base},
                       f"el día {dias[0]}"))
    elif len(dias) <= 4:
        salida.append((_por_meses(meses),
                       {"modo": "ESPECIFICO", "meses": meses,
                        "dias_corte": dias, **base},
                       f"los días {', '.join(str(d) for d in dias)}"))

    # --- ciclo por días corridos ---
    if len(fechas) >= 3:
        separaciones = {(b - a).days for a, b in zip(fechas, fechas[1:])}
        if len(separaciones) == 1:
            n = separaciones.pop()
            salida.append(("CADA_N_DIAS",
                           {"modo": "CICLICO", "intervalo": n, **base},
                           f"cada {n} días"))

    return salida


_MESES_ES = (None, "enero", "febrero", "marzo", "abril", "mayo", "junio",
             "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre")


def _separacion_regular(meses):
    """Cada cuántos meses se repite, si es regular. None si no lo es.

    Deducir la periodicidad del NÚMERO de meses estaba mal: julio y agosto son
    dos meses y no son un semestre. Lo que hace semestral a una obligación es
    que sus meses estén a SEIS de distancia, no que sean dos.
    """
    if len(meses) < 2:
        return None
    saltos = {(b - a) % 12 for a, b in zip(meses, meses[1:] + [meses[0] + 12])}
    return saltos.pop() if len(saltos) == 1 else None


def _periodicidad_de(meses):
    if len(meses) == 12:
        return "MENSUAL"
    if len(meses) == 1:
        return "ANUAL"
    return {6: "SEMESTRAL", 3: "TRIMESTRAL", 2: "BIMESTRAL",
            1: "MENSUAL"}.get(_separacion_regular(meses) or 0, "ANUAL")


def _como_se_lee(meses, dias):
    """La etiqueta EXACTA. En este sistema la etiqueta es el significado."""
    que_dia = (f"el día {dias[0]}" if len(dias) == 1
               else f"los días {', '.join(map(str, dias))}")
    if len(meses) == 1:
        de_mes = _MESES_ES[meses[0]]
        return (f"cada {dias[0]} de {de_mes}" if len(dias) == 1
                else f"cada año, en {de_mes}, {que_dia}")
    sep = _separacion_regular(meses)
    cadencia = {6: "cada semestre", 3: "cada trimestre",
                2: "cada dos meses", 1: "cada mes"}.get(sep)
    if cadencia:
        return f"{cadencia}, {que_dia}"
    # Meses irregulares: se nombran, en vez de inventarles una cadencia.
    nombres = [_MESES_ES[m] for m in meses]
    lista = ", ".join(nombres[:-1]) + " y " + nombres[-1]
    return f"en {lista}, {que_dia}"


def _por_meses(meses):
    return {12: "MENSUAL", 6: "BIMESTRAL", 4: "TRIMESTRAL",
            2: "SEMESTRAL", 1: "ANUAL"}.get(len(meses), "MENSUAL")


_DIAS = ("lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo")


def _dias_es(indices):
    nombres = [_DIAS[i] for i in indices]
    if len(nombres) == 1:
        return nombres[0]
    return ", ".join(nombres[:-1]) + " y " + nombres[-1]


def inferir(fechas) -> Inferencia:
    """Qué periodicidad describe EXACTAMENTE las fechas marcadas.

    Si ninguna lo hace, se devuelve la lista literal. Nunca se devuelve un
    patrón aproximado: un patrón mal inferido es peor que ninguno, porque el
    usuario lo da por bueno.
    """
    fechas = sorted(set(fechas))
    if not fechas:
        return Inferencia({"modo": "FECHAS", "fechas": []}, "SIN_PATRON",
                          "no se marcó ninguna fecha", exacta=False)

    marcadas = set(fechas)
    inicio, fin = fechas[0], fechas[-1]
    descartados = []

    for clave, regla, texto in _candidatos(fechas):
        if _nominales(regla, inicio, fin) == marcadas:
            from app.services.periodicidad import CATALOGO

            nombre = CATALOGO[clave].nombre if clave in CATALOGO else clave
            return Inferencia(regla, clave,
                              f"{nombre}: {texto}", descartados=descartados)
        descartados.append(texto)

    return Inferencia(
        {"modo": "FECHAS", "fechas": [f.isoformat() for f in fechas],
         "fecha_inicio": inicio.isoformat()},
        "SIN_PATRON",
        f"no siguen ningún patrón: quedan como {len(fechas)} fecha(s) sueltas",
        descartados=descartados)


# =========================================================================
# Las LECTURAS de una selección: lo que el usuario ve tras marcar en el
# calendario
# =========================================================================
#
# `inferir` responde «qué patrón es este», y necesita muchas fechas. La pregunta
# de verdad, al marcar en un calendario, es otra: **qué puede ser esto**.
#
# Seleccionar el lunes 6 de julio de 2026 no determina nada, pero acota mucho:
# puede ser diario desde ese día, todos los lunes, el 6 de cada mes, el primer
# lunes de cada mes, cada 6 de julio, o una sola vez. Son seis obligaciones
# distintas y la selección es idéntica en las seis.
#
# Por eso la lista no es un menú que estorba: es la pregunta que la selección
# deja abierta, y que alguien tiene que contestar. El sistema no puede
# adivinarla — y adivinarla sería inventar la obligación.


@dataclass
class Lectura:
    """Una interpretación posible de lo que se marcó."""
    periodicidad: str
    nombre: str
    #: Qué significa, en los términos de quien crea la obligación.
    dice: str
    regla: dict
    #: Las primeras fechas que produciría, para verlo antes de decidir.
    proximas: list = field(default_factory=list)


def _proximas(regla, desde, cuantas=4):
    from app.services.scheduler import GeneradorTareas

    salida, dia, tope = [], desde, desde + timedelta(days=800)
    while dia <= tope and len(salida) < cuantas:
        for v in GeneradorTareas.obtener_vencimientos_para_fecha(dia, regla):
            salida.append(v.nominal)
            if len(salida) >= cuantas:
                break
        dia += timedelta(days=1)
    return salida


def interpretaciones(seleccion) -> list[Lectura]:
    """Qué puede ser lo que el usuario acaba de marcar.

    Solo se ofrecen lecturas que **contienen** la selección: si una
    interpretación no generase alguna de las fechas marcadas, no es una lectura
    de lo que se marcó, es otra cosa. Comprobado ejecutando, no razonado.
    """
    from app.services.periodicidad import CATALOGO

    fechas = sorted(set(seleccion))
    if not fechas:
        return []

    inicio, fin = fechas[0], fechas[-1]
    base = {"fecha_inicio": inicio.isoformat()}
    semanas = sorted({f.weekday() for f in fechas})
    dias = sorted({f.day for f in fechas})
    meses = sorted({f.month for f in fechas})
    candidatas = []

    candidatas.append(("DIARIA", "todos los días desde esa fecha",
                       {"modo": "CICLICO", "intervalo": 1, **base}))
    candidatas.append(("SEMANAL", f"todas las semanas, {_dias_es(semanas)}",
                       {"modo": "SEMANAL", "dias_semana": semanas, **base}))
    candidatas.append(("CADA_N_SEMANAS", f"cada dos semanas, {_dias_es(semanas)}",
                       {"modo": "SEMANAL", "dias_semana": semanas,
                        "cada_n_semanas": 2, **base}))

    if len(dias) == 1:
        candidatas.append(("MENSUAL", f"el día {dias[0]} de cada mes",
                           {"modo": "ESPECIFICO", "meses": list(range(1, 13)),
                            "dia_corte": dias[0], **base}))
    else:
        candidatas.append(("MENSUAL",
                           f"los días {', '.join(map(str, dias))} de cada mes",
                           {"modo": "ESPECIFICO", "meses": list(range(1, 13)),
                            "dias_corte": dias, **base}))

    if len(semanas) == 1:
        ordinal, desde_final = _ordinal_en_el_mes(fechas[0])
        candidatas.append(("MENSUAL",
                           f"el {ordinal}º {_dias_es(semanas)} de cada mes",
                           {"modo": "ESPECIFICO", "meses": list(range(1, 13)),
                            "dia_semana_mes": semanas[0],
                            "semana_del_mes": ordinal, **base}))
        if desde_final == -1:
            candidatas.append(("MENSUAL",
                               f"el último {_dias_es(semanas)} de cada mes",
                               {"modo": "ESPECIFICO", "meses": list(range(1, 13)),
                                "dia_semana_mes": semanas[0],
                                "semana_del_mes": -1, **base}))
        habil = _n_esimo_habil_del_mes(fechas[0])
        if habil:
            candidatas.append(("MENSUAL", f"el {habil}º día hábil de cada mes",
                               {"modo": "ESPECIFICO", "meses": list(range(1, 13)),
                                "dia_habil": habil, **base}))

    if len(meses) < 12:
        candidatas.append((_periodicidad_de(meses),
                           _como_se_lee(meses, dias),
                           {"modo": "ESPECIFICO", "meses": meses,
                            **({"dia_corte": dias[0]} if len(dias) == 1
                               else {"dias_corte": dias}), **base}))

    candidatas.append(("SIN_PATRON",
                       "solo esa fecha" if len(fechas) == 1
                       else f"solo esas {len(fechas)} fechas",
                       {"modo": "FECHAS",
                        "fechas": [f.isoformat() for f in fechas], **base}))

    lecturas, vistas = [], set()
    for clave, dice, regla in candidatas:
        # La lectura tiene que CONTENER lo marcado. Si no generase alguna de las
        # fechas seleccionadas, no es una interpretación de lo que se marcó.
        if not set(fechas) <= _nominales(regla, inicio, fin):
            continue
        if dice in vistas:
            continue
        vistas.add(dice)
        lecturas.append(Lectura(
            periodicidad=clave,
            nombre=CATALOGO[clave].nombre if clave in CATALOGO else clave,
            dice=dice, regla=regla, proximas=_proximas(regla, inicio)))
    return lecturas
