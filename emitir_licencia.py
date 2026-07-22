"""Emite una licencia firmada de INFORMES. Lo corre EL LICENCIANTE, no el cliente.

    python emitir_licencia.py \
        --licenciatario "ACME S.A.S." --nit 900123456-1 \
        --edicion profesional --perfil onprem \
        --version 4.0 --usuarios 25 --sedes 3 --modalidad asistida \
        --mantenimiento-hasta 2027-07-21 \
        --salida licencia.quipu

La clave de firma se lee de `INFORMES_LICENCIA_SK` (hex). **Es el secreto que
sostiene todo el licenciamiento**: quien la tenga puede emitir licencias válidas.
No vive en el repositorio ni viaja al cliente.

Generar el par una sola vez:

    python -c "import quipu; vk, sk = quipu.generate_signing_keypair(); \
               print('PUBKEY=', vk.hex()); print('SK=', sk.hex())"

La PUBKEY va al `.env` del cliente como `INFORMES_LICENCIA_PUBKEY`.
"""
from __future__ import annotations

import argparse
import os
import sys
from datetime import date

sys.path.append(os.path.dirname(os.path.abspath(__file__)))

from app.services.license_service import EDICIONES, PERFILES, emitir, estado


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--licenciatario", required=True, help="Razón social")
    p.add_argument("--nit", required=True)
    p.add_argument("--edicion", required=True, choices=EDICIONES)
    p.add_argument("--perfil", required=True, choices=PERFILES)
    p.add_argument("--version", default="4.0", help="Versión licenciada")
    p.add_argument("--usuarios", required=True, type=int, help="Usuarios Autorizados")
    p.add_argument("--sedes", required=True, type=int, help="Sedes / entornos")
    p.add_argument("--modalidad", default="asistente",
                   choices=("asistente", "asistida", "air-gapped"))
    p.add_argument("--mantenimiento-hasta", required=True, help="AAAA-MM-DD. Fin del\n                   PERÍODO DE MANTENIMIENTO, no de la licencia: el derecho de uso es\n                   perpetuo (Cláusulas 3.1, 3.2 y 7.6 del EULA).")
    p.add_argument("--salida", default="licencia.quipu")
    args = p.parse_args()

    sk = os.getenv("INFORMES_LICENCIA_SK", "")
    if not sk:
        print("ERROR: falta INFORMES_LICENCIA_SK (clave de firma en hex).",
              file=sys.stderr)
        return 1

    try:
        date.fromisoformat(args.mantenimiento_hasta)
    except ValueError:
        print("ERROR: --mantenimiento-hasta debe ser AAAA-MM-DD.", file=sys.stderr)
        return 1

    datos = {
        "licenciatario": args.licenciatario,
        "nit": args.nit,
        "edicion": args.edicion,
        "perfil": args.perfil,
        "version": args.version,
        "usuarios": args.usuarios,
        "sedes": args.sedes,
        "modalidad": args.modalidad,
        "emitida": date.today().isoformat(),
        "mantenimiento_hasta": args.mantenimiento_hasta,
    }

    try:
        firmada = emitir(datos, sk)
    except Exception as e:  # noqa: BLE001
        print(f"ERROR al emitir: {e}", file=sys.stderr)
        return 1

    with open(args.salida, "w", encoding="utf-8") as f:
        f.write(firmada)
    print(f"Licencia escrita en {args.salida}")

    # Verificar lo EMITIDO, no lo que creemos haber emitido: si la clave pública
    # configurada aquí no corresponde a la privada que firmó, más vale saberlo
    # ahora que cuando el cliente abra la aplicación.
    if os.getenv("INFORMES_LICENCIA_PUBKEY"):
        anterior = os.getenv("INFORMES_LICENCIA")
        os.environ["INFORMES_LICENCIA"] = args.salida
        try:
            e = estado()
            print(f"Comprobación: {e.situacion} — {e.mensaje}")
            if e.situacion != "valida":
                print("AVISO: la licencia recién emitida NO verifica con la "
                      "INFORMES_LICENCIA_PUBKEY configurada.", file=sys.stderr)
                return 1
        finally:
            if anterior is None:
                os.environ.pop("INFORMES_LICENCIA", None)
            else:
                os.environ["INFORMES_LICENCIA"] = anterior
    else:
        print("AVISO: sin INFORMES_LICENCIA_PUBKEY no se pudo COMPROBAR lo "
              "emitido. Configúrala para que este script se verifique a sí mismo.")

    return 0


if __name__ == "__main__":
    sys.exit(main())
