"""Banco del servicio de firma de entregas.

Lo que se prueba NO es que una firma válida verifique: eso lo cumpliría también
una función que devolviera siempre `True`. Se prueba que **DISCRIMINA** — que
detecta cada forma concreta de alteración que la firma existe para detectar.

Requiere `quipu-crypto` (ver requirements-firma.txt). Si no está instalado, las
pruebas que necesitan criptografía real se saltan y las de degradación se
ejecutan igual: esa degradación es justamente lo que hay que garantizar cuando
la librería falta.

    pip install -r requirements-firma.txt
    python -m pytest tests/test_signing_service.py -v
"""
from __future__ import annotations

import hashlib
import os
from datetime import datetime, timedelta

import pytest

from app.services import signing_service as fs

CRIPTO = pytest.mark.skipif(
    not fs._DISPONIBLE,  # noqa: SLF001 — es el interruptor del propio módulo
    reason="quipu-crypto no está instalado (pip install -r requirements-firma.txt)",
)


def test_donde_la_cripto_DEBE_estar_su_ausencia_es_un_FALLO():
    """Saltarse el banco entero es indistinguible de aprobarlo.

    Sin `quipu-crypto` instalado, las pruebas de firma se marcan como omitidas y
    la corrida sale VERDE sin haber ejercitado ni una línea de criptografía. En
    la máquina de un desarrollador eso es razonable —la dependencia es opcional
    por diseño—; en integración continua es un verificador que no puede ponerse
    rojo.

    De ahí `EXIGIR_CRIPTO=1`: donde la firma tiene que estar de verdad, su
    ausencia se anuncia en vez de disolverse en un «omitida» que nadie lee.
    """
    if os.getenv("EXIGIR_CRIPTO") != "1":
        pytest.skip("solo se exige donde la dependencia debe estar instalada")
    assert fs._DISPONIBLE, (  # noqa: SLF001
        "EXIGIR_CRIPTO=1 pero quipu-crypto no está instalado: el banco de firma "
        "se habría saltado entero y la corrida habría salido verde. "
        f"Motivo: {fs.por_que_no()}"
    )


def entrega_base(tmp_path=None, **cambios):
    """Una entrega como dict: `canonico` acepta dict u objeto ORM indistintamente,
    así el banco no necesita base de datos."""
    datos = {
        "id": 42,
        "informe_id": 7,
        "usuario_id": 3,
        "fecha_limite_teorica": datetime(2026, 7, 1, 12, 0, 0),
        "fecha_entrega_real": datetime(2026, 6, 30, 9, 30, 0),
        "estado": "CUMPLIDO",
        "ruta_archivo": None,
    }
    datos.update(cambios)
    return datos


@pytest.fixture
def claves(monkeypatch):
    """Par de claves real, inyectado por entorno como en producción."""
    if not fs._DISPONIBLE:  # noqa: SLF001
        pytest.skip("quipu-crypto no instalado")
    vk, sk = fs.generar_claves()
    monkeypatch.setenv("QUIPU_VERIFYING_KEY", vk)
    monkeypatch.setenv("QUIPU_SIGNING_KEY", sk)
    return vk, sk


# --------------------------------------------------------------------------
# Degradación: sin librería o sin claves, el sistema sigue funcionando
# --------------------------------------------------------------------------

def test_sin_claves_no_esta_disponible(monkeypatch):
    monkeypatch.delenv("QUIPU_SIGNING_KEY", raising=False)
    monkeypatch.delenv("QUIPU_VERIFYING_KEY", raising=False)
    assert fs.disponible() is False


def test_sin_claves_firmar_devuelve_none_y_no_revienta(monkeypatch):
    """Una entrega SIEMPRE debe poder registrarse. Que la firma sea un extra no
    puede convertirse en un motivo para no poder entregar."""
    monkeypatch.delenv("QUIPU_SIGNING_KEY", raising=False)
    monkeypatch.delenv("QUIPU_VERIFYING_KEY", raising=False)
    assert fs.firmar_entrega(entrega_base()) is None


def test_sin_claves_verificar_no_dice_que_si(monkeypatch):
    """Ante la duda, NO verificada. Un servicio caído no puede producir un
    veredicto positivo: sería peor que no tener firma."""
    monkeypatch.delenv("QUIPU_SIGNING_KEY", raising=False)
    monkeypatch.delenv("QUIPU_VERIFYING_KEY", raising=False)
    ok, motivo = fs.verificar_entrega(entrega_base(), "lo que sea")
    assert ok is False
    assert "no disponible" in motivo


# --------------------------------------------------------------------------
# La forma canónica: determinista y sensible a lo que importa
# --------------------------------------------------------------------------

def test_la_forma_canonica_es_determinista():
    assert fs.canonico(entrega_base()) == fs.canonico(entrega_base())


@pytest.mark.parametrize("campo,valor", [
    ("id", 43),
    ("informe_id", 8),
    ("usuario_id", 4),
    ("fecha_limite_teorica", datetime(2026, 7, 2, 12, 0, 0)),
    ("fecha_entrega_real", datetime(2026, 6, 30, 9, 31, 0)),
    ("estado", "EXTEMPORANEO"),
])
def test_cambiar_cualquier_campo_auditable_cambia_la_forma_canonica(campo, valor):
    """Si algún campo NO alterara la forma canónica, la firma no lo protegería y
    se podría cambiar sin invalidar nada. Uno por uno, sin excepciones."""
    assert fs.canonico(entrega_base()) != fs.canonico(entrega_base(**{campo: valor}))


def test_la_forma_canonica_incluye_el_hash_del_archivo(tmp_path):
    """La firma cubre el CONTENIDO del documento, no solo sus metadatos: si no,
    se podría sustituir el archivo entregado dejando la firma válida."""
    f = tmp_path / "informe.pdf"
    f.write_bytes(b"contenido original")
    con_archivo = fs.canonico(entrega_base(ruta_archivo=str(f)))

    f.write_bytes(b"contenido SUSTITUIDO")
    tras_sustituir = fs.canonico(entrega_base(ruta_archivo=str(f)))

    assert con_archivo != tras_sustituir
    assert hashlib.sha256(b"contenido original").hexdigest().encode() in con_archivo


def test_archivo_inexistente_no_revienta():
    """Una entrega sin archivo, o con la ruta rota, tiene que poder firmarse."""
    assert fs._hash_archivo("/no/existe/nada.pdf") is None  # noqa: SLF001
    assert fs._hash_archivo(None) is None  # noqa: SLF001


# --------------------------------------------------------------------------
# Con criptografía real: que DISCRIMINE
# --------------------------------------------------------------------------

@CRIPTO
def test_firma_y_verifica_la_misma_entrega(claves):
    e = entrega_base()
    firma = fs.firmar_entrega(e)
    assert firma
    ok, motivo = fs.verificar_entrega(e, firma)
    assert ok is True, motivo


@CRIPTO
def test_sin_firma_no_verifica(claves):
    ok, motivo = fs.verificar_entrega(entrega_base(), "")
    assert ok is False
    assert motivo == "sin firma"


@CRIPTO
@pytest.mark.parametrize("campo,valor", [
    ("estado", "CUMPLIDO_A_MANO"),
    ("fecha_entrega_real", datetime(2026, 6, 25, 8, 0, 0)),
    ("fecha_limite_teorica", datetime(2026, 8, 1, 12, 0, 0)),
    ("usuario_id", 99),
])
def test_detecta_que_la_entrega_se_alteró_tras_firmar(claves, campo, valor):
    """EL PUNTO ENTERO DEL SISTEMA. Cambiar la fecha de entrega o el estado
    después de firmar es exactamente el fraude que esto debe hacer imposible de
    ocultar — y son los dos campos que alguien tendría motivo para tocar."""
    e = entrega_base()
    firma = fs.firmar_entrega(e)

    alterada = entrega_base(**{campo: valor})
    ok, motivo = fs.verificar_entrega(alterada, firma)

    assert ok is False
    assert motivo == "la entrega fue alterada tras la firma"


@CRIPTO
def test_detecta_la_sustitucion_del_archivo_entregado(claves, tmp_path):
    """Entregar un documento, firmarlo y sustituir el archivo después."""
    f = tmp_path / "informe.pdf"
    f.write_bytes(b"el informe que se entrego")
    e = entrega_base(ruta_archivo=str(f))
    firma = fs.firmar_entrega(e)
    assert fs.verificar_entrega(e, firma)[0] is True

    f.write_bytes(b"otro informe puesto despues")
    ok, motivo = fs.verificar_entrega(e, firma)
    assert ok is False
    assert motivo == "la entrega fue alterada tras la firma"


@CRIPTO
def test_rechaza_una_firma_forjada(claves):
    e = entrega_base()
    firma = fs.firmar_entrega(e)
    forjada = firma[:-4] + ("0000" if not firma.endswith("0000") else "1111")
    ok, motivo = fs.verificar_entrega(e, forjada)
    assert ok is False
    assert motivo in ("firma inválida o forjada", "la entrega fue alterada tras la firma")


@CRIPTO
def test_rechaza_la_firma_de_OTRO_par_de_claves(claves, monkeypatch):
    """Firmar con una clave y verificar con otra debe fallar. Es lo que impide
    que alguien con su propio par se fabrique entregas 'verificadas'."""
    e = entrega_base()
    firma = fs.firmar_entrega(e)

    otra_vk, _otra_sk = fs.generar_claves()
    monkeypatch.setenv("QUIPU_VERIFYING_KEY", otra_vk)

    ok, _ = fs.verificar_entrega(e, firma)
    assert ok is False


@CRIPTO
def test_dos_entregas_distintas_dan_firmas_distintas(claves):
    a = fs.firmar_entrega(entrega_base(id=1))
    b = fs.firmar_entrega(entrega_base(id=2))
    assert a != b


# --------------------------------------------------------------------------
# Selector de custodia: entorno vs HSM
# --------------------------------------------------------------------------

def test_el_modo_por_defecto_es_entorno(monkeypatch):
    monkeypatch.delenv("QUIPU_CUSTODIA", raising=False)
    assert fs._modo() == "entorno"  # noqa: SLF001


def test_el_modo_no_distingue_mayusculas_ni_espacios(monkeypatch):
    monkeypatch.setenv("QUIPU_CUSTODIA", "  HSM  ")
    assert fs._modo() == "hsm"  # noqa: SLF001


def test_modo_hsm_sin_configurar_no_esta_disponible_y_LO_EXPLICA(monkeypatch):
    """Un HSM mal configurado NO puede parecer «firma desactivada»: en ese caso
    alguien cree tener firma y no la tiene. El motivo tiene que ser explícito."""
    monkeypatch.setenv("QUIPU_CUSTODIA", "hsm")
    for v in ("QUIPU_PKCS11_MODULE", "QUIPU_PKCS11_ED25519", "QUIPU_PKCS11_MLDSA"):
        monkeypatch.delenv(v, raising=False)
    monkeypatch.setattr(fs, "_custodio_hsm", None)
    monkeypatch.setattr(fs, "_custodio_hsm_error", None)

    assert fs.disponible() is False
    motivo = fs.por_que_no()
    assert "QUIPU_PKCS11" in motivo or "hsm" in motivo.lower(), motivo


def test_modo_hsm_no_cae_de_vuelta_a_la_variable_de_entorno(monkeypatch, claves):
    """Si alguien pide custodia en HSM y el HSM no está, el sistema NO debe
    firmar con la clave del .env a escondidas: sería degradar la garantía justo
    donde se prometió la más fuerte, y sin avisar (directiva 20)."""
    monkeypatch.setenv("QUIPU_CUSTODIA", "hsm")
    for v in ("QUIPU_PKCS11_MODULE", "QUIPU_PKCS11_ED25519", "QUIPU_PKCS11_MLDSA"):
        monkeypatch.delenv(v, raising=False)
    monkeypatch.setattr(fs, "_custodio_hsm", None)
    monkeypatch.setattr(fs, "_custodio_hsm_error", None)

    # Las claves de entorno SÍ están puestas por la fixture `claves`.
    assert fs.firmar_entrega(entrega_base()) is None


def test_por_que_no_calla_cuando_si_esta_disponible(claves, monkeypatch):
    monkeypatch.delenv("QUIPU_CUSTODIA", raising=False)
    assert fs.disponible() is True
    assert fs.por_que_no() == ""


def test_la_clave_de_verificacion_en_modo_entorno_sale_del_env(claves, monkeypatch):
    monkeypatch.delenv("QUIPU_CUSTODIA", raising=False)
    vk, _sk = claves
    assert fs.clave_de_verificacion() == bytes.fromhex(vk)


# --------------------------------------------------------------------------
# La firma dice a QUÉ OBLIGACIÓN pertenece, y las firmas viejas siguen valiendo
# --------------------------------------------------------------------------

def entrega_con_obligacion(**cambios):
    datos = entrega_base(
        fecha_vencimiento_nominal=datetime(2026, 1, 1, 23, 59),
        tipo_ajuste="ANTICIPADO")
    datos.update(cambios)
    return datos


def test_lo_firmado_incluye_la_obligacion_y_el_tipo_de_ajuste():
    """Una firma con fecha límite 31 de diciembre no dice, por sí sola, si
    acredita la obligación de diciembre entregada a tiempo o la de enero
    entregada por adelantado. Con el día nominal dentro, sí."""
    import json

    datos = json.loads(fs.canonico(entrega_con_obligacion()).decode())
    assert datos["v"] == 2
    assert datos["fecha_vencimiento_nominal"] == "2026-01-01T23:59:00"
    assert datos["tipo_ajuste"] == "ANTICIPADO"


def test_el_contenido_v1_se_puede_reconstruir_tal_cual_se_firmo():
    """Sin esto, añadir un campo convertiría todas las firmas anteriores en «la
    entrega fue alterada»: una acusación falsa de manipulación, que es peor que
    no firmar."""
    import json

    v1 = json.loads(fs.canonico(entrega_con_obligacion(), version=1).decode())
    assert "v" not in v1
    assert "fecha_vencimiento_nominal" not in v1
    assert "tipo_ajuste" not in v1


def test_una_firma_v1_SIGUE_verificando_tras_el_cambio_de_formato(claves):
    """La prueba que de verdad importa: una entrega firmada con el formato
    anterior tiene que seguir dando «verificada»."""
    e = entrega_con_obligacion()
    firma_v1 = fs.quipu.encode_signed(fs.canonico(e, version=1),
                                      bytes.fromhex(fs._sk_hex()))
    ok, motivo = fs.verificar_entrega(e, firma_v1)
    assert ok, motivo


def test_una_firma_v2_verifica(claves):
    e = entrega_con_obligacion()
    ok, motivo = fs.verificar_entrega(e, fs.firmar_entrega(e))
    assert ok, motivo


@pytest.mark.parametrize("campo,valor", [
    ("fecha_vencimiento_nominal", datetime(2026, 2, 1, 23, 59)),
    ("tipo_ajuste", "GRACIA"),
])
def test_alterar_la_obligacion_tras_firmar_se_DETECTA(claves, campo, valor):
    """Si estos campos no estuvieran realmente firmados, las pruebas de arriba
    serían decorativas: comprobarían un JSON que a nadie le consta."""
    e = entrega_con_obligacion()
    firma = fs.firmar_entrega(e)
    e[campo] = valor
    ok, motivo = fs.verificar_entrega(e, firma)
    assert not ok and "alterada" in motivo, motivo


def test_una_version_desconocida_no_se_llama_ALTERADA():
    """Una firma hecha por una versión POSTERIOR del producto no es una entrega
    manipulada. Confundirlas acusaría al cliente de algo que no hizo."""
    import json

    assert fs._version_firmada(json.dumps({"v": 1}).encode()) == 1
    assert fs._version_firmada(json.dumps({"v": 2}).encode()) == 2
    assert fs._version_firmada(json.dumps({"id": 1}).encode()) == 1, "sin `v` es v1"
    assert fs._version_firmada(json.dumps({"v": 99}).encode()) is None
    assert fs._version_firmada(b"no es json") is None
    assert fs._version_firmada(json.dumps(["lista"]).encode()) is None
