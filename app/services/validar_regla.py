"""Valida una regla de periodicidad ANTES de guardarla.

# El patrón que este módulo existe para cortar

Un barrido sistemático del motor (2026-07-21) encontró varias configuraciones
que un usuario puede crear sin esfuerzo y que **el sistema aceptaba sin decir
nada**:

    cíclico con intervalo negativo (un dedazo) ...... 0 vencimientos, en silencio
    horas con inicio 18 y fin 8 (turno nocturno) .... 0 vencimientos, en silencio
    específico sin meses marcados .................. 0 vencimientos, en silencio
    semanal con varios días y ajuste ............... vencimientos DUPLICADOS
    día de corte 5 + «último día del mes» .......... gana «último», sin avisar

Todas comparten forma: **la obligación queda creada y no hace lo que su autor
cree**. Nadie recibe una tarea que no existe, así que el error no se descubre
por su efecto — se descubre cuando lo pregunta un ente de control.

# La estrategia: verificar el RESULTADO, no enumerar los errores

Enumerar combinaciones malas es una carrera perdida: siempre queda una. Lo que
sí cubre todos los casos —incluidos los que nadie ha imaginado— es **ejecutar la
regla y comprobar que produce vencimientos**. Si no produce nada, da igual por
qué: no se guarda.

Las comprobaciones concretas de más abajo existen igualmente, pero no para
detectar (de eso se encarga la ejecución) sino para **explicar**: «no genera
nada» no ayuda a arreglarlo; «el intervalo no puede ser negativo», sí.

# Errores y advertencias son cosas distintas

  - **Error**: la regla no puede guardarse. Produciría una obligación que no
    funciona.
  - **Advertencia**: la regla funciona, pero probablemente no hace lo que se
    espera. Se guarda; se avisa.

Confundirlas tendría un coste concreto: si todo bloquea, alguien acaba
buscándole la vuelta al validador; si nada bloquea, se guardan reglas rotas.
"""
from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Resultado:
    errores: list[str] = field(default_factory=list)
    advertencias: list[str] = field(default_factory=list)

    #: Campos que el usuario ya ha tenido ocasión de responder. `None` = todos.
    _respondidos: set | None = None

    @property
    def se_puede_guardar(self) -> bool:
        return not self.errores

    def toca(self, *campos) -> bool:
        """¿Ya se preguntó por alguno de estos campos?

        Callar sobre lo que aún no se ha preguntado no es ser permisivo: es no
        acusar a alguien de no haber contestado algo que nadie le ha preguntado.
        """
        if self._respondidos is None:
            return True
        return any(c in self._respondidos for c in campos)


#: Los pasos en que se puede ir construyendo una regla, y qué decide cada uno.
#:
#: Existe para que un asistente pueda revisar CADA PÁGINA en vez de esperar al
#: final: en el paso 1 no tiene sentido quejarse del día de corte, porque
#: todavía no se ha preguntado. Sin esto, el asistente enseñaría errores de cosas
#: que el usuario aún no ha tenido ocasión de responder, y eso entrena a pasar de
#: los avisos antes incluso de terminar.
#:
#: Es una DECLARACIÓN, no una segunda implementación: las reglas siguen siendo
#: las de abajo, y el asistente y el formulario de una sola página usan
#: exactamente las mismas. Dos juegos de reglas garantizan que uno esté mal.
PASOS = [
    ("Periodicidad", {"modo"}),
    ("Cuándo vence", {"meses", "cada_n_anios", "dia_corte", "dias_corte", "dia_habil",
                      "es_ultimo_dia", "usar_ultimo_dia_en_meses_cortos",
                      "dia_semana_mes", "semana_del_mes", "intervalo",
                      "dias_semana", "dia_habil_semana", "fechas", "hora_inicio", "hora_fin",
                      "intervalo_horas"}),
    ("Si cae en día no hábil", {"ajuste_habil", "ajuste_habil_anterior",
                                "mover_habil", "mover_habil_anterior"}),
    ("Desde cuándo rige", {"fecha_inicio"}),
]


def campos_hasta(paso: int) -> set:
    """Los campos ya decididos al llegar a `paso` (0 = el primero)."""
    decididos = set()
    for _, campos in PASOS[:paso + 1]:
        decididos |= campos
    return decididos


def validar(config: dict, respondidos: set | None = None,
            periodicidad: str | None = None) -> Resultado:
    """Comprueba una regla de periodicidad. No toca la base de datos.

    `periodicidad` es el nombre que el usuario le dio —«anual», «trimestral»—.
    Si se da, se comprueba que la regla genere lo que ese nombre PROMETE: el
    nombre es una afirmación y el motor sabe contar.

    `respondidos` limita la revisión a los campos que el usuario ya ha tenido
    ocasión de contestar. Se usa en el asistente para revisar página a página;
    con `None` —el formulario de una sola página— se revisa todo, que es el
    comportamiento de siempre.
    """
    r = Resultado()
    r._respondidos = respondidos  # noqa: SLF001 — lo consultan los _ de abajo
    modo = config.get("modo", "CICLICO")

    if modo == "CICLICO":
        _ciclico(config, r)
    elif modo == "ESPECIFICO":
        _especifico(config, r)
    elif modo == "FECHAS":
        _fechas(config, r)
    elif modo == "SEMANAL":
        _semanal(config, r)
    elif modo == "HOURLY":
        _horas(config, r)

    # Campos contestados que este modo NO va a mirar. Es el defecto en su forma
    # más pura: el dato está puesto, quien lo puso cree que hace algo, y no hace
    # nada. Nunca se descubre por su efecto, porque el efecto es que no pasa
    # nada. En el asistente esto no debería llegar a ocurrir —cada paso pregunta
    # solo lo suyo— pero una regla puede venir de una importación o de una
    # versión anterior.
    if respondidos is None:
        from app.services.periodicidad import (campos_ignorados,
                                               formas_del_dia_contestadas)

        ignorados = campos_ignorados(config)
        if ignorados:
            r.advertencias.append(
                f"Con esta periodicidad no se usarán: {', '.join(ignorados)}. "
                f"Están puestos pero no cambian ninguna fecha.")

        formas = formas_del_dia_contestadas(config)
        if len(formas) > 1:
            from app.services.periodicidad import FORMAS_DEL_DIA
            r.advertencias.append(
                "Hay varias formas de indicar el día contestadas a la vez ("
                + ", ".join(FORMAS_DEL_DIA[f] for f in formas)
                + "). Son excluyentes: solo se aplicará una.")

    if not r.errores and periodicidad and respondidos is None:
        _coincide_con_su_nombre(config, periodicidad, r)

    # La comprobación que cubre lo que no se ha imaginado: ¿genera algo?
    # Se hace al final y solo si no hay errores ya explicados, para no repetir
    # el mismo problema con dos redacciones distintas.
    # La comprobación de que GENERA algo necesita la regla entera.
    if not r.errores and respondidos is None and modo not in ("HOURLY", "INMEDIATO"):
        from datetime import date, datetime

        from app.services.preview_calendario import proximos

        # Se barre desde la FECHA DE INICIO de la regla, no desde hoy. La
        # pregunta correcta es «¿genera algo alguna vez?», no «¿genera algo de
        # hoy en adelante?»: una obligación de una sola vez cuya fecha ya pasó
        # es legítima —el sistema admite registro retroactivo, para eso existe
        # MAX_RETROACTIVE_DAYS— y rechazarla impediría registrar el pasado.
        desde = None
        if config.get("fecha_inicio"):
            try:
                desde = datetime.strptime(str(config["fecha_inicio"]), "%Y-%m-%d").date()
            except (ValueError, TypeError):
                r.errores.append("La fecha de inicio debe tener el formato AAAA-MM-DD.")
                return r
        venc, aviso = proximos(config, cuantos=1, desde=desde or date.today())
        if not venc:
            r.errores.append(
                "Esta regla no genera ningún vencimiento. Revise la "
                "periodicidad, los meses seleccionados y la fecha de inicio."
                + (f" ({aviso})" if aviso else "")
            )

    return r


def _entero(valor, defecto=0):
    try:
        return int(valor)
    except (TypeError, ValueError):
        return defecto


def _ciclico(config, r):
    if not r.toca("intervalo"):
        return
    intervalo = _entero(config.get("intervalo"), -1)
    if intervalo < 0:
        r.errores.append(
            "El intervalo no puede ser negativo. Use 0 para una obligación que "
            "ocurre una sola vez."
        )
    elif intervalo == 0:
        r.advertencias.append(
            "Con intervalo 0 la obligación ocurre UNA SOLA VEZ, en la fecha de "
            "inicio. Si esperaba que se repitiera, indique cada cuántos días."
        )
    if r.toca("fecha_inicio") and not config.get("fecha_inicio"):
        r.errores.append("Una periodicidad cíclica necesita fecha de inicio.")


#: Máximo de días hábiles que puede tener un mes: 23.
#:
#: Es MATEMÁTICO, no del calendario de ningún país — un mes de 31 días que
#: empieza en lunes tiene 23 días de lunes a viernes, y los festivos solo pueden
#: restar. Por eso sí puede ir escrito aquí.
HABILES_MAXIMO_MES = 23


def _habiles_minimos_del_mes(anios=5):
    """El mes más flaco del calendario CONFIGURADO, no del colombiano.

    Aquí había un 18 escrito a mano, medido sobre los festivos de Colombia. Es
    justo el error contra el que existe la directiva de diseñar para lo
    universal: el número correcto depende de cuántos festivos tenga el país del
    cliente, y una entidad con otro calendario habría recibido un aviso falso.

    El 23 de arriba sí puede ir fijo porque no depende de ningún calendario.
    Este no, así que se cuenta.
    """
    import calendar
    from datetime import date

    from app.services.calendar_engine import MotorReglasDinamicas

    motor = MotorReglasDinamicas()
    hoy = date.today().year
    return min(
        sum(1 for d in range(1, calendar.monthrange(a, m)[1] + 1)
            if motor._es_habil(date(a, m, d)))
        for a in range(hoy, hoy + anios) for m in range(1, 13)
    )


def _fechas(config, r):
    """Lista de fechas explícita: la salida de emergencia del diseño.

    Una obligación que no sigue ningún patrón se enumera. Sin esto, «el sistema
    puede crear cualquier informe» sería cierto solo para las formas que se nos
    ocurrieron.
    """
    if not r.toca("fechas"):
        return
    fechas = config.get("fechas") or []
    if not fechas:
        r.errores.append("Escriba al menos una fecha, en formato AAAA-MM-DD.")
        return
    from datetime import datetime

    malas = []
    for texto in fechas:
        try:
            datetime.strptime(str(texto).strip(), "%Y-%m-%d")
        except (ValueError, TypeError):
            malas.append(str(texto))
    if malas:
        # Se nombran. «Hay fechas mal escritas» obliga a buscarlas a ojo.
        r.errores.append(
            "Estas fechas no se entienden, use el formato AAAA-MM-DD: "
            + ", ".join(malas[:5]) + ("…" if len(malas) > 5 else ""))


def _especifico(config, r):
    if r.toca("meses") and not config.get("meses"):
        r.errores.append("Seleccione al menos un mes de entrega.")

    if r.toca("cada_n_anios") and config.get("cada_n_anios"):
        cada = _entero(config.get("cada_n_anios"), 0)
        if cada < 1:
            r.errores.append("«Cada cuántos años» debe ser 1 o más.")
        elif cada > 1 and not config.get("fecha_inicio"):
            # Sin fecha de inicio no hay desde dónde contar los años, y el motor
            # tomaría el año que se esté evaluando: la obligación saldría todos
            # los años sin que nadie entendiera por qué.
            r.errores.append(
                "Con una periodicidad de varios años hace falta la fecha de "
                "inicio: es el año desde el que se cuenta.")

    if config.get("dias_corte"):
        fuera = [d for d in config["dias_corte"]
                 if not (isinstance(d, int) or str(d).isdigit()) or not 1 <= int(d) <= 31]
        if fuera:
            r.errores.append(
                f"Los días de corte deben estar entre 1 y 31: {fuera}")
        else:
            if len(set(int(d) for d in config["dias_corte"])) != len(config["dias_corte"]):
                r.advertencias.append(
                    "Hay días de corte repetidos; solo se generará un "
                    "vencimiento por día.")
            # La misma regla que para el día suelto: si alguno de los días no
            # existe en alguno de los meses, lo decide el administrador. Que la
            # comprobación viviera solo en la rama del escalar era un agujero:
            # `dias_corte: [15, 31]` se colaba sin decidir nada.
            for d in sorted({int(x) for x in config["dias_corte"]}):
                if d in (29, 30, 31):
                    _dia_que_algunos_meses_no_tienen(config, d, r)
        return  # manda la lista; el resto no aplica

    if not r.toca("dia_corte", "dias_corte", "dia_habil", "es_ultimo_dia",
                  "dia_semana_mes", "semana_del_mes"):
        return

    if config.get("dia_semana_mes") is not None and config.get("semana_del_mes"):
        dia = _entero(config.get("dia_semana_mes"), -1)
        ordinal = _entero(config.get("semana_del_mes"), 0)
        if not 0 <= dia <= 6:
            r.errores.append("El día de la semana debe estar entre 0 (lunes) y 6.")
        if ordinal not in (1, 2, 3, 4, 5, -1):
            r.errores.append(
                "La semana del mes debe ser 1, 2, 3, 4, 5 o -1 para «la última».")
        elif ordinal == 5:
            r.advertencias.append(
                "No todos los meses tienen un quinto; en esos se usará el "
                "último. Si quería «el último», use -1.")
        return

    if config.get("dia_habil"):
        n = _entero(config.get("dia_habil"), 0)
        if not 1 <= n <= HABILES_MAXIMO_MES:
            r.errores.append(
                f"El día hábil debe estar entre 1 y {HABILES_MAXIMO_MES}. "
                f"Ningún mes tiene más días hábiles que eso.")
        else:
            minimo = _habiles_minimos_del_mes()
            if n > minimo:
                # No es un error: se usa el último hábil, igual que el día 31 en
                # un mes de 30. Pero quien pide el día 20 espera el 20.
                r.advertencias.append(
                    f"Con este calendario, algunos meses tienen solo {minimo} "
                    f"días hábiles. En esos, el vencimiento será el último día "
                    f"hábil del mes.")
        if config.get("dia_corte") or config.get("es_ultimo_dia"):
            r.advertencias.append(
                "Indicó un día hábil, así que el día de corte y «último día del "
                "mes» no se usarán.")
        return  # el resto de comprobaciones son sobre el día corrido

    dia = _entero(config.get("dia_corte"), 0)
    if not config.get("es_ultimo_dia"):
        if not 1 <= dia <= 31:
            r.errores.append("El día de corte debe estar entre 1 y 31.")
        elif dia in (29, 30, 31):
            _dia_que_algunos_meses_no_tienen(config, dia, r)
    elif dia and dia != 1:
        # Los dos campos puestos: el motor ignora `dia_corte` en silencio.
        r.advertencias.append(
            f"Marcó «último día del mes», así que el día de corte ({dia}) no se "
            "usará."
        )


def _semanal(config, r):
    if not r.toca("dias_semana", "dia_habil_semana"):
        return
    if config.get("dia_habil_semana"):
        n = _entero(config.get("dia_habil_semana"), 0)
        if not 1 <= n <= 7:
            r.errores.append(
                "El día hábil de la semana debe estar entre 1 y 7.")
        elif n > 5:
            r.advertencias.append(
                "Una semana normal tiene 5 días hábiles; en las que tengan "
                "menos, el vencimiento será el último día hábil de la semana.")
        if config.get("dias_semana"):
            r.advertencias.append(
                "Indicó un día hábil de la semana, así que los días marcados "
                "no se usarán.")
        return
    dias = config.get("dias_semana") or []
    if not dias:
        r.errores.append("Seleccione al menos un día de la semana.")
    # Aquí hubo dos advertencias que ya NO corresponden, y conviene decir por
    # qué desaparecieron: se advertía de vencimientos duplicados con varios días
    # y ajuste, y de que el ajuste «anterior» no se aplicaba en este modo.
    #
    # Las dos eran parches sobre una asimetría del motor —SEMANAL emitía el
    # vencimiento el día que lo ORIGINABA en vez del día en que CAE—. Al
    # alinearlo con el modo ESPECÍFICO ambas causas desaparecieron, y advertir
    # de algo que ya no ocurre entrena a ignorar los avisos.


def _horas(config, r):
    if not r.toca("hora_inicio", "hora_fin", "intervalo_horas"):
        return
    ini = _entero(config.get("hora_inicio"), -1)
    fin = _entero(config.get("hora_fin"), -1)
    intervalo = _entero(config.get("intervalo_horas"), 0)
    if not 0 <= ini <= 23 or not 0 <= fin <= 23:
        r.errores.append("Las horas deben estar entre 0 y 23.")
    elif ini > fin:
        # NO es un error: es un turno que cruza la medianoche. Antes se
        # rechazaba y se le pedía al administrador que partiera su obligación en
        # dos para caber en el sistema. El sistema tiene que admitir la forma que
        # tenga la obligación, no al revés.
        r.advertencias.append(
            f"La franja va de las {ini} a las {fin} del día siguiente: es un "
            f"turno que cruza la medianoche. Si no era lo que quería, invierta "
            f"las horas."
        )
    if intervalo < 1:
        r.errores.append("El intervalo en horas debe ser de al menos 1.")


# El aviso «el primer vencimiento cae antes de la vigencia» vivió aquí unas
# horas y se retiró al corregir el motor.
#
# Decía que con «día 1 + hábil anterior» desde el 1 de enero, el vencimiento de
# enero no se generaría porque su fecha caía el 31 de diciembre. Era cierto, y
# era un parche sobre una confusión de fondo: el motor identificaba la
# obligación por la fecha YA AJUSTADA, así que el vencimiento de enero
# «pertenecía» a diciembre y quedaba fuera de vigencia.
#
# Separando el día NOMINAL del día LÍMITE la obligación ya no se pierde: nace el
# 1 de enero, dentro de vigencia, y su entrega se adelanta al 31 de diciembre.
# No hay nada que advertir. Mantener el aviso habría entrenado a ignorar los
# avisos, que es el modo en que un validador deja de servir.

#: Días máximos de cada mes, en el peor caso (febrero de año común).
_DIAS_DEL_MES = {1: 31, 2: 28, 3: 31, 4: 30, 5: 31, 6: 30,
                 7: 31, 8: 31, 9: 30, 10: 31, 11: 30, 12: 31}
_NOMBRE_MES = {1: "enero", 2: "febrero", 3: "marzo", 4: "abril", 5: "mayo",
               6: "junio", 7: "julio", 8: "agosto", 9: "septiembre",
               10: "octubre", 11: "noviembre", 12: "diciembre"}


def _dia_que_algunos_meses_no_tienen(config, dia, r):
    """Un informe «el día 30 de cada mes» no puede guardarse tal cual.

    Febrero nunca tiene un día 30. El motor usaba el último día real del mes, y
    eso es **rellenar un dato que el administrador no dio**: quien escribió 30
    espera el 30, y en febrero se encuentra con el 28 sin haberlo decidido. Un
    valor de relleno produce un sistema que parece funcionar, y eso es peor que
    uno que se queja: la queja se atiende en un minuto, la fecha inventada no se
    nota nunca.

    Tampoco vale descartar el mes en silencio: entonces desaparece una
    obligación y nadie la ve hasta que la pregunta un ente de control.

    Así que se bloquea y se le devuelve la decisión, que es suya: o declara la
    excepción para los meses que no llegan a ese día, o elige otro día, o quita
    esos meses de la selección. El camino permisivo sigue existiendo —hay
    obligaciones que de verdad vencen «el último día»— pero hay que **pedirlo**.
    """
    meses = [m for m in (config.get("meses") or []) if isinstance(m, int)]
    cortos = [m for m in meses if _DIAS_DEL_MES.get(m, 31) < dia]
    if not cortos:
        return  # ningún mes seleccionado se queda corto: nada que decidir

    if config.get("usar_ultimo_dia_en_meses_cortos"):
        if dia == 31:
            # «Día 31, y si el mes no lo tiene, el último» produce EXACTAMENTE
            # las mismas fechas que «último día del mes» — comprobado, no
            # supuesto. Son dos formas de escribir la misma obligación, y
            # admitir las dos es sembrar el problema: dentro de un año nadie
            # sabrá si la norma decía «el 31» o «el último día», que NO son la
            # misma exigencia aunque coincidan en el calendario.
            #
            # Se rechaza la que no la nombra. No por imposible —funciona— sino
            # porque el registro tiene que decir lo que la norma dice.
            r.errores.append(
                "«Día 31, y el último cuando no exista» es exactamente «último "
                "día del mes», pero escrito de forma que no lo dice. Marque "
                "«Último día del mes»: la obligación queda registrada con su "
                "nombre, y dentro de un año se sabrá qué exigía la norma.")
            return
        # Lo pidió explícitamente. Se confirma qué va a pasar y se sigue.
        r.advertencias.append(
            f"En {_lista(cortos)} no hay día {dia}: el vencimiento será el "
            f"último día del mes, como usted indicó. Ojo: esto NO es «el último "
            f"día del mes» —en {_lista([m for m in (config.get('meses') or []) if m not in cortos][:2])} "
            f"y los demás será el {dia}, no el último.")
        return

    r.errores.append(
        f"No existe el día {dia} en {_lista(cortos)}. Elija una de estas tres: "
        f"marcar «usar el último día del mes cuando no exista», cambiar el día "
        f"de corte, o quitar {'esos meses' if len(cortos) > 1 else 'ese mes'} "
        f"de la selección."
    )


def _lista(meses):
    nombres = [_NOMBRE_MES[m] for m in sorted(meses)]
    if len(nombres) == 1:
        return nombres[0]
    return ", ".join(nombres[:-1]) + " ni " + nombres[-1]


def _coincide_con_su_nombre(config, clave, r):
    """¿La regla genera lo que su nombre promete?

    Un informe ANUAL entregado dos veces al año no es anual. Hasta ahora eso se
    guardaba en silencio: la periodicidad era una etiqueta del formulario y nada
    la comprobaba contra lo que la regla produce de verdad.

    No se rechaza la FORMA —dos entregas al año es una obligación legítima— sino
    el NOMBRE, y se dice cuál corresponde. Es lo mismo que «día 31 con excepción»
    frente a «último día del mes»: el registro tiene que decir lo que la norma
    dice, porque dentro de un año eso es lo único que quedará.
    """
    from datetime import date, timedelta

    from app.services.periodicidad import CATALOGO
    from app.services.scheduler import GeneradorTareas

    p = CATALOGO.get(clave)
    if p is None or p.al_anio is None:
        return  # su número depende de lo que se conteste; no hay nada que exigir

    # Se cuenta sobre un año completo empezando en la vigencia, no sobre el año
    # natural: una regla que arranca en julio tendría la mitad de vencimientos
    # en su primer año natural y saldría marcada por una razón falsa.
    try:
        desde = date.fromisoformat(str(config.get("fecha_inicio")))
    except (ValueError, TypeError):
        return
    hasta = date(desde.year + 1, desde.month, desde.day) - timedelta(days=1)

    total, dia = 0, desde
    while dia <= hasta:
        total += len(GeneradorTareas.obtener_vencimientos_para_fecha(dia, config))
        dia += timedelta(days=1)

    if total == p.al_anio:
        return

    sugerida = next((q.nombre for q in CATALOGO.values() if q.al_anio == total), None)
    arreglo = (f" Con esa cantidad, la periodicidad que corresponde es "
               f"«{sugerida}»." if sugerida else
               " Revise los meses y los días seleccionados.")
    r.errores.append(
        f"Marcó «{p.nombre}», que son {p.al_anio} entrega(s) al año, pero esta "
        f"regla genera {total}.{arreglo}")
