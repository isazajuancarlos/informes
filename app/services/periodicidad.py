"""Qué preguntar según la periodicidad. La línea lógica del asistente.

# El problema que resuelve

Cada periodicidad necesita datos distintos, y solo esos:

    semanal    → el día de la SEMANA. No una fecha, no un número de día.
    mensual    → el número del día del mes.
    anual      → el día Y el mes.
    varias al día → la franja horaria.

Preguntar de más no es inocuo. Un formulario que muestra «día de corte» junto a
«último día del mes» invita a rellenar los dos, y entonces el motor descarta uno
**en silencio**: la obligación queda creada y no hace lo que su autor cree. Ese es
el defecto que este módulo existe para hacer imposible, y no se arregla avisando
—se arregla no preguntando.

# El vocabulario es el del usuario, no el del motor

El motor tiene cinco modos (`CICLICO`, `ESPECIFICO`, `SEMANAL`, `HOURLY`,
`FECHAS`). Nadie define una obligación pensando en eso: piensa «esto es
trimestral». Que el usuario tenga que saber que «anual» se expresa marcando un
solo mes en `ESPECIFICO` es la implementación asomando por la interfaz.

Aquí se traduce en un solo sitio.

# La tabla está MEDIDA, no escrita de memoria

`CAMPOS_QUE_USA` dice qué lee de verdad cada modo. No se dedujo leyendo el
código: se obtuvo cambiando cada campo y viendo si el resultado cambiaba.
`tests/test_periodicidad.py` repite esa medición y exige que coincida con lo
declarado aquí — si alguien añade un campo al motor y no lo declara, la prueba se
cae. Sin eso, esta tabla envejece y vuelve a preguntar de más.
"""
from __future__ import annotations

#: Qué campos cambian el resultado en cada modo del motor. MEDIDO.
CAMPOS_QUE_USA = {
    "CICLICO": {"intervalo", "ajuste"},
    "ESPECIFICO": {"meses", "cada_n_anios", "dia", "ajuste"},
    "SEMANAL": {"dias_semana", "cada_n_semanas", "ajuste"},
    # El ajuste a día hábil NO aplica: una franja horaria no se traslada a otro
    # día. Preguntarlo aquí sería ofrecer una opción que no hace nada.
    "HOURLY": {"franja_horaria"},
    "FECHAS": {"fechas", "ajuste"},
}

#: Las cuatro formas de decir QUÉ DÍA del mes, dentro de `ESPECIFICO`. Son
#: excluyentes: contestar dos deja que el motor descarte una sin decirlo.
FORMAS_DEL_DIA = {
    "dia_corte": "un día fijo del mes",
    "dias_corte": "varios días fijos del mes",
    "es_ultimo_dia": "el último día del mes",
    "dia_habil": "el n-ésimo día hábil",
    "semana_del_mes": "un día de la semana concreto (p. ej. el último martes)",
}

#: Los campos de la regla, agrupados por la pregunta que los produce.
GRUPOS = {
    "intervalo": {"intervalo"},
    "cada_n_anios": {"cada_n_anios"},
    "meses": {"meses"},
    "dia": set(FORMAS_DEL_DIA) | {"dia_semana_mes", "usar_ultimo_dia_en_meses_cortos"},
    "dias_semana": {"dias_semana", "dia_habil_semana"},
    "cada_n_semanas": {"cada_n_semanas"},
    "fechas": {"fechas"},
    "franja_horaria": {"hora_inicio", "hora_fin", "intervalo_horas"},
    "ajuste": {"ajuste_habil", "ajuste_habil_anterior",
               "mover_habil", "mover_habil_anterior"},
}


class Periodicidad:
    """Una periodicidad tal como la nombra quien crea la obligación."""

    def __init__(self, nombre, modo, pregunta, fija=None, ayuda="",
                 al_anio=None, multiples_dias=False, sinonimos=()):
        self.nombre = nombre
        self.modo = modo
        #: Grupos de `GRUPOS` que hay que preguntar, EN ORDEN.
        self.pregunta = pregunta
        #: Lo que la periodicidad ya decide por sí misma y NO se pregunta.
        #: «Mensual» significa los doce meses; «diaria», intervalo 1. Preguntarlo
        #: sería pedirle al usuario que repita lo que acaba de decir, y abrirle
        #: la puerta a contradecirse.
        self.fija = fija or {}
        self.ayuda = ayuda
        #: Cuántos vencimientos al año PROMETE este nombre. `None` cuando
        #: depende de lo que se conteste (semanal, cada N días, sin patrón).
        #:
        #: El nombre de la periodicidad es una AFIRMACIÓN, y el motor sabe
        #: contar. Sin esto, «anual» con dos fechas se guardaba en silencio y
        #: nadie sabía, al leerlo un año después, si la norma exigía una entrega
        #: o dos.
        self.al_anio = al_anio
        #: Si tiene sentido que ocurra varias veces dentro de su periodo.
        self.multiples_dias = multiples_dias
        #: Cómo la llama la gente, incluidos los términos AMBIGUOS a propósito.
        #: Si la norma del cliente dice «bisemanal» y aquí no aparece, no se
        #: desambigua nada: se esconde, y el usuario adivina. Un término que
        #: lleva a dos periodicidades obliga a elegir, que es lo que hace falta.
        self.sinonimos = tuple(sinonimos)

    def significado(self) -> str:
        """Qué produce esta periodicidad, dicho para una persona.

        Es lo que la identifica de verdad. El NOMBRE no sirve para eso —
        «bisemanal» y «bianual» significan dos cosas cada uno— y por eso el
        nombre no se guarda en ninguna parte: en la base queda la regla, que es
        significado puro.

        Se enseña SIEMPRE, no solo cuando dos nombres colisionan. Que el
        significado aparezca únicamente al desambiguar deja al usuario eligiendo
        «Trimestral» de una lista sin ver que son cuatro entregas al año, y el
        error se descubre un trimestre después.
        """
        propio = _SIGNIFICADO.get(self.clave_en_catalogo())
        if propio:
            return propio
        if self.al_anio == 1:
            return "una al año, en una única fecha"
        if self.al_anio:
            return f"{self.al_anio} al año"
        return "depende de lo que se conteste"

    def clave_en_catalogo(self) -> str:
        for clave, p in CATALOGO.items():
            if p is self:
                return clave
        return ""

    def regla_base(self) -> dict:
        """El punto de partida de la regla: el modo y lo que ya está decidido."""
        return {"modo": self.modo, **self.fija}


#: Qué produce cada periodicidad cuyo número no es fijo. Las de número fijo lo
#: derivan de `al_anio` y no hace falta escribirlas.
_SIGNIFICADO = {
    # Las de número fijo también lo declaran: «12 al año» es cierto pero no dice
    # sobre qué. «12 al año, una por mes» sí, y esa base es lo que el usuario
    # está eligiendo de verdad.
    "MENSUAL": "12 al año — una por mes",
    "QUINCENAL": "24 al año — dos por mes, en los días que indique",
    "BIMESTRAL": "6 al año — una cada dos meses",
    "TRIMESTRAL": "4 al año — una por trimestre",
    "SEMESTRAL": "2 al año — una por semestre",
    "ANUAL": "1 al año — en una única fecha, día y mes",
    "DIARIA": "una cada día",
    "VARIAS_AL_DIA": "varias cada día, en la franja que se indique",
    "SEMANAL": "una por cada día de la semana que marque — 52 al año con uno, "
               "104 con dos",
    "CADA_N_SEMANAS": "una cada N semanas, en el día que elija — 26 al año si "
                      "son dos",
    "CADA_N_DIAS": "una cada N días, contados desde la vigencia",
    "PLURIANUAL": "una cada N años",
    "SIN_PATRON": "exactamente las fechas que escriba",
}

#: El catálogo. Añadir una periodicidad es añadir una línea aquí; no hay que
#: tocar ni el motor ni el validador.
CATALOGO = {
    "DIARIA": Periodicidad(
        "Diaria", "CICLICO", ["ajuste"], fija={"intervalo": 1},
        sinonimos=("diario", "diaria", "a diario", "todos los días"),
        ayuda="Todos los días. Si además ocurre varias veces al día, use "
              "«Varias veces al día»."),
    "VARIAS_AL_DIA": Periodicidad(
        "Varias veces al día", "HOURLY", ["franja_horaria"], al_anio=None, multiples_dias=True,
        sinonimos=("por horas", "varias veces al día", "intradía"),
        ayuda="Se pregunta la franja de horas, no una fecha. El ajuste a día "
              "hábil no aplica: una franja no se traslada a otro día."),
    "SEMANAL": Periodicidad(
        "Semanal", "SEMANAL", ["dias_semana", "ajuste"], multiples_dias=True,
        sinonimos=("semanal", "cada semana", "bisemanal",
                   "dos veces por semana", "2 veces por semana"),
        ayuda="Se pregunta el día de la SEMANA, no un número de día ni una "
              "fecha."),
    "CADA_N_SEMANAS": Periodicidad(
        "Cada varias semanas", "SEMANAL",
        ["cada_n_semanas", "dias_semana", "ajuste"], multiples_dias=True,
        sinonimos=("bisemanal", "cada dos semanas", "cada quince días",
                   "catorcenal", "cada 15 días"),
        ayuda="Cada dos semanas, cada tres. Se elige el DÍA de la semana y el "
              "sistema cuenta desde la vigencia — no hace falta que la fecha de "
              "inicio caiga en ese día.\n\n"
              "Ojo con la palabra «bisemanal»: en español significa tanto «dos "
              "veces por semana» como «cada dos semanas», así que no se usa "
              "aquí. Son 104 y 26 entregas al año: no da igual cuál."),
    "MENSUAL": Periodicidad(
        "Mensual", "ESPECIFICO", ["dia", "ajuste"], al_anio=12, multiples_dias=True,
        sinonimos=("mensual", "cada mes"),
        fija={"meses": list(range(1, 13))},
        ayuda="Todos los meses. Solo se pregunta qué día."),
    "QUINCENAL": Periodicidad(
        "Quincenal", "ESPECIFICO", ["dia", "ajuste"],
        sinonimos=("quincenal", "dos veces al mes", "bimensual", "quincena"),
        fija={"meses": list(range(1, 13))}, al_anio=24, multiples_dias=True,
        ayuda="Dos veces al mes. La multiplicidad de días no es una excepción "
              "aquí: es la definición. Lo normal es «el 15 y el último» o «el 7 "
              "y el 20»."),
    "BIMESTRAL": Periodicidad(
        "Bimestral", "ESPECIFICO", ["meses", "dia", "ajuste"], al_anio=6,
        sinonimos=("bimestral", "cada dos meses"),
        ayuda="Seis entregas al año. Se preguntan los meses y el día."),
    "TRIMESTRAL": Periodicidad(
        "Trimestral", "ESPECIFICO", ["meses", "dia", "ajuste"], al_anio=4,
        sinonimos=("trimestral", "cada trimestre", "cada tres meses"),
        ayuda="Cuatro entregas al año."),
    "SEMESTRAL": Periodicidad(
        "Semestral", "ESPECIFICO", ["meses", "dia", "ajuste"], al_anio=2,
        sinonimos=("semestral", "cada semestre", "dos veces al año"),
        ayuda="Dos entregas al año."),
    "ANUAL": Periodicidad(
        "Anual", "ESPECIFICO", ["meses", "dia", "ajuste"], al_anio=1,
        sinonimos=("anual", "cada año", "una vez al año", "bianual"),
        ayuda="Una vez al año: se pregunta el MES y el DÍA."),
    "PLURIANUAL": Periodicidad(
        "Cada varios años", "ESPECIFICO", ["cada_n_anios", "meses", "dia", "ajuste"],
        sinonimos=("bienal", "bianual", "trienal", "quinquenal", "decenal",
                   "plurianual", "cada varios años"),
        ayuda="Bienal, trienal, un plan quinquenal, un censo decenal. Se cuenta "
              "desde el año de la fecha de inicio. No es lo mismo que «cada 730 "
              "días»: los días se desfasan, los años no."),
    "CADA_N_DIAS": Periodicidad(
        "Cada cierto número de días", "CICLICO", ["intervalo", "ajuste"],
        sinonimos=("cada n días", "cada cierto número de días", "catorcenal"),
        ayuda="Para ciclos que no encajan en el calendario: cada 45 días, cada "
              "tres años. Se cuenta desde la fecha de inicio."),
    "SIN_PATRON": Periodicidad(
        "Fechas sueltas, sin patrón", "FECHAS", ["fechas", "ajuste"],
        sinonimos=("sin patrón", "fechas sueltas", "eventual", "por solicitud"),
        ayuda="Cuando ningún patrón encaja, se escriben las fechas. Ninguna "
              "obligación se queda sin poder registrarse."),
}


def campos_a_preguntar(clave: str) -> list[str]:
    """Los campos de la regla que hay que pedir para esta periodicidad."""
    p = CATALOGO[clave]
    campos = []
    for grupo in p.pregunta:
        campos += sorted(GRUPOS[grupo])
    return campos


def campos_ignorados(config: dict) -> list[str]:
    """Campos con valor que el motor NO va a mirar, dado el modo de la regla.

    Es el defecto en su forma más pura: el dato está puesto, el usuario cree que
    hace algo, y no hace nada. No se detecta nunca por su efecto, porque el
    efecto es que no pasa nada.
    """
    modo = config.get("modo", "CICLICO")
    usados = set()
    for grupo in CAMPOS_QUE_USA.get(modo, set()):
        usados |= GRUPOS.get(grupo, set())

    ignorados = []
    for grupo, campos in GRUPOS.items():
        if grupo in CAMPOS_QUE_USA.get(modo, set()):
            continue
        for campo in sorted(campos):
            if config.get(campo):
                ignorados.append(campo)
    return ignorados


def formas_del_dia_contestadas(config: dict) -> list[str]:
    """Cuáles de las formas excluyentes de decir «qué día» vienen contestadas."""
    return [c for c in FORMAS_DEL_DIA if config.get(c)]


def buscar(termino: str) -> list[str]:
    """Las periodicidades que responden a ese término. Puede devolver VARIAS.

    Que devuelva más de una no es un fallo de la búsqueda: es el término el que
    es ambiguo, y el momento de resolverlo es al crear la obligación, no cuando
    un ente de control pregunte por qué el informe salió 104 veces en vez de 26.

    Los casos que ocurren de verdad:

      «bisemanal»  → semanal marcando dos días (104/año) o cada dos semanas
                     (26/año). La RAE admite los dos sentidos.
      «quincenal»  → dos veces al mes en días fijos (24/año) o cada quince días
                     corridos (26/año). El segundo se desplaza; el primero no.
      «bianual»    → cada dos años, o dos veces al año.
    """
    t = (termino or "").strip().lower()
    if not t:
        return []
    return [clave for clave, p in CATALOGO.items()
            if t in p.sinonimos or t in p.nombre.lower()]


def es_ambiguo(termino: str) -> bool:
    return len(buscar(termino)) > 1


def catalogo_para_elegir() -> list[tuple[str, str, str]]:
    """(clave, nombre, significado) de TODAS, para el primer paso del asistente.

    El significado va siempre, no solo cuando dos nombres colisionan: lo que
    identifica una periodicidad es lo que produce, y el nombre puede mentir.
    «Bisemanal» y «bianual» lo demuestran — cada uno significa dos cosas.
    """
    return [(clave, p.nombre, p.significado()) for clave, p in CATALOGO.items()]


def desambiguar(termino: str) -> list[tuple[str, str, str]]:
    """Lo mismo, restringido a las que responden a un término.

    Devuelve varias cuando el término es ambiguo, y entonces el significado es
    justamente lo que permite elegir.
    """
    return [(clave, CATALOGO[clave].nombre, CATALOGO[clave].significado())
            for clave in buscar(termino)]
