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

Claves (setup una vez):
    from app.services.signing_service import generar_claves
    vk, sk = generar_claves()   # -> pon vk/sk en el .env:
    #   QUIPU_VERIFYING_KEY=<vk>   (pública)
    #   QUIPU_SIGNING_KEY=<sk>     (SECRETA; custodia = responsabilidad del operador)
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


def disponible() -> bool:
    """True si quipu-crypto está instalado y hay par de claves configurado."""
    return _DISPONIBLE and bool(_sk_hex()) and bool(_vk_hex())


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
    sk = bytes.fromhex(_sk_hex())
    return quipu.encode_signed(canonico(entrega), sk)


def verificar_entrega(entrega, firma: str) -> tuple[bool, str]:
    """Verifica que `firma` corresponde a la entrega ACTUAL. Devuelve
    (ok, motivo). Distingue dos fallos: firma inválida/forjada, y entrega
    alterada tras firmarse."""
    if not disponible():
        return (False, "servicio de firma no disponible")
    if not firma:
        return (False, "sin firma")
    vk = bytes.fromhex(_vk_hex())
    try:
        firmado = quipu.decode_verified(firma, vk)
    except Exception:
        return (False, "firma inválida o forjada")
    if firmado == canonico(entrega):
        return (True, "verificada")
    return (False, "la entrega fue alterada tras la firma")
