"""El clausulado que se entrega debe ser el mismo que el de trabajo.

`EULA_BORRADOR.md` es el documento interno (lleva las notas ⚑) y
`MODELO_LICENCIA_INFORMES.md` es lo que ve un tercero. Son dos copias del mismo
texto mantenidas a mano, y ya divergieron una vez: el 2026-07-21 al modelo del
cliente le faltaba la Cláusula 3.5. En un contrato eso no es un descuido de
formato — significa entregar un clausulado distinto del que uno cree.

`verificar_clausulado.py` corre en el CI, pero hasta ahora un guion suelto: si
alguien lo rompía, la suite seguía verde. Aquí queda amarrado, y con la prueba
que de verdad importa — que el verificador **sepa ponerse rojo**.
"""
from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

import pytest

RAIZ = Path(__file__).resolve().parent.parent


def _cargar_verificador():
    ruta = RAIZ / "verificar_clausulado.py"
    if not ruta.exists():
        pytest.skip("verificar_clausulado.py no está en esta copia")
    spec = importlib.util.spec_from_file_location("verificar_clausulado", ruta)
    modulo = importlib.util.module_from_spec(spec)
    sys.modules["verificar_clausulado"] = modulo
    spec.loader.exec_module(modulo)
    return modulo


VC = _cargar_verificador()


def test_los_dos_documentos_concuerdan():
    """El estado real del repositorio. Es la prueba que se pondrá roja el día
    que alguien edite uno solo de los dos archivos."""
    assert VC.main() == 0, "el modelo del cliente divergió del documento de trabajo"


def test_el_modelo_del_cliente_no_lleva_las_notas_internas():
    """Las notas ⚑ son de uso interno: razonan qué se cede y por qué, y algunas
    dicen dónde está flojo el contrato. No pueden viajar al cliente."""
    entregable = (RAIZ / "MODELO_LICENCIA_INFORMES.md").read_text(encoding="utf-8")
    assert "⚑" not in entregable
    assert VC.CORTE_INTERNO not in entregable


def test_la_concesion_condiciona_el_uso_a_las_sedes(tmp_path):
    """El vacío que se cerró: el límite por sedes vivía como casilla del Anexo A
    y no como limitación de la concesión. Sin esto, el Anexo enumera sedes que
    ninguna cláusula convierte en límite."""
    for nombre in ("EULA_BORRADOR.md", "MODELO_LICENCIA_INFORMES.md"):
        cuerpo = VC.cuerpos(RAIZ / nombre, cortar_internas=True)
        assert "Sedes" in cuerpo["3.1"], f"{nombre}: la concesión no acota por sedes"
        assert "Anexo A" in cuerpo["3.1"]
        assert "ampliación" in cuerpo["3.5"].lower(), f"{nombre}: falta el procedimiento"
        assert "3.5.1" in cuerpo, f"{nombre}: falta qué pasa si se usa fuera sin ampliar"


def _documento(clausula_35: str) -> str:
    return (
        "# CONTRATO\n\n"
        "**3.1. Carácter perpetuo y alcance.** Se concede en las Sedes del Anexo A.\n\n"
        f"**3.5. Ampliación del alcance.** {clausula_35}\n\n"
        "**4.1. Restricciones.** No hacer nada raro.\n"
    )


def test_el_verificador_caza_un_texto_distinto_con_el_mismo_titulo(tmp_path):
    """LA prueba de esta batería.

    Comparar solo números y títulos deja pasar la divergencia peligrosa: dos
    cláusulas «3.5 Ampliación del alcance» que fijan plazos distintos. Un
    verificador ciego a eso no informa de nada, y encima da confianza.
    """
    a = tmp_path / "trabajo.md"
    b = tmp_path / "cliente.md"
    a.write_text(_documento("Requiere acuerdo previo en 30 días."), encoding="utf-8")
    b.write_text(_documento("Requiere acuerdo previo en 5 días."), encoding="utf-8")

    ca = VC.cuerpos(a, cortar_internas=True)
    cb = VC.cuerpos(b, cortar_internas=True)
    assert set(ca) == set(cb), "los números coinciden: por eso el chequeo viejo pasaba"
    assert ca["3.5"] != cb["3.5"], "el cambio de plazo debe detectarse"
    assert ca["3.1"] == cb["3.1"], "y no debe señalar las que sí coinciden"


def test_las_notas_internas_y_el_formato_no_cuentan_como_divergencia(tmp_path):
    """El reverso: si marcara diferencias por las notas ⚑ o por dónde cae un
    salto de línea, saldría rojo siempre y alguien lo apagaría."""
    a = tmp_path / "trabajo.md"
    b = tmp_path / "cliente.md"
    a.write_text(
        "**3.5. Ampliación del alcance.** Requiere acuerdo\nprevio.\n\n"
        "> ⚑ Nota interna: aquí está la palanca comercial.\n\n---\n",
        encoding="utf-8",
    )
    b.write_text("**3.5. Ampliación del alcance.** Requiere acuerdo previo.\n", encoding="utf-8")
    assert VC.cuerpos(a, cortar_internas=True)["3.5"] == VC.cuerpos(b, cortar_internas=True)["3.5"]
