"""El asistente de instalación, que es lo primero que ve un cliente.

# El fallo que estas pruebas impiden

`instalar.py` sembraba los datos iniciales **sin crear antes el esquema**. Como
`seed_data.py` inserta filas y las tablas todavía no existen, fallaba SIEMPRE en
una instalación de verdad — es la primera vez que se toca esa base.

Y fallar no fallaba: el error salía como un aviso de pasada, el asistente se
declaraba «Instalación terminada», devolvía **0**, y a continuación anunciaba

    Usuario de ejemplo:   admin@co.co
    Contraseña:           123456

El cliente seguía el manual, leía que todo había ido bien, y no podía entrar en
un sistema que no tenía ni un solo usuario.

Nadie lo había visto porque a mano funciona: quien lo probó ejecutó `init_db`
antes, sin darse cuenta de que el asistente no lo hacía.

# Por qué se prueba con un doble y no ejecutando el asistente

Ejecutarlo de verdad monta un entorno virtual y descarga dependencias: minutos.
Eso lo hace el trabajo `instalación` de integración continua, sobre una copia
limpia. Aquí se comprueba lo que se rompió —el ORDEN y el ABORTO— en un segundo,
que es lo que permite tenerlo en cada corrida.
"""
from __future__ import annotations

import os
import sys

import pytest

os.environ.setdefault("DATABASE_URL", "sqlite:///./test_inst.db")
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import instalar  # noqa: E402


class _Resultado:
    def __init__(self, returncode):
        self.returncode = returncode


class Espia:
    """Registra los comandos en vez de ejecutarlos. Puede fallar a la orden."""

    def __init__(self, falla_en=None):
        self.comandos = []
        self.falla_en = falla_en  # subcadena del comando que debe fallar

    def __call__(self, cmd):
        self.comandos.append(cmd)
        texto = " ".join(cmd)
        if self.falla_en and self.falla_en in texto:
            return _Resultado(1)
        return _Resultado(0)


def test_el_esquema_se_crea_ANTES_de_sembrar():
    """El orden es el arreglo. Invertirlo deja la instalación sin usuarios."""
    espia = Espia()
    instalar.preparar_base_de_datos("py", {}, ejecutar=espia)

    textos = [" ".join(c) for c in espia.comandos]
    assert len(textos) == 2, textos
    assert "app.init_db" in textos[0], textos
    assert "seed_data.py" in textos[1], textos


def test_si_falla_el_esquema_el_instalador_SALE_CON_ERROR():
    """Y no sigue adelante: sembrar sobre una base sin tablas no puede salir bien."""
    espia = Espia(falla_en="app.init_db")
    with pytest.raises(SystemExit) as e:
        instalar.preparar_base_de_datos("py", {}, ejecutar=espia)
    assert e.value.code != 0
    assert len(espia.comandos) == 1, "no debe intentar sembrar tras fallar el esquema"


def test_si_falla_la_SIEMBRA_el_instalador_SALE_CON_ERROR():
    """Este es el que estaba roto.

    Antes era un aviso: el asistente terminaba «bien», devolvía 0 y anunciaba
    unas credenciales que no existían. Un sistema instalado sin usuarios es un
    sistema en el que no se puede entrar, y el cliente no tiene forma de saber
    que el fallo estaba escrito tres líneas más arriba.
    """
    espia = Espia(falla_en="seed_data.py")
    with pytest.raises(SystemExit) as e:
        instalar.preparar_base_de_datos("py", {}, ejecutar=espia)
    assert e.value.code != 0


def test_cuando_todo_va_bien_NO_sale_con_error():
    """Un asistente que aborta siempre es tan inútil como uno que nunca aborta."""
    instalar.preparar_base_de_datos("py", {}, ejecutar=Espia())


def test_el_mensaje_de_fallo_de_la_siembra_dice_que_NO_HAY_USUARIOS():
    """«Falló la siembra» no le dice nada a quien instala. «El sistema quedó sin
    ningún usuario, así que no se puede entrar» sí, y es lo que ocurre."""
    _, _, fallo, ayuda = instalar.PASOS_BASE_DE_DATOS[1]
    assert "usuarios" in (fallo + ayuda).lower()
    assert "no se puede entrar" in ayuda.lower() or "SIN NINGÚN USUARIO" in ayuda


def test_los_pasos_estan_declarados_en_orden():
    """La lista es la fuente del orden; si alguien la reordena, esto lo dice."""
    argumentos = [" ".join(p[1]) for p in instalar.PASOS_BASE_DE_DATOS]
    assert argumentos == ["-m app.init_db", "seed_data.py"], argumentos
