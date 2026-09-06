from __future__ import annotations

import time

from celery import shared_task
from django.db import transaction


@shared_task(name="demoapp.ping")
def ping() -> str:
    """Lightweight Beat / inspect target — returns quickly."""
    return "pong"


@shared_task(name="demoapp.busy_work", bind=True)
def busy_work(self, seconds: float = 2.0) -> str:
    """Sleepy task used to exercise Celery --autoscale under burst load."""
    time.sleep(max(0.0, float(seconds)))
    return f"done:{self.request.id}"


@shared_task(name="demoapp.process_work_item", bind=True)
def process_work_item(self, work_id: str, seconds: float | None = None) -> str:
    """Load a WorkItem row, mark running → done/failed, sleep `seconds`."""
    from demoapp.models import WorkItem

    with transaction.atomic():
        item = WorkItem.objects.select_for_update().get(pk=work_id)
        item.status = WorkItem.Status.RUNNING
        item.error = ""
        item.save(update_fields=["status", "error", "updated_at"])
        duration = float(seconds if seconds is not None else item.seconds)

    try:
        time.sleep(max(0.0, duration))
        result = f"done:{self.request.id}"
        WorkItem.objects.filter(pk=work_id).update(
            status=WorkItem.Status.DONE,
            result=result,
            error="",
        )
        return result
    except Exception as exc:  # noqa: BLE001 — surface to row for demo API
        WorkItem.objects.filter(pk=work_id).update(
            status=WorkItem.Status.FAILED,
            error=str(exc),
        )
        raise
