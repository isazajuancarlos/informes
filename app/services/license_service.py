"""Licencia offline: un archivo firmado que la aplicación verifica en local.

Un cliente air-gapped no puede validar contra un servidor. La alternativa es un
archivo firmado con la misma criptografía post-cuántica que ya usa el sistema
para las entregas (Ed25519 + ML-DSA-87, vía Quipu): EL LICENCIANTE lo emite con
su clave privada, la aplicación lo verifica con la pública, y nadie necesita red.

# LO QUE ESTE MÓDULO NO HACE, Y ES LA DECISIÓN MÁS IMPORTANTE

**No bloquea nada. Nunca.**

Ni sin licencia, ni con una vencida, ni con una manipulada, ni con exceso de
usuarios. Y no es una omisión: es la aplicación directa del principio del modo
degradado.

    La ausencia de una capa OPCIONAL nunca debe impedirle al cliente cumplir una
    obligación que tiene con un TERCERO.

Este sistema existe para que una organización no incumpla ante Supersalud, una
EPS o un ente de control. El día que una licencia vencida impida registrar una
entrega, **Xiliux pasa a ser la causa del incumplimiento del cliente** — y de eso
responde Xiliux, no el cliente. Un bloqueo convertiría una función en un defecto.

Lo que sí hace, y es lo que un licenciamiento serio necesita:

  - **Visibilidad**: un aviso permanente y visible en la interfaz.
  - **Evidencia**: el estado queda registrado y es auditable.

El remedio ante el exceso de uso es CONTRACTUAL (Cláusula Tercera y Anexo A del
EULA), no técnico. El software aporta la prueba; el contrato, la consecuencia.
Un DRM que se puede rodear con `chmod` da una falsa sensación de control y, a
cambio, regala el riesgo de tumbarle la operación a un hospital un lunes a las
siete de la mañana.

# El formato

Los campos son EXACTAMENTE los del Anexo A del EULA, para que el archivo y el
contrato no puedan discrepar. Si el Anexo A cambia, este módulo cambia con él.

# Emitir una licencia

    python emitir_licencia.py --licenciatario "ACME S.A.S." --nit 900123456 \\
        --edicion profesional --perfil onprem --usuarios 25 --sedes 3 \\
        --vigencia-hasta 2027-07-21
"""
from __future__ import annotations

import json
import os
from dataclasses import dataclass
from datetime import date, datetime

try:
    import quipu  # paquete PyPI: quipu-crypto
    _DISPONIBLE = True
except Exception:
    quipu = None
    _DISPONIBLE = False

#: Ediciones válidas (Anexo A del EULA).
EDICIONES = ("esencial", "profesional", "corporativa")
#: Perfiles de despliegue válidos (Anexo A del EULA).
PERFILES = ("standalone", "onprem", "cloud")

#: Campos que TODA licencia debe traer. Faltar uno la invalida: una licencia a
#: medias no puede pasar por buena, porque su función es delimitar el alcance y
#: un alcance incompleto no delimita nada (directiva 20).
CAMPOS = (
    "licenciatario", "nit", "edicion", "perfil", "version",
    "usuarios", "sedes", "modalidad", "emitida", "vigencia_hasta",
)


def _ruta() -> str:
    return os.getenv("INFORMES_LICENCIA", "licencia.quipu")


def _clave_publica() -> str:
    """Clave de verificación del LICENCIANTE.

    Va en configuración y no incrustada en el código a propósito: así el mismo
    binario sirve para varios licenciantes (revendedor, filial) sin recompilar.
    Su secreto no importa —es pública—; lo que importa es que el cliente no la
    pueda cambiar sin que se note, y de eso se ocupa el contrato, no el código:
    sustituirla es exactamente igual de detectable que borrar el archivo.
    """
    return os.getenv("INFORMES_LICENCIA_PUBKEY", "")


@dataclass(frozen=True)
class Estado:
    """Veredicto sobre la licencia. Tres resultados, no dos."""

    #: `valida` | `ausente` | `invalida` | `vencida` | `sin_verificador`
    situacion: str
    #: Texto corto para la interfaz.
    mensaje: str
    #: Datos de la licencia si se pudo leer y verificar (aunque esté vencida).
    datos: dict | None = None

    @property
    def hay_que_avisar(self) -> bool:
        return self.situacion != "valida"

    @property
    def es_grave(self) -> bool:
        """Distingue «no hay licencia» de «la licencia no cuadra».

        Sin licencia es un estado normal en evaluación o en una instalación
        recién hecha. Una licencia manipulada o vencida es otra cosa, y
        mezclarlas entrenaría a ignorar el aviso.
        """
        return self.situacion in ("invalida", "vencida")


def _canonico(datos: dict) -> bytes:
    """Bytes deterministas de la licencia. Es lo que se firma."""
    return json.dumps(
        {c: datos.get(c) for c in CAMPOS},
        sort_keys=True, separators=(",", ":"), ensure_ascii=False, default=str,
    ).encode("utf-8")


def emitir(datos: dict, signing_key_hex: str) -> str:
    """Emite una licencia firmada. Lo usa EL LICENCIANTE, no el cliente."""
    if not _DISPONIBLE:
        raise RuntimeError("quipu-crypto no está instalado")
    faltan = [c for c in CAMPOS if not datos.get(c)]
    if faltan:
        raise ValueError(f"faltan campos obligatorios: {', '.join(faltan)}")
    if datos["edicion"] not in EDICIONES:
        raise ValueError(f"edicion debe ser una de {EDICIONES}")
    if datos["perfil"] not in PERFILES:
        raise ValueError(f"perfil debe ser uno de {PERFILES}")
    return quipu.encode_signed(_canonico(datos), bytes.fromhex(signing_key_hex))


def _leer_archivo() -> str | None:
    ruta = _ruta()
    if not os.path.isfile(ruta):
        return None
    try:
        with open(ruta, "r", encoding="utf-8") as f:
            return f.read().strip()
    except OSError:
        return None


def estado(hoy: date | None = None) -> Estado:
    """Verifica la licencia instalada y devuelve el veredicto.

    Se llama en cada render de la cabecera, así que es barato: leer un archivo
    pequeño y verificar una firma. No se cachea a propósito — una licencia que
    se sustituye en caliente debe reflejarse sin reiniciar, y cachear el
    veredicto es cómo se consigue que la interfaz mienta durante horas.
    """
    hoy = hoy or date.today()

    if not _DISPONIBLE:
        return Estado("sin_verificador",
                      "No se puede verificar la licencia: falta quipu-crypto.")
    vk_hex = _clave_publica()
    if not vk_hex:
        return Estado("sin_verificador",
                      "No se puede verificar la licencia: falta "
                      "INFORMES_LICENCIA_PUBKEY.")

    firmado = _leer_archivo()
    if not firmado:
        return Estado("ausente",
                      "Sin licencia instalada. El sistema funciona con todas "
                      "sus funciones; el uso se rige por el contrato.")

    try:
        crudo = quipu.decode_verified(firmado, bytes.fromhex(vk_hex))
        datos = json.loads(crudo.decode("utf-8"))
    except Exception:
        # Firma que no valida, clave equivocada o archivo corrupto: todo da el
        # MISMO resultado. Distinguirlos aquí no ayudaría a nadie honesto y sí
        # orientaría a quien intente fabricar una.
        return Estado("invalida",
                      "La licencia instalada no es válida o fue alterada. "
                      "Avise a EL LICENCIANTE.")

    if any(not datos.get(c) for c in CAMPOS):
        return Estado("invalida", "La licencia está incompleta.")

    try:
        hasta = datetime.strptime(str(datos["vigencia_hasta"]), "%Y-%m-%d").date()
    except (ValueError, TypeError):
        return Estado("invalida", "La licencia tiene una vigencia ilegible.", datos)

    if hoy > hasta:
        dias = (hoy - hasta).days
        return Estado(
            "vencida",
            f"La licencia venció hace {dias} día(s) ({hasta.isoformat()}). "
            "El sistema sigue operando con normalidad; regularice con "
            "EL LICENCIANTE.",
            datos,
        )

    return Estado("valida", f"Licencia vigente hasta {hasta.isoformat()}.", datos)


def excede_usuarios(activos: int) -> tuple[bool, str]:
    """¿Se superó el número de Usuarios Autorizados del Anexo A?

    Devuelve (excede, mensaje). **No bloquea**: informa. El remedio es
    contractual (Cláusula Tercera), y el software solo aporta la prueba.
    """
    e = estado()
    if e.datos is None:
        return (False, "")
    try:
        tope = int(e.datos["usuarios"])
    except (KeyError, TypeError, ValueError):
        return (False, "")
    if activos > tope:
        return (True, f"Usuarios activos ({activos}) por encima de los "
                      f"autorizados en el Anexo A ({tope}).")
    return (False, "")
