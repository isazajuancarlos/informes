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

    @property
    def se_puede_guardar(self) -> bool:
        return not self.errores


def validar(config: dict) -> Resultado:
    """Comprueba una regla de periodicidad. No toca la base de datos."""
    r = Resultado()
    modo = config.get("modo", "CICLICO")

    if modo == "CICLICO":
        _ciclico(config, r)
    elif modo == "ESPECIFICO":
        _especifico(config, r)
    elif modo == "SEMANAL":
        _semanal(config, r)
    elif modo == "HOURLY":
        _horas(config, r)

    # La comprobación que cubre lo que no se ha imaginado: ¿genera algo?
    # Se hace al final y solo si no hay errores ya explicados, para no repetir
    # el mismo problema con dos redacciones distintas.
    if not r.errores and modo not in ("HOURLY", "INMEDIATO"):
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
    if not config.get("fecha_inicio"):
        r.errores.append("Una periodicidad cíclica necesita fecha de inicio.")


#: Días hábiles que tiene un mes. Comprobado sobre 2020-2040 con el calendario
#: colombiano: entre 18 y 23. Por encima de 23 no existe en ningún mes.
HABILES_MINIMO_MES = 18
HABILES_MAXIMO_MES = 23


def _especifico(config, r):
    if not config.get("meses"):
        r.errores.append("Seleccione al menos un mes de entrega.")

    if config.get("dia_habil"):
        n = _entero(config.get("dia_habil"), 0)
        if not 1 <= n <= HABILES_MAXIMO_MES:
            r.errores.append(
                f"El día hábil debe estar entre 1 y {HABILES_MAXIMO_MES}. "
                f"Ningún mes tiene más días hábiles que eso.")
        elif n > HABILES_MINIMO_MES:
            # No es un error: se usa el último hábil, igual que el día 31 en un
            # mes de 30. Pero quien pide el día 20 espera el 20.
            r.advertencias.append(
                f"Algunos meses tienen solo {HABILES_MINIMO_MES} días hábiles. "
                f"En esos, el vencimiento será el último día hábil del mes.")
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
            # No es un error: el motor usa el último día real del mes. Pero si
            # nadie lo dice, quien puso 31 espera el 31 y en febrero verá el 28.
            r.advertencias.append(
                f"Con día de corte {dia}, en los meses más cortos el vencimiento "
                "será el último día del mes."
            )
    elif dia and dia != 1:
        # Los dos campos puestos: el motor ignora `dia_corte` en silencio.
        r.advertencias.append(
            f"Marcó «último día del mes», así que el día de corte ({dia}) no se "
            "usará."
        )


def _semanal(config, r):
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
    ini = _entero(config.get("hora_inicio"), -1)
    fin = _entero(config.get("hora_fin"), -1)
    intervalo = _entero(config.get("intervalo_horas"), 0)
    if not 0 <= ini <= 23 or not 0 <= fin <= 23:
        r.errores.append("Las horas deben estar entre 0 y 23.")
    elif ini > fin:
        r.errores.append(
            f"La hora de inicio ({ini}) es posterior a la de fin ({fin}), y así "
            "no se genera ningún vencimiento. Para un turno nocturno, cree dos "
            "franjas: una hasta las 23 y otra desde las 0."
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
