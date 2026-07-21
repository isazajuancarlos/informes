"""
Firma de entregas con Quipu — PROTOTIPO.

Firma post-cuántica verificable (Ed25519 + ML-DSA-87, vía `quipu-crypto`) de los
campos auditables de una entrega. Da **integridad** y **no repudio** — encaja con
la misión de trazabilidad del sistema. La firma NO cifra: es autenticidad, no
confidencialidad (la firma va "en claro").

Requiere `quipu-crypto` (ver requirements-firma.txt). Es AGPL-3.0; se usa aquí
como producto propio (Xiliux es titular de Quipu, doble licencia). Si no está
instalado o faltan las claves, el servicio se **desactiva con gracia**:
`firmar_entrega` devuelve None y `verificar_entrega` devuelve (False, motivo),
sin romper el resto del sistema.

CUSTODIA DE LA CLAVE — dos perfiles, según lo que el despliegue exija:

  QUIPU_CUSTODIA=entorno   (por defecto)
      La clave privada vive en `QUIPU_SIGNING_KEY`. Simple y suficiente para un
      despliegue de un solo operador. Su límite hay que decirlo claro: quien
      lea las variables de entorno del proceso tiene la clave, y con ella puede
      firmar entregas que nunca ocurrieron.

      Setup:
          from app.services.signing_service import generar_claves
          vk, sk = generar_claves()
          #   QUIPU_VERIFYING_KEY=<vk>   (pública)
          #   QUIPU_SIGNING_KEY=<sk>     (SECRETA)

  QUIPU_CUSTODIA=hsm
      La clave privada NUNCA entra al proceso: vive dentro de un dispositivo
      PKCS#11 (HSM, token, tarjeta) y la firma se calcula ahí dentro. Es lo que
      responde a la primera pregunta de todo comité de seguridad.

          QUIPU_PKCS11_MODULE=/ruta/libXXX_pkcs11.so
          QUIPU_PKCS11_PIN=...            (opcional, si el token lo exige)
          QUIPU_PKCS11_ED25519=<etiqueta de la clave Ed25519>
          QUIPU_PKCS11_MLDSA=<etiqueta de la clave ML-DSA>

      La clave pública se lee del propio dispositivo: no hay que configurarla,
      y así no puede desincronizarse de la privada.

POR QUÉ NO SE OFRECE CUSTODIA POR UMBRAL (Shamir k-de-n), aunque Quipu la tenga:

    Repartir la clave entre varias personas protege lo que se guarda, no lo que
    se USA: para firmar hay que reconstruirla, y entonces vuelve a estar entera
    en memoria. Y sobre todo, aquí no encaja con la operación — se firma en
    CADA entrega, sin nadie delante, a cualquier hora. No se puede convocar a
    tres custodios cada vez que alguien sube un informe.

    Shamir sirve para el RESPALDO de la clave (que nadie pueda perderla ni
    llevársela solo), no para la firma rutinaria. Ofrecerlo aquí daría una
    sensación de control que la operación real desmiente.
"""
from __future__ import annotations

import os
import json
import hashlib
from datetime import datetime

try:
    import quipu  # paquete PyPI: quipu-crypto
    _DISPONIBLE = True
except Exception:
    quipu = None
    _DISPONIBLE = False


def _sk_hex() -> str:
    return os.getenv("QUIPU_SIGNING_KEY", "")


def _vk_hex() -> str:
    return os.getenv("QUIPU_VERIFYING_KEY", "")


def _modo() -> str:
    """Perfil de custodia: `entorno` (por defecto) o `hsm`."""
    return os.getenv("QUIPU_CUSTODIA", "entorno").strip().lower()


# El custodio HSM sostiene una sesión PKCS#11 y se crea UNA vez: abrir sesión y
# autenticar en cada firma sería lento y castigaría al dispositivo. Se cachea
# por proceso.
#
# OJO: una sesión PKCS#11 está atada al hilo que la abrió —lleva estado de login
# y punteros del módulo— y `CustodioHsm` es `unsendable` por eso. En un servidor
# con varios hilos hay que crear uno por hilo o serializar el acceso. Se declara
# aquí porque es una limitación del recurso, no un descuido que se pueda pulir.
_custodio_hsm = None
_custodio_hsm_error: str | None = None


def _hsm():
    """Devuelve el custodio PKCS#11, o None si no se puede usar (con el motivo
    guardado en `_custodio_hsm_error` para que `por_que_no` pueda explicarlo)."""
    global _custodio_hsm, _custodio_hsm_error
    if _custodio_hsm is not None:
        return _custodio_hsm
    if not _DISPONIBLE:
        _custodio_hsm_error = "quipu-crypto no está instalado"
        return None
    if not hasattr(quipu, "CustodioHsm"):
        _custodio_hsm_error = (
            "la rueda de quipu-crypto instalada no trae la feature `hsm` "
            "(CustodioHsm no existe). Requiere quipu-crypto >= 0.9.1."
        )
        return None
    modulo = os.getenv("QUIPU_PKCS11_MODULE", "")
    ed = os.getenv("QUIPU_PKCS11_ED25519", "")
    ml = os.getenv("QUIPU_PKCS11_MLDSA", "")
    if not (modulo and ed and ml):
        _custodio_hsm_error = (
            "faltan QUIPU_PKCS11_MODULE, QUIPU_PKCS11_ED25519 o QUIPU_PKCS11_MLDSA"
        )
        return None
    try:
        _custodio_hsm = quipu.CustodioHsm(
            modulo, ed, ml, pin=os.getenv("QUIPU_PKCS11_PIN") or None
        )
        _custodio_hsm_error = None
    except Exception as e:  # noqa: BLE001 — se informa, no se oculta
        _custodio_hsm_error = f"no se pudo abrir el dispositivo PKCS#11: {e}"
        _custodio_hsm = None
    return _custodio_hsm


def disponible() -> bool:
    """True si la firma se puede usar con el perfil de custodia configurado."""
    if not _DISPONIBLE:
        return False
    if _modo() == "hsm":
        return _hsm() is not None
    return bool(_sk_hex()) and bool(_vk_hex())


def por_que_no() -> str:
    """Explica por qué la firma NO está disponible.

    Existe porque «desactivado» sin motivo es indistinguible de «mal
    configurado», y la diferencia importa: en el primer caso no hay nada que
    hacer y en el segundo alguien creía tener firma y no la tiene.
    """
    if disponible():
        return ""
    if not _DISPONIBLE:
        return "quipu-crypto no está instalado (pip install -r requirements-firma.txt)"
    if _modo() == "hsm":
        return _custodio_hsm_error or "el dispositivo PKCS#11 no está disponible"
    faltan = [n for n, v in (("QUIPU_SIGNING_KEY", _sk_hex()),
                             ("QUIPU_VERIFYING_KEY", _vk_hex())) if not v]
    return f"faltan variables de entorno: {', '.join(faltan)}" if faltan else "motivo desconocido"


def clave_de_verificacion() -> bytes | None:
    """La clave pública con la que verificar. En HSM se lee del dispositivo, así
    que no puede desincronizarse de la privada; en modo entorno viene del .env."""
    if _modo() == "hsm":
        c = _hsm()
        return bytes(c.verifying_key()) if c is not None else None
    return bytes.fromhex(_vk_hex()) if _vk_hex() else None


def generar_claves() -> tuple[str, str]:
    """Genera un par de claves de firma. Devuelve (verifying_key_hex,
    signing_key_hex) para el .env. La signing_key es SECRETA."""
    if not _DISPONIBLE:
        raise RuntimeError("quipu-crypto no está instalado (pip install quipu-crypto)")
    vk, sk = quipu.generate_signing_keypair()
    return vk.hex(), sk.hex()


def _valor(entrega, campo, default=None):
    """Lee un campo de un objeto ORM (EntregaEsperada) o de un dict."""
    if isinstance(entrega, dict):
        return entrega.get(campo, default)
    return getattr(entrega, campo, default)


def _hash_archivo(ruta) -> str | None:
    """SHA-256 del archivo entregado, si existe: así la firma cubre también el
    CONTENIDO del documento, no solo los metadatos."""
    if not ruta or not os.path.isfile(ruta):
        return None
    h = hashlib.sha256()
    with open(ruta, "rb") as f:
        for bloque in iter(lambda: f.read(65536), b""):
            h.update(bloque)
    return h.hexdigest()


def canonico(entrega) -> bytes:
    """Representación DETERMINISTA de los campos auditables de la entrega. Es lo
    que se firma; cualquier cambio posterior en estos campos invalida la
    verificación. `sort_keys` + separadores fijos → mismos bytes para mismos datos."""
    def _iso(v):
        return v.isoformat() if isinstance(v, datetime) else v
    datos = {
        "id": _valor(entrega, "id"),
        "informe_id": _valor(entrega, "informe_id"),
        "usuario_id": _valor(entrega, "usuario_id"),
        "fecha_limite": _iso(_valor(entrega, "fecha_limite_teorica")),
        "fecha_entrega": _iso(_valor(entrega, "fecha_entrega_real")),
        "estado": _valor(entrega, "estado"),
        "archivo_sha256": _hash_archivo(_valor(entrega, "ruta_archivo")),
    }
    return json.dumps(datos, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=False, default=str).encode("utf-8")


def firmar_entrega(entrega) -> str | None:
    """Firma los campos auditables de la entrega. Devuelve el blob firmado
    (guárdalo junto a la entrega, p.ej. columna `firma`), o None si el servicio
    no está disponible."""
    if not disponible():
        return None
    datos = canonico(entrega)
    if _modo() == "hsm":
        # La clave privada NUNCA cruza a Python: la firma se calcula dentro del
        # dispositivo. Es toda la diferencia entre los dos perfiles.
        return _hsm().encode_signed(datos)
    return quipu.encode_signed(datos, bytes.fromhex(_sk_hex()))


def verificar_entrega(entrega, firma: str) -> tuple[bool, str]:
    """Verifica que `firma` corresponde a la entrega ACTUAL. Devuelve
    (ok, motivo). Distingue dos fallos: firma inválida/forjada, y entrega
    alterada tras firmarse."""
    if not disponible():
        return (False, f"servicio de firma no disponible: {por_que_no()}")
    if not firma:
        return (False, "sin firma")
    vk = clave_de_verificacion()
    if vk is None:
        return (False, "no hay clave de verificación configurada")
    try:
        firmado = quipu.decode_verified(firma, vk)
    except Exception:
        return (False, "firma inválida o forjada")
    if firmado == canonico(entrega):
        return (True, "verificada")
    return (False, "la entrega fue alterada tras la firma")
