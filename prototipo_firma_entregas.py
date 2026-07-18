"""
Prototipo de firma de entregas con Quipu — demostración de extremo a extremo.

Ejecuta:
    pip install quipu-crypto
    python prototipo_firma_entregas.py

Muestra: firmar una entrega, verificarla intacta (OK), detectar una entrega
alterada tras la firma, y rechazar una firma forjada.
"""
import os
import sys
from datetime import datetime

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from app.services import signing_service as fs  # noqa: E402


def main():
    if not fs._DISPONIBLE:
        print("❌ quipu-crypto no está instalado. Ejecuta: pip install quipu-crypto")
        sys.exit(1)

    # 1. Setup: generar claves y ponerlas en el entorno (en producción irían al .env).
    vk, sk = fs.generar_claves()
    os.environ["QUIPU_VERIFYING_KEY"] = vk
    os.environ["QUIPU_SIGNING_KEY"] = sk
    print(f"✅ Claves generadas (vk {len(vk)//2} B, sk {len(sk)//2} B). "
          f"disponible={fs.disponible()}")

    # 2. Una entrega (dict; funciona igual con un EntregaEsperada real).
    entrega = {
        "id": 42,
        "informe_id": 7,
        "usuario_id": 3,
        "fecha_limite_teorica": datetime(2026, 3, 31, 23, 59),
        "fecha_entrega_real": datetime(2026, 3, 30, 10, 15),
        "estado": "CUMPLIDO",
        "ruta_archivo": None,
    }

    # 3. Firmar.
    firma = fs.firmar_entrega(entrega)
    print(f"\n✍️  Entrega #42 firmada. Blob de firma ({len(firma)} símbolos): "
          f"{firma[:48]}…")

    # 4. Verificar la entrega intacta -> debe pasar.
    ok, motivo = fs.verificar_entrega(entrega, firma)
    print(f"\n🔎 Verificar intacta:      {ok}  ({motivo})")
    assert ok, "una entrega intacta debería verificar"

    # 5. Alterar un campo tras la firma -> debe detectarse.
    alterada = dict(entrega, estado="PENDIENTE")   # alguien 'des-cumple' la entrega
    ok, motivo = fs.verificar_entrega(alterada, firma)
    print(f"🔎 Verificar alterada:     {ok}  ({motivo})")
    assert not ok, "una entrega alterada NO debería verificar"

    # 6. Firma forjada -> debe rechazarse.
    ok, motivo = fs.verificar_entrega(entrega, "esto-no-es-una-firma-valida")
    print(f"🔎 Verificar firma falsa:  {ok}  ({motivo})")
    assert not ok, "una firma forjada NO debería verificar"

    print("\n✅ Prototipo OK: firma verificable, detecta alteración y rechaza forjadas.")


if __name__ == "__main__":
    main()
