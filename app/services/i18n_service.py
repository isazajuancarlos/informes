"""
Internationalization service — provides t() function for UI string translation.
"""
import json
from pathlib import Path
from app.core.config import IDIOMA_DEFECTO

_CACHE: dict = {}
_I18N_DIR = Path(__file__).resolve().parent.parent / "i18n"


def _load(lang: str) -> dict:
    if lang not in _CACHE:
        path = _I18N_DIR / f"{lang}.json"
        _CACHE[lang] = json.loads(path.read_text("utf-8")) if path.exists() else {}
    return _CACHE[lang]


def t(key: str, **kwargs) -> str:
    """Translate a dot-notation key for the current user's language."""
    try:
        from nicegui import app as nicegui_app
        lang = nicegui_app.storage.user.get("idioma", IDIOMA_DEFECTO)
    except Exception:
        lang = IDIOMA_DEFECTO

    strings = _load(lang)
    value = strings
    for part in key.split("."):
        value = value.get(part) if isinstance(value, dict) else None
        if value is None:
            return key  # graceful fallback

    if not isinstance(value, str):
        return key
    try:
        return value.format(**kwargs) if kwargs else value
    except (KeyError, ValueError):
        return value


def invalidar_cache():
    """Call after saving new settings to reload translations."""
    _CACHE.clear()
