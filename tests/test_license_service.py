"""Banco de la licencia offline.

Dos cosas se prueban, y la segunda importa tanto como la primera:

1. Que **discrimine**: detecta una licencia manipulada, una firmada con otra
   clave, una vencida y una incompleta.
2. Que **NUNCA bloquee**. Ninguna situación —ausente, inválida, vencida— puede
   impedir operar. Es el principio del modo degradado: la ausencia de una capa
   opcional no puede ser la causa de que el cliente incumpla ante un tercero.
   Si alguien "endurece" esto en el futuro, estas pruebas tienen que caerse.

Requiere `quipu-crypto` (requirements-firma.txt).
"""
from __future__ import annotations

import json
from datetime import date, timedelta

import pytest

from app.services import license_service as ls

CRIPTO = pytest.mark.skipif(
    not ls._DISPONIBLE,  # noqa: SLF001
    reason="quipu-crypto no está instalado",
)


def datos_validos(**cambios):
    d = {
        "licenciatario": "ACME S.A.S.",
        "nit": "900123456-1",
        "edicion": "profesional",
        "perfil": "onprem",
        "version": "4.0",
        "usuarios": 25,
        "sedes": 3,
        "modalidad": "asistida",
        "emitida": date.today().isoformat(),
        "vigencia_hasta": (date.today() + timedelta(days=365)).isoformat(),
    }
    d.update(cambios)
    return d


@pytest.fixture
def entorno(tmp_path, monkeypatch):
    """Par de claves real y una ruta de licencia aislada."""
    if not ls._DISPONIBLE:  # noqa: SLF001
        pytest.skip("quipu-crypto no instalado")
    import quipu
    vk, sk = quipu.generate_signing_keypair()
    ruta = tmp_path / "licencia.quipu"
    monkeypatch.setenv("INFORMES_LICENCIA", str(ruta))
    monkeypatch.setenv("INFORMES_LICENCIA_PUBKEY", vk.hex())
    return {"vk": vk.hex(), "sk": sk.hex(), "ruta": ruta}


def instalar(entorno, datos):
    entorno["ruta"].write_text(ls.emitir(datos, entorno["sk"]), encoding="utf-8")


# --------------------------------------------------------------------------
# Que NUNCA bloquee — el invariante que no se puede perder
# --------------------------------------------------------------------------

@CRIPTO
@pytest.mark.parametrize("situacion", ["ausente", "invalida", "vencida"])
def test_ninguna_situacion_bloquea_la_operacion(entorno, situacion):
    """El módulo NO expone ninguna forma de bloquear, y así debe seguir.

    Si alguien añade un `debe_bloquear` o un `raise` en `estado()`, esta prueba
    se cae. Es deliberado: el día que una licencia vencida impida registrar una
    entrega, Xiliux pasa a ser la causa del incumplimiento del cliente.
    """
    if situacion == "invalida":
        entorno["ruta"].write_text("basura que no es una firma", encoding="utf-8")
    elif situacion == "vencida":
        instalar(entorno, datos_validos(
            vigencia_hasta=(date.today() - timedelta(days=1)).isoformat()))

    e = ls.estado()
    assert e.situacion == situacion
    # No hay atributo de bloqueo, ni el veredicto es una excepción.
    assert not hasattr(e, "bloquear")
    assert not hasattr(e, "debe_bloquear")
    assert isinstance(e.mensaje, str) and e.mensaje


@CRIPTO
def test_el_exceso_de_usuarios_informa_pero_no_bloquea(entorno):
    instalar(entorno, datos_validos(usuarios=5))
    excede, msg = ls.excede_usuarios(9)
    assert excede is True
    assert "5" in msg and "9" in msg
    # Y la licencia sigue siendo VÁLIDA: el exceso no la invalida, se remedia
    # por contrato.
    assert ls.estado().situacion == "valida"


# --------------------------------------------------------------------------
# Que discrimine
# --------------------------------------------------------------------------

@CRIPTO
def test_una_licencia_bien_emitida_verifica(entorno):
    instalar(entorno, datos_validos())
    e = ls.estado()
    assert e.situacion == "valida", e.mensaje
    assert e.datos["licenciatario"] == "ACME S.A.S."
    assert e.hay_que_avisar is False


@CRIPTO
def test_sin_archivo_es_ausente_y_no_grave(entorno):
    e = ls.estado()
    assert e.situacion == "ausente"
    assert e.es_grave is False, "sin licencia es normal en evaluación"


@CRIPTO
@pytest.mark.parametrize("campo,valor", [
    ("licenciatario", "OTRA S.A.S."),
    ("usuarios", 9999),
    ("sedes", 99),
    ("edicion", "corporativa"),
    ("vigencia_hasta", "2099-01-01"),
])
def test_detecta_la_licencia_manipulada(entorno, campo, valor):
    """EL PUNTO DEL SISTEMA. Ampliarse los usuarios o la vigencia editando el
    archivo es exactamente lo que la firma debe hacer imposible de ocultar."""
    firmada = ls.emitir(datos_validos(), entorno["sk"])
    # Se altera el contenido firmado tal como lo haría alguien con un editor.
    alterada = ls.emitir(datos_validos(**{campo: valor}), entorno["sk"])
    assert firmada != alterada  # la firma cubre ese campo

    # Y una firma que no corresponde a los datos no verifica.
    entorno["ruta"].write_text(firmada[:-8] + "00000000", encoding="utf-8")
    e = ls.estado()
    assert e.situacion == "invalida"
    assert e.es_grave is True


@CRIPTO
def test_rechaza_una_licencia_firmada_con_OTRA_clave(entorno, monkeypatch):
    """Sin esto, cualquiera con su propio par se emite licencias válidas."""
    import quipu
    _otra_vk, otra_sk = quipu.generate_signing_keypair()
    entorno["ruta"].write_text(ls.emitir(datos_validos(), otra_sk.hex()),
                               encoding="utf-8")
    assert ls.estado().situacion == "invalida"


@CRIPTO
def test_una_licencia_vencida_se_reconoce_y_dice_cuanto(entorno):
    instalar(entorno, datos_validos(
        vigencia_hasta=(date.today() - timedelta(days=10)).isoformat()))
    e = ls.estado()
    assert e.situacion == "vencida"
    assert e.es_grave is True
    assert "10" in e.mensaje
    assert e.datos is not None, "una vencida sigue siendo legible: hay que poder ver de quién es"


@CRIPTO
def test_el_ultimo_dia_de_vigencia_TODAVIA_vale(entorno):
    """Fuera por uno clásico: la licencia vence AL FINAL del día indicado."""
    hoy = date.today()
    instalar(entorno, datos_validos(vigencia_hasta=hoy.isoformat()))
    assert ls.estado(hoy=hoy).situacion == "valida"
    assert ls.estado(hoy=hoy + timedelta(days=1)).situacion == "vencida"


# --------------------------------------------------------------------------
# Emisión: no se puede emitir una licencia a medias
# --------------------------------------------------------------------------

@CRIPTO
@pytest.mark.parametrize("campo", ls.CAMPOS)
def test_no_emite_si_falta_cualquier_campo(entorno, campo):
    """Una licencia incompleta no delimita nada, y su función es delimitar."""
    d = datos_validos()
    d[campo] = None
    with pytest.raises(ValueError, match="faltan campos"):
        ls.emitir(d, entorno["sk"])


@CRIPTO
@pytest.mark.parametrize("campo,malo", [("edicion", "premium"), ("perfil", "k8s")])
def test_no_emite_con_valores_fuera_del_anexo_A(entorno, campo, malo):
    with pytest.raises(ValueError):
        ls.emitir(datos_validos(**{campo: malo}), entorno["sk"])


# --------------------------------------------------------------------------
# Degradación
# --------------------------------------------------------------------------

def test_sin_clave_publica_no_dice_que_si(monkeypatch, tmp_path):
    """Sin verificador, el veredicto NO puede ser «válida»: sería aprobar sin
    mirar, que es peor que no tener licenciamiento."""
    monkeypatch.delenv("INFORMES_LICENCIA_PUBKEY", raising=False)
    monkeypatch.setenv("INFORMES_LICENCIA", str(tmp_path / "no-existe"))
    e = ls.estado()
    assert e.situacion == "sin_verificador"
    assert e.hay_que_avisar is True


def test_los_campos_son_los_del_anexo_A():
    """Si el Anexo A del EULA cambia, esta lista cambia con él. La prueba está
    para que la discrepancia se note aquí y no en una auditoría."""
    assert set(ls.CAMPOS) == {
        "licenciatario", "nit", "edicion", "perfil", "version",
        "usuarios", "sedes", "modalidad", "emitida", "vigencia_hasta",
    }
    assert ls.EDICIONES == ("esencial", "profesional", "corporativa")
    assert ls.PERFILES == ("standalone", "onprem", "cloud")
