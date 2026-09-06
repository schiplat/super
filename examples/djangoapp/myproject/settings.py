from __future__ import annotations

import os
from pathlib import Path
from urllib.parse import unquote, urlparse

BASE_DIR = Path(__file__).resolve().parent.parent


def _require(name: str) -> str:
    value = os.environ.get(name)
    if not value:
        raise RuntimeError(f"missing required env var: {name}")
    return value


def _database_from_url(url: str) -> dict:
    u = urlparse(url)
    if u.scheme not in ("postgres", "postgresql"):
        raise RuntimeError(f"unsupported DATABASE_URL scheme: {u.scheme!r}")
    return {
        "ENGINE": "django.db.backends.postgresql",
        "NAME": unquote((u.path or "/").lstrip("/")) or "djangoapp",
        "USER": unquote(u.username or ""),
        "PASSWORD": unquote(u.password or ""),
        "HOST": u.hostname or "127.0.0.1",
        "PORT": str(u.port or 5432),
    }


SECRET_KEY = _require("DJANGO_SECRET_KEY")
DEBUG = os.environ.get("DJANGO_DEBUG", "0") in ("1", "true", "True", "yes")
ALLOWED_HOSTS = [
    h.strip() for h in os.environ.get("DJANGO_ALLOWED_HOSTS", "*").split(",") if h.strip()
]

INSTALLED_APPS = [
    "django.contrib.contenttypes",
    "django.contrib.auth",
    "django.contrib.sessions",
    "demoapp.apps.DemoappConfig",
]

MIDDLEWARE = [
    "django.middleware.security.SecurityMiddleware",
    "django.contrib.sessions.middleware.SessionMiddleware",
    "django.middleware.common.CommonMiddleware",
]

ROOT_URLCONF = "myproject.urls"
WSGI_APPLICATION = "myproject.wsgi.application"

TEMPLATES = [
    {
        "BACKEND": "django.template.backends.django.DjangoTemplates",
        "DIRS": [],
        "APP_DIRS": True,
        "OPTIONS": {"context_processors": []},
    }
]

DATABASES = {"default": _database_from_url(_require("DATABASE_URL"))}

LANGUAGE_CODE = "en-us"
TIME_ZONE = "UTC"
USE_I18N = True
USE_TZ = True

DEFAULT_AUTO_FIELD = "django.db.models.BigAutoField"

# Celery (read by celery.py via namespace="CELERY")
CELERY_BROKER_URL = _require("CELERY_BROKER_URL")
CELERY_RESULT_BACKEND = CELERY_BROKER_URL
CELERY_TASK_TRACK_STARTED = True
CELERY_TASK_TIME_LIMIT = 120
CELERY_WORKER_PREFETCH_MULTIPLIER = 1
CELERY_BEAT_SCHEDULE = {
    "demo-heartbeat": {
        "task": "demoapp.ping",
        "schedule": 60.0,
    },
}
