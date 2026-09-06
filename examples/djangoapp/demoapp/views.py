from __future__ import annotations

from django.db import connection
from django.http import HttpRequest, HttpResponse, JsonResponse
from django.views.decorators.http import require_GET, require_http_methods

from demoapp.models import WorkItem
from demoapp.tasks import process_work_item


def home(_request: HttpRequest) -> HttpResponse:
    return HttpResponse(
        "Super OSS djangoapp example\n"
        "  GET  /health/\n"
        "  POST /api/work/enqueue/  (JSON {\"seconds\": 2} or form)\n"
        "  GET  /api/work/<uuid>/\n",
        content_type="text/plain; charset=utf-8",
    )


def _redis_ping() -> None:
    import redis
    from django.conf import settings

    client = redis.from_url(settings.CELERY_BROKER_URL)
    try:
        if client.ping() is not True:
            raise RuntimeError("redis PING failed")
    finally:
        client.close()


@require_GET
def health(_request: HttpRequest) -> JsonResponse:
    """Readiness: Postgres + Redis (broker). Used by Super HTTP probe on `web`."""
    db_ok = False
    redis_ok = False
    try:
        connection.ensure_connection()
        db_ok = True
    except Exception:  # noqa: BLE001 — probe must return 503, not crash
        pass
    try:
        _redis_ping()
        redis_ok = True
    except Exception:  # noqa: BLE001
        pass
    ok = db_ok and redis_ok
    return JsonResponse(
        {"status": "ok" if ok else "degraded", "db": db_ok, "redis": redis_ok},
        status=200 if ok else 503,
    )


@require_http_methods(["GET", "POST"])
def enqueue_work(request: HttpRequest) -> JsonResponse:
    """Enqueue a WorkItem for Celery. GET ?seconds= for easy demo curls."""
    if request.method == "POST" and request.content_type and "json" in request.content_type:
        import json

        body = json.loads(request.body.decode("utf-8") or "{}")
        seconds = float(body.get("seconds", 2))
    else:
        seconds = float(request.GET.get("seconds") or request.POST.get("seconds") or 2)

    seconds = max(0.0, min(seconds, 60.0))
    item = WorkItem.objects.create(seconds=seconds, status=WorkItem.Status.PENDING)
    async_result = process_work_item.delay(str(item.id), seconds)
    return JsonResponse(
        {
            "id": str(item.id),
            "status": item.status,
            "seconds": item.seconds,
            "task_id": async_result.id,
        },
        status=202,
    )


@require_GET
def work_status(_request: HttpRequest, work_id: str) -> JsonResponse:
    try:
        item = WorkItem.objects.get(pk=work_id)
    except (WorkItem.DoesNotExist, ValueError):
        return JsonResponse({"error": "not_found"}, status=404)
    return JsonResponse(
        {
            "id": str(item.id),
            "status": item.status,
            "seconds": item.seconds,
            "result": item.result,
            "error": item.error,
            "created_at": item.created_at.isoformat(),
            "updated_at": item.updated_at.isoformat(),
        }
    )
