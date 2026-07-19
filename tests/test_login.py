"""El inicio de sesión y la migración de contraseñas.

Lo que se fija aquí es la propiedad que hace posible cambiar de algoritmo sin
echar a nadie: un hash es de una sola vía, así que la contraseña solo existe en
claro en el instante en que el usuario la escribe. Si no se recifra ahí, no se
recifra nunca.
"""
import os
import sys

import bcrypt
import pytest

os.environ.setdefault("DEPLOYMENT_PROFILE", "standalone")
os.environ["DATABASE_URL"] = "sqlite:///:memory:"
os.environ["EMAIL_ENABLED"] = "false"
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from app.core.database import Base, SessionLocal, engine  # noqa: E402
from app.models.meta_tables import Usuario  # noqa: E402
from app.services.auth import CADENA, AuthService  # noqa: E402


@pytest.fixture
def db():
    Base.metadata.create_all(engine)
    s = SessionLocal()
    yield s
    s.close()
    Base.metadata.drop_all(engine)


def _bcrypt_heredado(clave: str) -> str:
    """Un hash tal como lo escribía el sistema antes de esta migración."""
    return bcrypt.hashpw(clave.encode(), bcrypt.gensalt(rounds=12)).decode()


def _crear(db, email, password_hash, activo=True):
    u = Usuario(nombre="Prueba", email=email, password_hash=password_hash, activo=activo)
    db.add(u)
    db.commit()
    return u


# --- Lo nuevo se escribe con el esquema fuerte -------------------------------

def test_las_contrasenas_nuevas_son_argon2id():
    h = AuthService.get_password_hash("secreta")
    assert h.startswith("$argon2id$")
    assert CADENA.esquema_de(h) == "argon2id"


def test_una_contrasena_nueva_no_se_recifra_al_entrar(db):
    _crear(db, "nueva@x.co", AuthService.get_password_hash("secreta"))
    assert CADENA.verificar("secreta", AuthService.get_password_hash("secreta")).hash_nuevo is None


# --- Nadie se queda fuera ----------------------------------------------------

def test_el_usuario_heredado_en_bcrypt_todavia_entra(db):
    _crear(db, "viejo@x.co", _bcrypt_heredado("secreta"))
    assert AuthService.authenticate_user("viejo@x.co", "secreta") is not None


def test_al_entrar_su_hash_QUEDA_MIGRADO_EN_LA_BASE(db):
    """La prueba central: no basta con devolver el hash nuevo, hay que guardarlo."""
    u = _crear(db, "viejo@x.co", _bcrypt_heredado("secreta"))
    assert u.password_hash.startswith("$2b$")

    AuthService.authenticate_user("viejo@x.co", "secreta")

    db.expire_all()
    guardado = db.query(Usuario).filter_by(email="viejo@x.co").one().password_hash
    assert guardado.startswith("$argon2id$"), "el hash migrado debe persistirse"


def test_despues_de_migrar_la_misma_clave_sigue_sirviendo(db):
    _crear(db, "viejo@x.co", _bcrypt_heredado("secreta"))
    AuthService.authenticate_user("viejo@x.co", "secreta")
    db.expire_all()
    assert AuthService.authenticate_user("viejo@x.co", "secreta") is not None


# --- Y lo que debe seguir fallando -------------------------------------------

def test_la_clave_incorrecta_no_entra_ni_toca_el_hash(db):
    original = _bcrypt_heredado("secreta")
    _crear(db, "viejo@x.co", original)

    assert AuthService.authenticate_user("viejo@x.co", "equivocada") is None

    db.expire_all()
    assert db.query(Usuario).filter_by(email="viejo@x.co").one().password_hash == original, \
        "una clave errónea jamás debe reescribir el hash bueno"


def test_el_usuario_inactivo_no_entra(db):
    _crear(db, "baja@x.co", AuthService.get_password_hash("secreta"), activo=False)
    assert AuthService.authenticate_user("baja@x.co", "secreta") is None


def test_el_usuario_inexistente_no_entra(db):
    assert AuthService.authenticate_user("fantasma@x.co", "secreta") is None


def test_un_hash_irreconocible_niega_el_acceso_sin_reventar(db):
    """Los usuarios de demostración se crean con un marcador que no es un hash.
    Debe negar el paso, no tumbar la aplicación con una excepción."""
    _crear(db, "demo@x.co", "demo-no-usable")
    assert AuthService.authenticate_user("demo@x.co", "lo-que-sea") is None


# --- El informe de avance ----------------------------------------------------

def test_el_avance_delata_la_base_sin_migrar(db):
    for i in range(3):
        _crear(db, f"viejo{i}@x.co", _bcrypt_heredado("secreta"))

    avance = AuthService.avance_migracion()
    assert avance.total == 3
    assert avance.pendientes == 3
    assert not avance.completo


def test_el_avance_se_cierra_cuando_todos_han_entrado(db):
    for i in range(3):
        _crear(db, f"viejo{i}@x.co", _bcrypt_heredado("secreta"))
    for i in range(3):
        AuthService.authenticate_user(f"viejo{i}@x.co", "secreta")

    avance = AuthService.avance_migracion()
    assert avance.pendientes == 0
    assert avance.completo, "con todos migrados, ya se puede retirar bcrypt de la cadena"


def test_cien_inicios_de_sesion_migran_una_sola_vez(db):
    """Recifrar en cada entrada regalaría un Argon2 completo por inicio de
    sesión. Debe ocurrir una vez y no volver a ocurrir."""
    _crear(db, "viejo@x.co", _bcrypt_heredado("secreta"))

    AuthService.authenticate_user("viejo@x.co", "secreta")
    db.expire_all()
    tras_migrar = db.query(Usuario).filter_by(email="viejo@x.co").one().password_hash

    for _ in range(100):
        assert AuthService.authenticate_user("viejo@x.co", "secreta") is not None

    db.expire_all()
    assert db.query(Usuario).filter_by(email="viejo@x.co").one().password_hash == tras_migrar, \
        "el hash no debe cambiar en cada entrada una vez migrado"
