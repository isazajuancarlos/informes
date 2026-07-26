#!/usr/bin/env python3
"""Simulación completa de la operación en una EMPRESA PRIVADA.

Por qué existe
--------------
El producto es un motor genérico: los campos, las reglas y las variables son
configuración de cada cliente, no columnas del esquema. Eso es fácil de decir y
difícil de creer. Este guion lo demuestra montando de cero una operación que no
comparte NADA con el caso que originó el sistema: otro sector, otro vocabulario,
otras obligaciones, otras periodicidades, ningún dato real.

Y de paso sirve de auditoría: recorre calendario, generación de entregas,
archivos, comentarios, notificaciones y alertas, y REPORTA lo que encuentra mal.

    python demo_empresa_privada.py --meses 6
    python demo_empresa_privada.py --meses 6 --reset

La empresa es ficticia. Cualquier parecido con una real es casualidad.
"""
import argparse
import os
import random
import sys
from datetime import datetime, timedelta

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from app.core.database import Base, SessionLocal, engine  # noqa: E402
from app.models.meta_tables import (  # noqa: E402
    Comentario, ConjuntoDatos, EntregaEsperada, Informe, MetaRegla, MetaVariable,
    NotificacionApp, Participacion, Stakeholder, UnidadOrganizacional, Usuario,
    ROL_DESTINO, ROL_EJECUTA, ROL_ORIGEN, ROL_SUPERVISA, ROL_USUARIO,
)
from app.services.scheduler import GeneradorTareas  # noqa: E402


def vencimientos_de(informe, desde, hasta):
    """Los vencimientos de un informe, pedidos al MOTOR DE PRODUCCIÓN.

    Es el mismo que usa el planificador. Una demo que calcule las fechas por su
    cuenta enseña un sistema que no existe: antes usaba un segundo motor con
    otro esquema de regla, y las reglas que mostraba habrían generado cero
    tareas en la instalación del cliente.
    """
    salida, dia = [], desde
    while dia <= hasta:
        salida += GeneradorTareas.obtener_vencimientos_para_fecha(
            dia, informe.recurrencia_json or {})
        dia += timedelta(days=1)
    return salida

SEMILLA = 20260719  # reproducible: la misma demo en cada corrida
HALLAZGOS: list[str] = []


def hallazgo(texto: str):
    HALLAZGOS.append(texto)
    print(f"   🔎 {texto}")


# =====================================================================
# 1. LA EMPRESA
# =====================================================================
# Manufactura exportadora. Ni salud, ni sector público, ni una sola palabra
# del vocabulario del caso original: si el motor sirve aquí, es genérico.

UNIDADES = [
    ("Gerencia General", "GER"),
    ("Operaciones y Planta", "OPE"),
    ("Seguridad y Salud en el Trabajo", "SST"),
    ("Financiera", "FIN"),
    ("Aseguramiento de Calidad", "CAL"),
]

# (nombre, email, rol_sistema, unidad)
PERSONAS = [
    ("Mariana Restrepo", "gerencia@andina-demo.co", "ADMIN", "Gerencia General"),
    ("Camilo Vélez", "operaciones@andina-demo.co", "SUPERVISOR", "Operaciones y Planta"),
    ("Lucía Grajales", "sst@andina-demo.co", "OPERADOR", "Seguridad y Salud en el Trabajo"),
    ("Andrés Pineda", "financiera@andina-demo.co", "OPERADOR", "Financiera"),
    ("Sofía Marín", "calidad@andina-demo.co", "OPERADOR", "Aseguramiento de Calidad"),
    ("Julián Ossa", "auxiliar@andina-demo.co", "OPERADOR", "Financiera"),
]

# Terceros: entidades con las que la empresa intercambia información.
TERCEROS = [
    ("ARL Seguros del Norte", "ASEGURADORA"),
    ("Banco de Occidente Andino", "FINANCIERO"),
    ("Grupo Nordkapp AB (casa matriz)", "MATRIZ"),
    ("Nordics Retail AB (cliente)", "CLIENTE"),
    ("LabAnalítica S.A.S.", "PROVEEDOR"),
    ("Ente certificador ISO", "CERTIFICADOR"),
]

CONJUNTOS = [
    "Seguridad y salud en el trabajo",
    "Información financiera",
    "Calidad y producto",
    "Relación con clientes",
    "Sostenibilidad",
]

# Variables del cliente: NO son columnas del esquema, son filas configurables.
# Esta es la prueba de adaptabilidad: otro cliente define otras y el código
# no cambia.
VARIABLES = [
    ("severidad_accidente", "Severidad del accidente", "TEXTO", True),
    ("horas_hombre", "Horas-hombre trabajadas", "NUMERO", True),
    ("moneda_reporte", "Moneda del reporte", "TEXTO", True),
    ("lote_produccion", "Lote de producción", "TEXTO", False),
    ("cumple_covenant", "¿Cumple el covenant?", "BOOLEANO", True),
]

REGLAS = [
    ("severidad_accidente", "Severidad grave escala a gerencia", "IGUAL",
     {"valor": "GRAVE"}, "NOTIFICAR", {"destino": "Gerencia General"}),
    ("cumple_covenant", "Incumplir covenant bloquea el envío", "IGUAL",
     {"valor": False}, "BLOQUEAR", {"motivo": "Requiere visto bueno de Gerencia"}),
    ("horas_hombre", "Horas-hombre en cero es sospechoso", "MENOR_QUE",
     {"valor": 1}, "ADVERTIR", {"mensaje": "Revise el dato: no puede ser cero"}),
]

# --- Las obligaciones -------------------------------------------------
# (nombre, descripción, conjunto, unidad, recurrencia, origen, destinos,
#  usuarios, supervisor, extensiones, confidencial, autorizador)
#: Las reglas se escriben en el MISMO formato que entiende el planificador de
#: producción, y la demo genera sus entregas con ese planificador.
#:
#: Antes no era así, y la demo era un teatro: sus reglas usaban un esquema
#: distinto (`freq`/`bymonthday`) que el planificador real ignora —generaban CERO
#: vencimientos y el validador las habría rechazado—, y las entregas se
#: insertaban a mano con fechas calculadas por un segundo motor de calendario.
#: Se enseñaba a un cliente un sistema que, tal cual, no habría producido ni una
#: sola tarea.
MESES_TODOS = list(range(1, 13))
MESES_TRIMESTRE = [3, 6, 9, 12]

#: Cada obligación declara además `vencimientos_al_ano`: cuántos DEBE producir.
#:
#: Es una declaración de intención, deliberadamente separada de la regla. La
#: primera versión de la prueba derivaba ese número de la propia regla —
#: `len(regla["meses"])`— y por tanto no podía detectar que una obligación
#: trimestral se volviera mensual: al cambiar la regla cambiaba también lo
#: esperado. Una prueba circular pasa siempre, que es la peor manera de pasar.

#: Vigencia de las obligaciones de la demo. Se fija lejos hacia atrás para que el
#: histórico simulado quede dentro.
INICIO = "2020-01-01"

OBLIGACIONES = [
    dict(
        nombre="Reporte mensual de accidentalidad laboral",
        descripcion="Novedades de accidentes y enfermedad laboral del periodo.",
        conjunto="Seguridad y salud en el trabajo",
        unidad="Seguridad y Salud en el Trabajo",
        vencimientos_al_ano=12,
        recurrencia={"modo": "ESPECIFICO", "meses": MESES_TODOS, "dia_corte": 5,
                     "ajuste_habil": True, "fecha_inicio": INICIO},
        origen="sst@andina-demo.co",
        destinos_externos=["ARL Seguros del Norte"],
        usuarios_externos=[],
        supervisor="operaciones@andina-demo.co",
        extensiones=["pdf", "xlsx"],
        confidencial=False,
        autorizador="Director de Operaciones",
    ),
    dict(
        nombre="Estados financieros a casa matriz",
        descripcion="Balance y resultados del mes, en moneda de reporte del grupo.",
        conjunto="Información financiera",
        unidad="Financiera",
        vencimientos_al_ano=12,
        recurrencia={"modo": "ESPECIFICO", "meses": MESES_TODOS, "dia_corte": 10,
                     "ajuste_habil": True, "fecha_inicio": INICIO},
        origen="financiera@andina-demo.co",
        destinos_externos=["Grupo Nordkapp AB (casa matriz)"],
        usuarios_externos=[],
        supervisor="gerencia@andina-demo.co",
        extensiones=["xlsx", "pdf"],
        confidencial=True,
        autorizador="Gerente General",
    ),
    dict(
        nombre="Certificación trimestral de covenants",
        descripcion="Certificación de indicadores financieros pactados con el banco.",
        conjunto="Información financiera",
        unidad="Financiera",
        vencimientos_al_ano=4,
        recurrencia={"modo": "ESPECIFICO", "meses": MESES_TRIMESTRE, "dia_corte": 15,
                     "ajuste_habil_anterior": True, "fecha_inicio": INICIO},
        origen="financiera@andina-demo.co",
        destinos_externos=["Banco de Occidente Andino"],
        usuarios_externos=[],
        supervisor="gerencia@andina-demo.co",
        extensiones=["pdf"],
        confidencial=True,
        autorizador="Gerente General",
    ),
    dict(
        nombre="Informe de nivel de servicio al cliente",
        descripcion="Cumplimiento de entregas pactado en contrato de suministro.",
        conjunto="Relación con clientes",
        unidad="Operaciones y Planta",
        vencimientos_al_ano=12,
        recurrencia={"modo": "ESPECIFICO", "meses": MESES_TODOS, "dia_corte": 3,
                     "ajuste_habil": True, "fecha_inicio": INICIO},
        origen="operaciones@andina-demo.co",
        destinos_externos=["Nordics Retail AB (cliente)"],
        usuarios_externos=[],
        supervisor="gerencia@andina-demo.co",
        extensiones=["pdf"],
        confidencial=True,
        autorizador="Gerente General",
    ),
    dict(
        # RECEPCIÓN: aquí la empresa es el DESTINO y el tercero es el ORIGEN.
        nombre="Resultados de laboratorio externo",
        descripcion="Ensayos de materia prima que el laboratorio remite a la planta.",
        conjunto="Calidad y producto",
        unidad="Aseguramiento de Calidad",
        vencimientos_al_ano=12,
        recurrencia={"modo": "ESPECIFICO", "meses": MESES_TODOS, "dia_corte": 20,
                     "ajuste_habil": True, "fecha_inicio": INICIO},
        origen_externo="LabAnalítica S.A.S.",
        destino_interno="calidad@andina-demo.co",
        usuarios_externos=[],
        supervisor="operaciones@andina-demo.co",
        extensiones=["pdf", "csv"],
        confidencial=False,
        autorizador=None,
    ),
    dict(
        nombre="Acta de reunión del comité paritario",
        descripcion="Acta mensual del comité; circula solo dentro de la empresa.",
        conjunto="Seguridad y salud en el trabajo",
        unidad="Seguridad y Salud en el Trabajo",
        vencimientos_al_ano=12,
        recurrencia={"modo": "ESPECIFICO", "meses": MESES_TODOS, "dia_corte": 28,
                     "ajuste_habil_anterior": True, "fecha_inicio": INICIO},
        origen="sst@andina-demo.co",
        destinos_internos=["gerencia@andina-demo.co"],
        destinos_externos=[],
        usuarios_externos=[],
        supervisor="operaciones@andina-demo.co",
        extensiones=["pdf", "docx"],
        confidencial=False,
        autorizador=None,
    ),
    dict(
        nombre="Indicadores de calidad para auditoría de certificación",
        descripcion="Tablero de no conformidades y acciones correctivas.",
        conjunto="Calidad y producto",
        unidad="Aseguramiento de Calidad",
        vencimientos_al_ano=4,
        recurrencia={"modo": "ESPECIFICO", "meses": MESES_TRIMESTRE, "dia_corte": 25,
                     "ajuste_habil": True, "fecha_inicio": INICIO},
        origen="calidad@andina-demo.co",
        destinos_externos=[],
        usuarios_externos=["Ente certificador ISO"],
        supervisor="operaciones@andina-demo.co",
        extensiones=["xlsx"],
        confidencial=False,
        autorizador="Jefe de Calidad",
    ),
    dict(
        nombre="Consumo de energía y agua de planta",
        descripcion="Consumos del periodo para el reporte de sostenibilidad del grupo.",
        conjunto="Sostenibilidad",
        unidad="Operaciones y Planta",
        vencimientos_al_ano=52,
        recurrencia={"modo": "SEMANAL", "dias_semana": [0],
                     "mover_habil": True, "fecha_inicio": INICIO},
        origen="operaciones@andina-demo.co",
        destinos_externos=["Grupo Nordkapp AB (casa matriz)"],
        usuarios_externos=[],
        supervisor="gerencia@andina-demo.co",
        extensiones=["csv"],
        confidencial=False,
        autorizador=None,
    ),
]


# =====================================================================
# 2. MONTAJE
# =====================================================================

def construir(db, meses: int):
    print("\n▶ 1/5  Estructura de la empresa")
    unidades = {}
    for nombre, codigo in UNIDADES:
        u = UnidadOrganizacional(nombre=nombre, codigo=codigo)
        db.add(u)
        unidades[nombre] = u
    db.flush()

    personas = {}
    for nombre, email, rol, unidad in PERSONAS:
        p = Usuario(nombre=nombre, email=email, password_hash="demo-no-usable",
                    rol_sistema=rol, es_admin=(rol == "ADMIN"),
                    unidad_id=unidades[unidad].id, activo=True)
        db.add(p)
        personas[email] = p

    terceros = {}
    for nombre, tipo in TERCEROS:
        t = Stakeholder(nombre=nombre, tipo=tipo,
                        contacto_nombre="Contacto de demostración",
                        email=f"demo+{tipo.lower()}@example.invalid")
        db.add(t)
        terceros[nombre] = t
    db.flush()
    print(f"   {len(unidades)} unidades · {len(personas)} personas · {len(terceros)} terceros")

    print("\n▶ 2/5  Vocabulario del cliente (variables y reglas, no columnas)")
    variables = {}
    for interno, etiqueta, tipo, oblig in VARIABLES:
        v = MetaVariable(nombre_interno=interno, etiqueta_visual=etiqueta,
                         tipo_dato=tipo, es_obligatorio=oblig)
        db.add(v)
        variables[interno] = v
    db.flush()
    for var, nombre, op, cmp_, consec, payload in REGLAS:
        db.add(MetaRegla(variable_id=variables[var].id, nombre_regla=nombre,
                         operador=op, valor_comparacion_json=cmp_,
                         tipo_consecuencia=consec, payload_json=payload))

    conjuntos = {}
    for nombre in CONJUNTOS:
        c = ConjuntoDatos(nombre=nombre, activo=True)
        db.add(c)
        conjuntos[nombre] = c
    db.flush()
    print(f"   {len(variables)} variables · {len(REGLAS)} reglas · {len(conjuntos)} conjuntos")

    print("\n▶ 3/5  Obligaciones y sus flujos")
    informes = []
    for spec in OBLIGACIONES:
        inf = Informe(
            nombre=spec["nombre"],
            descripcion=spec["descripcion"],
            creador_id=personas["gerencia@andina-demo.co"].id,
            recurrencia_json=spec["recurrencia"],
            datos_generales={"extensiones": spec["extensiones"], "demo": True},
            conjunto_datos_id=conjuntos[spec["conjunto"]].id,
            unidad_id=unidades[spec["unidad"]].id,
            tiene_acuerdo_confidencialidad=spec["confidencial"],
            cargo_autorizador=spec["autorizador"],
            activo=True,
            fecha_creacion=datetime.now() - timedelta(days=30 * meses + 15),
        )
        db.add(inf)
        db.flush()

        # --- El flujo, en UNA sola tabla, sin importar si la parte es
        #     interna o externa. Ese es el punto del modelo.
        if spec.get("origen_externo"):
            db.add(Participacion(informe_id=inf.id,
                                 stakeholder_id=terceros[spec["origen_externo"]].id,
                                 rol=ROL_ORIGEN))
            db.add(Participacion(informe_id=inf.id,
                                 usuario_id=personas[spec["destino_interno"]].id,
                                 rol=ROL_DESTINO))
            db.add(Participacion(informe_id=inf.id,
                                 usuario_id=personas[spec["destino_interno"]].id,
                                 rol=ROL_EJECUTA))
        else:
            db.add(Participacion(informe_id=inf.id,
                                 usuario_id=personas[spec["origen"]].id,
                                 rol=ROL_ORIGEN))
            db.add(Participacion(informe_id=inf.id,
                                 usuario_id=personas[spec["origen"]].id,
                                 rol=ROL_EJECUTA))
            for orden, ext in enumerate(spec.get("destinos_externos", [])):
                db.add(Participacion(informe_id=inf.id,
                                     stakeholder_id=terceros[ext].id,
                                     rol=ROL_DESTINO, orden=orden))
            for orden, email in enumerate(spec.get("destinos_internos", [])):
                db.add(Participacion(informe_id=inf.id,
                                     usuario_id=personas[email].id,
                                     rol=ROL_DESTINO, orden=orden))

        for orden, ext in enumerate(spec.get("usuarios_externos", [])):
            db.add(Participacion(informe_id=inf.id,
                                 stakeholder_id=terceros[ext].id,
                                 rol=ROL_USUARIO, orden=orden))
        db.add(Participacion(informe_id=inf.id,
                             usuario_id=personas[spec["supervisor"]].id,
                             rol=ROL_SUPERVISA))
        # Suplente: la operación no puede depender de una sola persona.
        db.add(Participacion(informe_id=inf.id,
                             usuario_id=personas["auxiliar@andina-demo.co"].id,
                             rol=ROL_EJECUTA, es_principal=False, orden=1))
        informes.append(inf)

    db.flush()
    for inf in informes:
        print(f"   {inf.tipo_flujo:18} {inf.nombre[:52]}")
    return personas, terceros, informes


# =====================================================================
# 3. LA OPERACIÓN: calendario, entregas, evidencias, conversación
# =====================================================================

def operar(db, personas, informes, meses: int):
    print(f"\n▶ 4/5  Operación simulada de {meses} meses")
    rnd = random.Random(SEMILLA)
    hoy = datetime.now()
    desde = hoy - timedelta(days=30 * meses)

    carpeta = os.path.join("storage", "demo_andina")
    os.makedirs(carpeta, exist_ok=True)

    total = {"CUMPLIDO": 0, "EXTEMPORANEO": 0, "INCUMPLIDO": 0, "PENDIENTE": 0}
    archivos = 0
    ajustadas_por_festivo = 0

    for inf in informes:
        ejecutores = [p.usuario for p in inf.partes(ROL_EJECUTA) if not p.es_externa]
        titular = ejecutores[0] if ejecutores else None
        suplente = ejecutores[1] if len(ejecutores) > 1 else titular

        vencimientos = vencimientos_de(inf, desde.date(),
                                       (hoy + timedelta(days=45)).date())
        if not vencimientos:
            hallazgo(f"La regla de «{inf.nombre[:40]}» no genera NINGÚN "
                     f"vencimiento en el planificador real.")

        for v in vencimientos:
            limite = datetime.combine(v.limite, datetime.min.time())
            if not (desde <= limite <= hoy):
                continue
            if v.se_movio:
                ajustadas_por_festivo += 1

            # Quién responde: 1 de cada 8 veces contesta el suplente.
            responsable = suplente if rnd.random() < 0.125 else titular

            # Distribución realista: la mayoría cumple, algunos se atrasan,
            # unos pocos no entregan. Sin esto la demo no enseña nada.
            dado = rnd.random()
            if dado < 0.72:
                estado, entrega = "CUMPLIDO", limite - timedelta(hours=rnd.randint(2, 60))
            elif dado < 0.90:
                estado, entrega = "EXTEMPORANEO", limite + timedelta(hours=rnd.randint(4, 96))
            else:
                estado, entrega = "INCUMPLIDO", None

            ent = EntregaEsperada(
                informe_id=inf.id,
                usuario_id=responsable.id if responsable else None,
                fecha_programada=limite - timedelta(days=3),
                fecha_vencimiento_nominal=datetime.combine(v.nominal,
                                                           datetime.min.time()),
                fecha_limite_teorica=limite,
                tipo_ajuste=v.ajuste,
                fecha_entrega_real=entrega,
                estado=estado,
            )
            db.add(ent)
            db.flush()
            total[estado] += 1

            # Evidencia en disco para los que sí entregaron.
            if entrega:
                ext = (inf.datos_generales or {}).get("extensiones", ["pdf"])[0]
                nombre = f"INF_{inf.id}_{limite:%Y%m%d}.{ext}"
                ruta = os.path.join(carpeta, nombre)
                with open(ruta, "w", encoding="utf-8") as fh:
                    fh.write(f"EVIDENCIA DE DEMOSTRACIÓN — datos sintéticos\n"
                             f"Obligación: {inf.nombre}\n"
                             f"Periodo límite: {limite:%Y-%m-%d}\n"
                             f"Entregado: {entrega:%Y-%m-%d %H:%M}\n"
                             f"Responsable: {responsable.nombre if responsable else '—'}\n")
                ent.ruta_archivo = ruta
                archivos += 1

            # Conversación sobre los incumplidos y atrasados: es donde una
            # auditoría busca la justificación.
            if estado == "EXTEMPORANEO":
                db.add(Comentario(entrega_id=ent.id, autor_id=responsable.id,
                                  texto="Entrega tardía: el consolidado del periodo "
                                        "llegó incompleto y hubo que rehacerlo.",
                                  fecha=entrega))
            elif estado == "INCUMPLIDO":
                sup = [p.usuario for p in inf.partes(ROL_SUPERVISA)]
                if sup:
                    db.add(Comentario(entrega_id=ent.id, autor_id=sup[0].id,
                                      texto="Sin entrega en el plazo. Se solicita "
                                            "explicación formal al responsable.",
                                      fecha=limite + timedelta(days=1)))

    # Entregas futuras: lo que el sistema tiene por delante.
    pendientes = 0
    for inf in informes:
        for v in vencimientos_de(inf, hoy.date(), (hoy + timedelta(days=45)).date()):
            limite = datetime.combine(v.limite, datetime.min.time())
            if hoy < limite <= hoy + timedelta(days=45):
                ejec = [p.usuario for p in inf.partes(ROL_EJECUTA) if not p.es_externa]
                db.add(EntregaEsperada(
                    informe_id=inf.id,
                    usuario_id=ejec[0].id if ejec else None,
                    fecha_programada=limite - timedelta(days=3),
                    fecha_vencimiento_nominal=datetime.combine(
                        v.nominal, datetime.min.time()),
                    fecha_limite_teorica=limite,
                    tipo_ajuste=v.ajuste,
                    estado="PENDIENTE",
                ))
                pendientes += 1
    total["PENDIENTE"] = pendientes

    print(f"   entregas: {sum(total.values())}  "
          f"(cumplidas {total['CUMPLIDO']} · extemporáneas {total['EXTEMPORANEO']} · "
          f"incumplidas {total['INCUMPLIDO']} · pendientes {total['PENDIENTE']})")
    print(f"   evidencias escritas en disco: {archivos}")
    print(f"   fechas movidas por festivo o fin de semana: {ajustadas_por_festivo}")
    if ajustadas_por_festivo == 0:
        hallazgo("Ninguna fecha se ajustó por festivo: revisar que el calendario "
                 "colombiano esté cargado.")
    return total


# =====================================================================
# 4. AVISOS
# =====================================================================

def avisar(db, informes):
    print("\n▶ 5/5  Avisos y notificaciones")
    hoy = datetime.now()
    creadas = 0

    proximas = (db.query(EntregaEsperada)
                .filter(EntregaEsperada.estado == "PENDIENTE",
                        EntregaEsperada.fecha_limite_teorica > hoy,
                        EntregaEsperada.fecha_limite_teorica <= hoy + timedelta(days=3))
                .all())
    for ent in proximas:
        if not ent.usuario_id:
            continue
        horas = int((ent.fecha_limite_teorica - hoy).total_seconds() / 3600)
        db.add(NotificacionApp(usuario_id=ent.usuario_id, tipo="ALERTA",
                               titulo=f"Vence pronto: {ent.informe.nombre[:60]}",
                               mensaje=f"{horas} h restantes",
                               url_destino="/mis-pendientes",
                               entidad_tipo="entrega", entidad_id=ent.id))
        creadas += 1

    incumplidas = (db.query(EntregaEsperada)
                   .filter(EntregaEsperada.estado == "INCUMPLIDO").all())
    for ent in incumplidas:
        for p in ent.informe.partes(ROL_SUPERVISA):
            if p.usuario_id:
                db.add(NotificacionApp(usuario_id=p.usuario_id, tipo="URGENTE",
                                       titulo=f"INCUMPLIDA: {ent.informe.nombre[:60]}",
                                       mensaje="Superó la fecha límite sin entrega",
                                       url_destino="/analytics",
                                       entidad_tipo="entrega", entidad_id=ent.id))
                creadas += 1

    print(f"   notificaciones creadas: {creadas} "
          f"({len(proximas)} por vencer · {len(incumplidas)} incumplidas)")
    return creadas


# =====================================================================
# 5. AUDITORÍA DEL RESULTADO
# =====================================================================

def auditar(db, informes):
    print("\n" + "=" * 72)
    print("AUDITORÍA DE LA OPERACIÓN SIMULADA")
    print("=" * 72)

    print("\nFlujo derivado por obligación (ninguno se escribió a mano):")
    for inf in informes:
        externos = [p.nombre for p in inf.participaciones if p.es_externa]
        print(f"  {inf.tipo_flujo:18} {inf.nombre[:44]:44} ← {', '.join(externos) or 'solo interno'}")

    # Toda obligación necesita alguien que la ejecute y alguien que responda.
    for inf in informes:
        if not inf.partes(ROL_EJECUTA):
            hallazgo(f"«{inf.nombre[:40]}» no tiene quién la ejecute.")
        if not inf.partes(ROL_SUPERVISA):
            hallazgo(f"«{inf.nombre[:40]}» no tiene supervisor.")
        # Recibir y consultar no son lo mismo: un tablero preparado para que un
        # auditor lo consulte no se le ENVÍA a nadie, y la obligación existe
        # igual. Exigir DESTINO marcaba ese caso legítimo como defectuoso.
        if not inf.tiene_quien_la_reciba:
            hallazgo(f"«{inf.nombre[:40]}» no tiene ni destino ni usuario: "
                     f"nadie la recibe ni la consulta.")

    # Entregas sin responsable: se vuelven invisibles para las alertas, que
    # hacen JOIN contra usuario.
    huerfanas = (db.query(EntregaEsperada)
                 .filter(EntregaEsperada.usuario_id.is_(None)).count())
    if huerfanas:
        hallazgo(f"{huerfanas} entrega(s) sin responsable: el servicio de alertas "
                 f"las ignora porque cruza contra usuario.")

    # Confidencialidad declarada sin autorizador definido.
    for inf in informes:
        if inf.tiene_acuerdo_confidencialidad and not inf.cargo_autorizador:
            hallazgo(f"«{inf.nombre[:40]}» es confidencial pero no dice quién "
                     f"autoriza la entrega.")

    # Evidencias referenciadas que no existen en disco.
    faltantes = 0
    for ent in db.query(EntregaEsperada).filter(EntregaEsperada.ruta_archivo.isnot(None)):
        if not os.path.exists(ent.ruta_archivo):
            faltantes += 1
    if faltantes:
        hallazgo(f"{faltantes} entrega(s) apuntan a un archivo que no existe.")

    # Cumplimiento por unidad: la vista que pide un gerente.
    print("\nCumplimiento por obligación:")
    for inf in informes:
        ents = db.query(EntregaEsperada).filter(
            EntregaEsperada.informe_id == inf.id,
            EntregaEsperada.estado != "PENDIENTE").all()
        if not ents:
            continue
        ok = sum(1 for e in ents if e.estado == "CUMPLIDO")
        tasa = 100.0 * ok / len(ents)
        barra = "█" * int(tasa / 5) + "·" * (20 - int(tasa / 5))
        print(f"  {tasa:5.1f}% {barra} {inf.nombre[:42]}")

    print("\n" + "-" * 72)
    if HALLAZGOS:
        print(f"HALLAZGOS ({len(HALLAZGOS)}):")
        for h in HALLAZGOS:
            print(f"  • {h}")
    else:
        print("Sin hallazgos: la operación simulada quedó consistente.")
    print("-" * 72)


# =====================================================================

def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--meses", type=int, default=6, help="meses de historia (def. 6)")
    ap.add_argument("--reset", action="store_true", help="borra y recrea el esquema")
    args = ap.parse_args()

    print("=" * 72)
    print("ANDINA MANUFACTURA S.A.S. — empresa ficticia, datos sintéticos")
    print("=" * 72)

    if args.reset:
        Base.metadata.drop_all(engine)
    Base.metadata.create_all(engine)

    with SessionLocal() as db:
        if db.query(Informe).count() and not args.reset:
            print("\n⚠️  La base ya tiene informes. Usa --reset para empezar limpio.")
            return 1
        personas, terceros, informes = construir(db, args.meses)
        operar(db, personas, informes, args.meses)
        avisar(db, informes)
        db.commit()
        auditar(db, informes)

    print("\nArranca la aplicación y entra con cualquiera de los correos "
          "@andina-demo.co\n(la contraseña de estos usuarios NO es usable: "
          "asígnala desde la administración).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
